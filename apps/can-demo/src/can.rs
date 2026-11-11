//! Minimal SocketCAN (CAN_RAW) wrapper over libc, for classic CAN and CAN FD.
//!
//! The kernel has two frame layouts (linux/can.h):
//! - `struct can_frame`, 16 bytes: classic CAN, up to 8 data bytes
//! - `struct canfd_frame`, 72 bytes: CAN FD, up to 64 data bytes
//!
//! A CAN_RAW socket only exchanges `canfd_frame`s after the socket option
//! `CAN_RAW_FD_FRAMES` is switched on. With it on, `read()` returns either 16
//! bytes (a classic frame) or 72 bytes (an FD frame), and `write()` accepts
//! either size. Sending an FD frame also needs an interface whose MTU is 72
//! (`CANFD_MTU`, a controller with `fd on`) or more (a vcan with CAN XL MTU).

use std::ffi::CString;
use std::io;
use std::mem;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::Duration;

const AF_CAN: libc::c_int = 29;
const PF_CAN: libc::c_int = AF_CAN;
const CAN_RAW: libc::c_int = 1;
const SOL_CAN_BASE: libc::c_int = 100;
const SOL_CAN_RAW: libc::c_int = SOL_CAN_BASE + CAN_RAW;
const CAN_RAW_FD_FRAMES: libc::c_int = 5;

pub const CAN_EFF_FLAG: u32 = 0x8000_0000;
pub const CAN_ERR_FLAG: u32 = 0x2000_0000;
pub const CAN_SFF_MASK: u32 = 0x0000_07FF;
pub const CAN_EFF_MASK: u32 = 0x1FFF_FFFF;

/// Bytes on the socket for a classic frame / an FD frame (also the interface MTUs).
pub const CAN_MTU: usize = 16;
pub const CANFD_MTU: usize = 72;
pub const CAN_MAX_DLEN: usize = 8;
pub const CANFD_MAX_DLEN: usize = 64;
/// `canfd_frame.flags`: bit rate switch (data phase at the faster data bit rate).
pub const CANFD_BRS: u8 = 0x01;

/// The payload lengths a CAN FD frame can have: the 4-bit DLC maps 9..15 to
/// 12, 16, 20, 24, 32, 48, 64 bytes.
pub const CANFD_LENGTHS: [usize; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 20, 24, 32, 48, 64];

/// `struct can_frame` from linux/can.h (16 bytes).
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct RawCanFrame {
    can_id: u32,
    len: u8,
    _pad: u8,
    _res0: u8,
    _len8_dlc: u8,
    data: [u8; CAN_MAX_DLEN],
}

/// `struct canfd_frame` from linux/can.h (72 bytes).
#[repr(C)]
#[derive(Clone, Copy)]
struct RawCanFdFrame {
    can_id: u32,
    len: u8,
    flags: u8,
    _res0: u8,
    _res1: u8,
    data: [u8; CANFD_MAX_DLEN],
}

/// A classic or FD data frame as the application sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    can_id: u32,
    len: u8,
    flags: u8,
    fd: bool,
    data: [u8; CANFD_MAX_DLEN],
}

fn raw_id(id: u32) -> Option<u32> {
    if id > CAN_EFF_MASK {
        return None;
    }
    // IDs above 0x7FF are sent as extended (29-bit) IDs.
    Some(if id > CAN_SFF_MASK { id | CAN_EFF_FLAG } else { id })
}

impl Frame {
    /// A classic CAN data frame (0..8 bytes).
    pub fn classic(id: u32, payload: &[u8]) -> Option<Self> {
        if payload.len() > CAN_MAX_DLEN {
            return None;
        }
        Self::build(raw_id(id)?, payload, 0, false)
    }

    /// A CAN FD data frame. The length must be one of `CANFD_LENGTHS`; with
    /// `brs` the data phase uses the faster data bit rate.
    pub fn fd(id: u32, payload: &[u8], brs: bool) -> Option<Self> {
        if !CANFD_LENGTHS.contains(&payload.len()) {
            return None;
        }
        Self::build(raw_id(id)?, payload, if brs { CANFD_BRS } else { 0 }, true)
    }

