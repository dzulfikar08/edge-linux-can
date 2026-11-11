//! Minimal SocketCAN (CAN_RAW) wrapper over libc.
//!
//! Only classic CAN frames (8 data bytes) are handled. The kernel structs are
//! declared here so the layout is explicit; see linux/can.h.

use std::ffi::CString;
use std::io;
use std::mem;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::Duration;

const AF_CAN: libc::c_int = 29;
const PF_CAN: libc::c_int = AF_CAN;
const CAN_RAW: libc::c_int = 1;

pub const CAN_EFF_FLAG: u32 = 0x8000_0000;
pub const CAN_ERR_FLAG: u32 = 0x2000_0000;
pub const CAN_SFF_MASK: u32 = 0x0000_07FF;
pub const CAN_EFF_MASK: u32 = 0x1FFF_FFFF;

/// `struct can_frame` from linux/can.h (16 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CanFrame {
    pub can_id: u32,
    pub len: u8,
    _pad: u8,
    _res0: u8,
    _len8_dlc: u8,
    pub data: [u8; 8],
}

impl CanFrame {
    /// Builds a data frame. IDs above 0x7FF are sent as extended (29-bit) IDs.
    pub fn new(id: u32, payload: &[u8]) -> Option<Self> {
        if payload.len() > 8 || id > CAN_EFF_MASK {
            return None;
        }
        let mut data = [0u8; 8];
        data[..payload.len()].copy_from_slice(payload);
        let can_id = if id > CAN_SFF_MASK { id | CAN_EFF_FLAG } else { id };
        Some(Self { can_id, len: payload.len() as u8, data, ..Default::default() })
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

    pub fn payload(&self) -> &[u8] {
        &self.data[..usize::from(self.len.min(8))]
    }
}

/// `struct sockaddr_can`; only the interface index matters for CAN_RAW.
#[repr(C)]
struct SockaddrCan {
    can_family: libc::sa_family_t,
    can_ifindex: libc::c_int,
    _addr: [u8; 16],
}

pub struct CanSocket {
    fd: OwnedFd,
}

impl CanSocket {
    /// Opens a CAN_RAW socket bound to `iface` (e.g. "vcan0", "can0").
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

    pub fn write_frame(&self, frame: &CanFrame) -> io::Result<()> {
        let n = unsafe {
            libc::write(
                self.fd.as_raw_fd(),
                frame as *const CanFrame as *const libc::c_void,
                mem::size_of::<CanFrame>(),
            )
        };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        if n as usize != mem::size_of::<CanFrame>() {
            return Err(io::Error::new(io::ErrorKind::WriteZero, "short CAN write"));
        }
        Ok(())
    }

    /// Blocks until a frame arrives or the read timeout expires
    /// (`ErrorKind::WouldBlock` / `TimedOut`).
    pub fn read_frame(&self) -> io::Result<CanFrame> {
        let mut frame = CanFrame::default();
        let n = unsafe {
            libc::read(
                self.fd.as_raw_fd(),
                &mut frame as *mut CanFrame as *mut libc::c_void,
                mem::size_of::<CanFrame>(),
            )
        };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        if n as usize != mem::size_of::<CanFrame>() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "short CAN read"));
        }
        Ok(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_layout_matches_kernel() {
        assert_eq!(mem::size_of::<CanFrame>(), 16);
        assert_eq!(mem::size_of::<SockaddrCan>(), 24);
    }

    #[test]
    fn standard_and_extended_ids() {
        let f = CanFrame::new(0x123, &[1, 2, 3]).unwrap();
        assert!(!f.is_extended());
        assert_eq!(f.id(), 0x123);
        assert_eq!(f.payload(), &[1, 2, 3]);

        let e = CanFrame::new(0x1ABC_DEF0, &[]).unwrap();
        assert!(e.is_extended());
        assert_eq!(e.id(), 0x1ABC_DEF0);
        assert!(e.payload().is_empty());
    }

    #[test]
    fn rejects_oversized() {
        assert!(CanFrame::new(0x1, &[0; 9]).is_none());
        assert!(CanFrame::new(0x2000_0000, &[]).is_none());
    }
}