    fn build(can_id: u32, payload: &[u8], flags: u8, fd: bool) -> Option<Self> {
        let mut data = [0u8; CANFD_MAX_DLEN];
        data[..payload.len()].copy_from_slice(payload);
        Some(Self { can_id, len: payload.len() as u8, flags, fd, data })
    }

    pub fn id(&self) -> u32 {
        if self.is_extended() {
            self.can_id & CAN_EFF_MASK
        } else {
            self.can_id & CAN_SFF_MASK
        }
    }

    pub fn is_extended(&self) -> bool {
        self.can_id & CAN_EFF_FLAG != 0
    }

    pub fn is_error(&self) -> bool {
        self.can_id & CAN_ERR_FLAG != 0
    }

    pub fn is_fd(&self) -> bool {
        self.fd
    }

    pub fn flags(&self) -> u8 {
        self.flags
    }

    pub fn payload(&self) -> &[u8] {
        let max = if self.fd { CANFD_MAX_DLEN } else { CAN_MAX_DLEN };
        &self.data[..usize::from(self.len).min(max)]
    }

    /// The bytes written to the socket: 16 for classic, 72 for FD.
    fn to_wire(self) -> ([u8; CANFD_MTU], usize) {
        let mut buf = [0u8; CANFD_MTU];
        if self.fd {
            let raw = RawCanFdFrame {
                can_id: self.can_id,
                len: self.len,
                flags: self.flags,
                _res0: 0,
                _res1: 0,
                data: self.data,
            };
            // SAFETY: RawCanFdFrame is repr(C), 72 bytes, plain integers.
            unsafe { std::ptr::write_unaligned(buf.as_mut_ptr() as *mut RawCanFdFrame, raw) };
            (buf, CANFD_MTU)
        } else {
            let mut data = [0u8; CAN_MAX_DLEN];
            data.copy_from_slice(&self.data[..CAN_MAX_DLEN]);
            let raw = RawCanFrame { can_id: self.can_id, len: self.len, data, ..Default::default() };
            // SAFETY: RawCanFrame is repr(C), 16 bytes, plain integers.
            unsafe { std::ptr::write_unaligned(buf.as_mut_ptr() as *mut RawCanFrame, raw) };
            (buf, CAN_MTU)
        }
    }

    /// Parses what `read()` returned: 16 bytes (classic) or 72 bytes (FD).
    fn from_wire(buf: &[u8]) -> io::Result<Self> {
        match buf.len() {
            CAN_MTU => {
                // SAFETY: 16 bytes are there; any bit pattern is a valid RawCanFrame.
                let raw: RawCanFrame = unsafe { std::ptr::read_unaligned(buf.as_ptr() as *const RawCanFrame) };
                let mut data = [0u8; CANFD_MAX_DLEN];
                data[..CAN_MAX_DLEN].copy_from_slice(&raw.data);
                Ok(Self { can_id: raw.can_id, len: raw.len.min(8), flags: 0, fd: false, data })
            }
            CANFD_MTU => {
                // SAFETY: 72 bytes are there; any bit pattern is a valid RawCanFdFrame.
                let raw: RawCanFdFrame = unsafe { std::ptr::read_unaligned(buf.as_ptr() as *const RawCanFdFrame) };
                Ok(Self { can_id: raw.can_id, len: raw.len.min(64), flags: raw.flags, fd: true, data: raw.data })
            }
            n => Err(io::Error::new(io::ErrorKind::InvalidData, format!("unexpected CAN read of {n} bytes"))),
        }
    }
}

/// `struct sockaddr_can`; only the interface index matters for CAN_RAW.
#[repr(C)]
struct SockaddrCan {
    can_family: libc::sa_family_t,
    can_ifindex: libc::c_int,
    _addr: [u8; 16],
}

/// MTU of a network interface, from sysfs: 16 = classic CAN only, 72 = CAN FD,
/// 2060 = CAN XL capable (a vcan created with that MTU; it also carries FD).
pub fn interface_mtu(iface: &str) -> io::Result<usize> {
    let s = std::fs::read_to_string(format!("/sys/class/net/{iface}/mtu"))?;
    s.trim().parse().map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad mtu"))
}

pub struct CanSocket {
    fd: OwnedFd,
}

impl CanSocket {
    /// Opens a CAN_RAW socket bound to `iface` (e.g. "vcan0", "can0") with
    /// CAN FD frames enabled, so it receives both classic and FD frames.
    pub fn open(iface: &str) -> io::Result<Self> {
        let name = CString::new(iface).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
        // SAFETY: plain libc calls; return values are checked.
        let ifindex = unsafe { libc::if_nametoindex(name.as_ptr()) };
        if ifindex == 0 {
            return Err(io::Error::last_os_error());
        }
        let raw = unsafe { libc::socket(PF_CAN, libc::SOCK_RAW | libc::SOCK_CLOEXEC, CAN_RAW) };
        if raw < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: raw is a fresh, valid descriptor that we now own.
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };

        let on: libc::c_int = 1;
        let rc = unsafe {
            libc::setsockopt(
                fd.as_raw_fd(),
                SOL_CAN_RAW,
                CAN_RAW_FD_FRAMES,
                &on as *const libc::c_int as *const libc::c_void,
                mem::size_of::<libc::c_int>() as libc::socklen_t,
            )
        };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }

        let addr = SockaddrCan {
            can_family: AF_CAN as libc::sa_family_t,
            can_ifindex: ifindex as libc::c_int,
            _addr: [0; 16],
        };
        let rc = unsafe {
            libc::bind(
                fd.as_raw_fd(),
                &addr as *const SockaddrCan as *const libc::sockaddr,
                mem::size_of::<SockaddrCan>() as libc::socklen_t,
            )
        };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { fd })
    }

    pub fn set_read_timeout(&self, timeout: Duration) -> io::Result<()> {
        let tv = libc::timeval {
            tv_sec: timeout.as_secs() as libc::time_t,
            tv_usec: timeout.subsec_micros() as libc::suseconds_t,
        };
        let rc = unsafe {
            libc::setsockopt(
                self.fd.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_RCVTIMEO,
                &tv as *const libc::timeval as *const libc::c_void,
                mem::size_of::<libc::timeval>() as libc::socklen_t,
            )
        };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Sends one frame. An FD frame on a classic-only interface fails with
    /// EINVAL; a full TX queue gives ENOBUFS.
    pub fn write_frame(&self, frame: &Frame) -> io::Result<()> {
        let (buf, size) = frame.to_wire();
        let n = unsafe { libc::write(self.fd.as_raw_fd(), buf.as_ptr() as *const libc::c_void, size) };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        if n as usize != size {
            return Err(io::Error::new(io::ErrorKind::WriteZero, "short CAN write"));
        }
        Ok(())
    }

    /// Blocks until a frame arrives or the read timeout expires
    /// (`ErrorKind::WouldBlock` / `TimedOut`).
    pub fn read_frame(&self) -> io::Result<Frame> {
        let mut buf = [0u8; CANFD_MTU];
        let n = unsafe { libc::read(self.fd.as_raw_fd(), buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        Frame::from_wire(&buf[..n as usize])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_layout_matches_kernel() {
        assert_eq!(mem::size_of::<RawCanFrame>(), CAN_MTU);
        assert_eq!(mem::size_of::<RawCanFdFrame>(), CANFD_MTU);
        assert_eq!(mem::size_of::<SockaddrCan>(), 24);
    }

    #[test]
    fn standard_and_extended_ids() {
        let f = Frame::classic(0x123, &[1, 2, 3]).unwrap();
        assert!(!f.is_extended() && !f.is_fd());
        assert_eq!(f.id(), 0x123);
        assert_eq!(f.payload(), &[1, 2, 3]);

        let e = Frame::classic(0x1ABC_DEF0, &[]).unwrap();
        assert!(e.is_extended());
        assert_eq!(e.id(), 0x1ABC_DEF0);
        assert!(e.payload().is_empty());
    }

    #[test]
    fn rejects_oversized() {
        assert!(Frame::classic(0x1, &[0; 9]).is_none());
        assert!(Frame::classic(0x2000_0000, &[]).is_none());
        assert!(Frame::fd(0x1, &[0; 65], true).is_none());
    }

    #[test]
    fn fd_lengths_follow_the_dlc_table() {
        for n in 0..=64 {
            assert_eq!(Frame::fd(0x200, &vec![0; n], true).is_some(), CANFD_LENGTHS.contains(&n), "len {n}");
        }
    }

    #[test]
    fn wire_round_trip() {
        let c = Frame::classic(0x101, &[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        let (buf, n) = c.to_wire();
        assert_eq!(n, CAN_MTU);
        assert_eq!(Frame::from_wire(&buf[..n]).unwrap(), c);

        let payload: Vec<u8> = (0..64).collect();
        let f = Frame::fd(0x1234_5678, &payload, true).unwrap();
        let (buf, n) = f.to_wire();
        assert_eq!(n, CANFD_MTU);
        // can_id in host byte order, then len, flags (BRS), two reserved bytes.
        assert_eq!(&buf[..4], &(0x1234_5678u32 | CAN_EFF_FLAG).to_ne_bytes());
        assert_eq!(buf[4], 64);
        assert_eq!(buf[5], CANFD_BRS);
        let back = Frame::from_wire(&buf[..n]).unwrap();
        assert_eq!(back, f);
        assert_eq!(back.payload(), &payload[..]);

        assert!(Frame::from_wire(&buf[..20]).is_err());
    }

    /// Sends a classic and an FD frame on vcan0 and reads both back with a
    /// second socket (SocketCAN loops local traffic back to other sockets).
    /// Runs on the device (ptest) and on a host with vcan0; skipped elsewhere.
    #[test]
    fn vcan0_classic_and_fd_round_trip() {
        let iface = "vcan0";
        let Ok(mtu) = interface_mtu(iface) else {
            eprintln!("skipped: no {iface}");
            return;
        };
        let (tx, rx) = match (CanSocket::open(iface), CanSocket::open(iface)) {
            (Ok(t), Ok(r)) => (t, r),
            (Err(e), _) | (_, Err(e)) => {
                eprintln!("skipped: cannot open {iface}: {e}");
                return;
            }
        };
        rx.set_read_timeout(Duration::from_millis(200)).unwrap();
        let id = 0x7E5; // unused by the demo and by the BMS project (0x410..0x430)
        let classic = Frame::classic(id, b"PTEST-C1").unwrap();
        let fd = Frame::fd(id, &[0xA5; 48], true).unwrap();
        tx.write_frame(&classic).unwrap();
        if mtu >= CANFD_MTU {
            tx.write_frame(&fd).unwrap();
        } else {
            assert!(tx.write_frame(&fd).is_err(), "FD frame accepted on a classic interface");
        }
        let mut seen = (false, mtu < CANFD_MTU);
        for _ in 0..50 {
            match rx.read_frame() {
                Ok(f) if f.id() == id && !f.is_fd() => {
                    assert_eq!(f.payload(), b"PTEST-C1");
                    seen.0 = true;
                }
                Ok(f) if f.id() == id => {
                    assert_eq!(f.payload(), &[0xA5; 48][..]);
                    assert_ne!(f.flags() & CANFD_BRS, 0);
                    seen.1 = true;
                }
                _ => {}
            }
            if seen.0 && seen.1 {
                return;
            }
        }
        panic!("frames not looped back on {iface} (classic seen: {}, fd seen: {})", seen.0, seen.1);
    }
}
