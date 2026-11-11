//! can-demo: a heartbeat sender plus a listener that counts frames on a
//! SocketCAN interface. Built to run under systemd: logs go to stdout with
//! sd-daemon priority prefixes (picked up by journald), and it reports
//! READY=1 / WATCHDOG=1 over $NOTIFY_SOCKET when that is set.
//!
//! Frames it sends (see docs/can-demo.dbc):
//! - heartbeat, classic CAN, 8 bytes: seq (u32 LE) + "HBT1"
//! - telemetry, CAN FD with bit rate switch, 64 bytes (only when the
//!   interface can carry FD frames): seq, "FDT1", uptime, frames received,
//!   start count, and a 32-byte counting pattern that receivers can check.

mod can;
mod notify;
mod state;

use std::collections::BTreeMap;
use std::io;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use can::{CanSocket, Frame, CANFD_MTU};

#[derive(Debug, Clone, PartialEq)]
struct Config {
    iface: String,
    heartbeat_id: u32,
    /// CAN FD telemetry: auto = send it when the interface MTU allows FD.
    fd: FdMode,
    /// Telemetry id; None = heartbeat id + 0x100.
    fd_id: Option<u32>,
    interval: Duration,
    report_every: Duration,
    send: bool,
    listen: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            iface: "vcan0".into(),
            heartbeat_id: 0x100,
            fd: FdMode::Auto,
            fd_id: None,
            interval: Duration::from_millis(1000),
            report_every: Duration::from_secs(10),
            send: true,
            listen: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FdMode {
    Auto,
    On,
    Off,
}

impl Config {
    fn telemetry_id(&self) -> u32 {
        self.fd_id.unwrap_or(self.heartbeat_id + 0x100)
    }
}

const USAGE: &str = "\
usage: can-demo [--iface NAME] [--id HEX] [--fd auto|on|off] [--fd-id HEX]
                [--interval-ms N] [--report-s N] [--mode both|send|listen]

Environment (overridden by flags): CAN_DEMO_IFACE, CAN_DEMO_ID, CAN_DEMO_FD,
CAN_DEMO_FD_ID, CAN_DEMO_INTERVAL_MS, CAN_DEMO_REPORT_S, CAN_DEMO_MODE.";

fn parse_id(s: &str) -> Result<u32, String> {
    let t = s.trim_start_matches("0x").trim_start_matches("0X");
    u32::from_str_radix(t, 16).map_err(|e| format!("bad CAN id {s:?}: {e}"))
}

fn parse_u64(name: &str, s: &str) -> Result<u64, String> {
    s.parse::<u64>().map_err(|e| format!("bad {name} {s:?}: {e}"))
}

fn apply(cfg: &mut Config, key: &str, val: &str) -> Result<(), String> {
    match key {
        "iface" => cfg.iface = val.to_string(),
        "id" => cfg.heartbeat_id = parse_id(val)?,
        "fd-id" => cfg.fd_id = Some(parse_id(val)?),
        "fd" => {
            cfg.fd = match val {
                "auto" => FdMode::Auto,
                "on" => FdMode::On,
                "off" => FdMode::Off,
                _ => return Err(format!("bad fd mode {val:?}")),
            }
        }
        "interval-ms" => cfg.interval = Duration::from_millis(parse_u64(key, val)?.max(10)),
        "report-s" => cfg.report_every = Duration::from_secs(parse_u64(key, val)?.max(1)),
        "mode" => {
            (cfg.send, cfg.listen) = match val {
                "both" => (true, true),
                "send" => (true, false),
                "listen" => (false, true),
                _ => return Err(format!("bad mode {val:?}")),
            }
        }
        _ => return Err(format!("unknown option --{key}")),
    }
    Ok(())
}

fn parse_config<I, E>(args: I, env: E) -> Result<Config, String>
where
    I: IntoIterator<Item = String>,
    E: Fn(&str) -> Option<String>,
{
    let mut cfg = Config::default();
    for (var, key) in [
        ("CAN_DEMO_IFACE", "iface"),
        ("CAN_DEMO_ID", "id"),
        ("CAN_DEMO_FD", "fd"),
        ("CAN_DEMO_FD_ID", "fd-id"),
        ("CAN_DEMO_INTERVAL_MS", "interval-ms"),
        ("CAN_DEMO_REPORT_S", "report-s"),
        ("CAN_DEMO_MODE", "mode"),
    ] {
        if let Some(v) = env(var).filter(|v| !v.is_empty()) {
            apply(&mut cfg, key, &v)?;
        }
    }
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        if arg == "-h" || arg == "--help" {
            return Err(USAGE.to_string());
        }
        let Some(flag) = arg.strip_prefix("--") else {
            return Err(format!("unexpected argument {arg:?}\n{USAGE}"));
        };
        let (key, val) = match flag.split_once('=') {
            Some((k, v)) => (k.to_string(), v.to_string()),
            None => (flag.to_string(), it.next().ok_or(format!("--{flag} needs a value"))?),
        };
        apply(&mut cfg, &key, &val)?;
    }
    if !cfg.send && !cfg.listen {
        return Err("nothing to do".into());
    }
    Ok(cfg)
}

/// Log with an sd-daemon(3) priority prefix when stdout goes to the journal.
fn log(prio: u8, msg: &str) {
    if std::env::var_os("JOURNAL_STREAM").is_some() {
        println!("<{prio}>{msg}");
    } else {
        println!("{msg}");
    }
}
const INFO: u8 = 6;
const WARN: u8 = 4;
const ERR: u8 = 3;

fn heartbeat_payload(seq: u32) -> [u8; 8] {
    let mut p = [0u8; 8];
    p[..4].copy_from_slice(&seq.to_le_bytes());
    p[4..].copy_from_slice(b"HBT1");
    p
}

fn heartbeat_seq(frame: &Frame) -> Option<u32> {
    let p = frame.payload();
    (!frame.is_fd() && p.len() == 8 && &p[4..] == b"HBT1").then(|| u32::from_le_bytes([p[0], p[1], p[2], p[3]]))
}

/// What goes into the 64-byte FD telemetry frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Telemetry {
    seq: u32,
    uptime_ms: u64,
    frames_rx: u64,
    start_count: u32,
}

const TELEMETRY_LEN: usize = 64;
const PATTERN_AT: usize = 32;

fn telemetry_payload(t: &Telemetry) -> [u8; TELEMETRY_LEN] {
    let mut p = [0u8; TELEMETRY_LEN];
    p[0..4].copy_from_slice(&t.seq.to_le_bytes());
    p[4..8].copy_from_slice(b"FDT1");
    p[8..16].copy_from_slice(&t.uptime_ms.to_le_bytes());
    p[16..24].copy_from_slice(&t.frames_rx.to_le_bytes());
    p[24..28].copy_from_slice(&t.start_count.to_le_bytes());
    // bytes 28..32 reserved (0); 32..64: seq's low byte counting up, so a
    // receiver can tell that all 64 bytes arrived intact.
    for (i, b) in p[PATTERN_AT..].iter_mut().enumerate() {
        *b = (t.seq as u8).wrapping_add(i as u8);
    }
    p
}

/// Some(Ok) for an intact telemetry frame, Some(Err(seq)) for one whose
/// pattern is wrong, None if the frame is not telemetry.
fn parse_telemetry(frame: &Frame) -> Option<Result<Telemetry, u32>> {
    let p = frame.payload();
    if !frame.is_fd() || p.len() != TELEMETRY_LEN || &p[4..8] != b"FDT1" {
        return None;
    }
    let u64_at = |i: usize| u64::from_le_bytes(p[i..i + 8].try_into().unwrap());
    let t = Telemetry {
        seq: u32::from_le_bytes(p[0..4].try_into().unwrap()),
        uptime_ms: u64_at(8),
        frames_rx: u64_at(16),
        start_count: u32::from_le_bytes(p[24..28].try_into().unwrap()),
    };
    let intact = p[PATTERN_AT..].iter().enumerate().all(|(i, &b)| b == (t.seq as u8).wrapping_add(i as u8));
    Some(if intact { Ok(t) } else { Err(t.seq) })
}

/// Whether to send FD telemetry on this interface (FdMode::Auto: if its MTU
/// can carry FD frames).
fn fd_enabled(cfg: &Config) -> bool {
    match cfg.fd {
        FdMode::On => true,
        FdMode::Off => false,
        FdMode::Auto => can::interface_mtu(&cfg.iface).is_ok_and(|m| m >= CANFD_MTU),
    }
}

struct Shared {
    stop: AtomicBool,
    frames_rx: AtomicU64,
    start_count: AtomicU64,
}

fn run_sender(cfg: Config, shared: Arc<Shared>) -> io::Result<()> {
    let sock = CanSocket::open(&cfg.iface)?;
    let fd = fd_enabled(&cfg);
    if fd {
        log(INFO, &format!("sender: CAN FD telemetry on {} with id {:#x}", cfg.iface, cfg.telemetry_id()));
    } else {
        log(INFO, &format!("sender: {} is classic CAN only (or --fd off): heartbeat only", cfg.iface));
    }
    let started = Instant::now();
    let mut seq: u32 = 0;
    let mut failing = false;
    let mut fd_failing = false;
    while !shared.stop.load(Ordering::Relaxed) {
        if fd {
            let t = Telemetry {
                seq,
                uptime_ms: started.elapsed().as_millis() as u64,
                frames_rx: shared.frames_rx.load(Ordering::Relaxed),
                start_count: shared.start_count.load(Ordering::Relaxed) as u32,
            };
            let frame = Frame::fd(cfg.telemetry_id(), &telemetry_payload(&t), true).expect("valid id and length");
            match sock.write_frame(&frame) {
                Ok(()) if fd_failing => {
                    log(INFO, &format!("telemetry: sending again (seq {seq})"));
                    fd_failing = false;
                }
                Ok(()) => {}
                Err(e) if !fd_failing => {
                    log(WARN, &format!("telemetry: write failed: {e}"));
                    fd_failing = true;
                }
                Err(_) => {}
            }
        }
        let frame = Frame::classic(cfg.heartbeat_id, &heartbeat_payload(seq)).expect("valid id");
        match sock.write_frame(&frame) {
            Ok(()) => {
                if failing {
                    log(INFO, &format!("heartbeat: sending again (seq {seq})"));
                    failing = false;
                }
                seq = seq.wrapping_add(1);
            }
            // ENOBUFS happens when the TX queue is full (e.g. no peer on real CAN).
            Err(e) if !failing => {
                log(WARN, &format!("heartbeat: write failed: {e}"));
                failing = true;
            }
            Err(_) => {}
        }
        thread::sleep(cfg.interval);
    }
    Ok(())
}

#[derive(Default)]
struct Stats {
    total: u64,
    errors: u64,
    fd: u64,
    fd_bad: u64,
    per_id: BTreeMap<u32, u64>,
    hb_last: Option<u32>,
    hb_missed: u64,
}

impl Stats {
    fn record(&mut self, frame: &Frame, heartbeat_id: u32) {
        self.total += 1;
        if frame.is_error() {
            self.errors += 1;
            return;
        }
        *self.per_id.entry(frame.id()).or_default() += 1;
        if frame.is_fd() {
            self.fd += 1;
            if let Some(Err(_)) = parse_telemetry(frame) {
                self.fd_bad += 1;
            }
        }
        if frame.id() == heartbeat_id {
            if let Some(seq) = heartbeat_seq(frame) {
                if let Some(last) = self.hb_last {
                    let gap = seq.wrapping_sub(last);
                    if gap > 1 {
                        self.hb_missed += u64::from(gap - 1);
                    }
                }
                self.hb_last = Some(seq);
            }
        }
    }

    fn summary(&self) -> String {
        let ids: Vec<String> = self.per_id.iter().take(8).map(|(id, n)| format!("{id:#x}={n}")).collect();
        let more = if self.per_id.len() > 8 { format!(" (+{} ids)", self.per_id.len() - 8) } else { String::new() };
        format!(
            "frames total={} errors={} fd={} fd_bad={} heartbeat_last_seq={} heartbeat_missed={} ids[{}]{}",
            self.total,
            self.errors,
            self.fd,
            self.fd_bad,
            self.hb_last.map_or("-".into(), |s| s.to_string()),
            self.hb_missed,
            ids.join(" "),
            more
        )
    }
}

fn run_listener(cfg: &Config, shared: &Shared) -> io::Result<()> {
    let sock = CanSocket::open(&cfg.iface)?;
    sock.set_read_timeout(Duration::from_secs(1))?;
    let mut stats = Stats::default();
    let mut last_report = Instant::now();
    while !shared.stop.load(Ordering::Relaxed) {
        match sock.read_frame() {
            Ok(frame) => {
                if !stats.per_id.contains_key(&frame.id()) && !frame.is_error() {
                    let kind = match (frame.is_fd(), frame.flags() & can::CANFD_BRS != 0) {
                        (false, _) => "classic",
                        (true, false) => "FD",
                        (true, true) => "FD+BRS",
                    };
                    log(INFO, &format!("listener: first frame from id {:#x} len {} ({kind})", frame.id(), frame.payload().len()));
                }
                stats.record(&frame, cfg.heartbeat_id);
                shared.frames_rx.store(stats.total, Ordering::Relaxed);
            }
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => {}
            Err(e) => return Err(e),
        }
        notify::watchdog();
        if last_report.elapsed() >= cfg.report_every {
            log(INFO, &format!("listener: {}", stats.summary()));
            last_report = Instant::now();
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let cfg = match parse_config(std::env::args().skip(1), |k| std::env::var(k).ok()) {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(2);
        }
    };
    log(
        INFO,
        &format!(
            "can-demo {} starting: iface={} heartbeat_id={:#x} fd={:?} fd_id={:#x} interval={}ms send={} listen={}",
            env!("CARGO_PKG_VERSION"),
            cfg.iface,
            cfg.heartbeat_id,
            cfg.fd,
            cfg.telemetry_id(),
            cfg.interval.as_millis(),
            cfg.send,
            cfg.listen
        ),
    );
    let shared = Arc::new(Shared {
        stop: AtomicBool::new(false),
        frames_rx: AtomicU64::new(0),
        start_count: AtomicU64::new(0),
    });
    if let Some(dir) = std::env::var_os("STATE_DIRECTORY") {
        match state::bump_start_count(std::path::Path::new(&dir), &cfg.iface) {
            Ok(n) => {
                shared.start_count.store(n, Ordering::Relaxed);
                log(INFO, &format!("start #{n} on {} (count kept in {})", cfg.iface, dir.to_string_lossy()))
            }
            Err(e) => log(WARN, &format!("cannot update start count in {}: {e}", dir.to_string_lossy())),
        }
    }

    let sender = cfg.send.then(|| {
        let (c, s) = (cfg.clone(), shared.clone());
        thread::spawn(move || run_sender(c, s))
    });
    notify::ready();

    let result = if cfg.listen {
        run_listener(&cfg, &shared)
    } else {
        loop {
            notify::watchdog();
            if sender.as_ref().is_some_and(|h| h.is_finished()) {
                break Ok(());
            }
            thread::sleep(Duration::from_secs(1));
        }
    };
    shared.stop.store(true, Ordering::Relaxed);
    let sender_result = sender.map(|h| h.join().unwrap_or_else(|_| Err(io::Error::other("sender panicked"))));

    match result.and(sender_result.unwrap_or(Ok(()))) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            log(ERR, &format!("can-demo: socket error on {}: {e}", cfg.iface));
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(args: &[&str], env: &[(&str, &str)]) -> Result<Config, String> {
        let env: Vec<(String, String)> = env.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        parse_config(args.iter().map(|s| s.to_string()), move |k| {
            env.iter().find(|(ek, _)| ek == k).map(|(_, v)| v.clone())
        })
    }

    #[test]
    fn defaults() {
        assert_eq!(cfg(&[], &[]).unwrap(), Config::default());
    }

    #[test]
    fn flags_override_env() {
        let c = cfg(&["--iface", "can1", "--id=0x7ff"], &[("CAN_DEMO_IFACE", "can0"), ("CAN_DEMO_MODE", "listen")]).unwrap();
        assert_eq!(c.iface, "can1");
        assert_eq!(c.heartbeat_id, 0x7ff);
        assert!(!c.send && c.listen);
    }

    #[test]
    fn rejects_bad_input() {
        assert!(cfg(&["--mode", "x"], &[]).is_err());
        assert!(cfg(&["--id", "zz"], &[]).is_err());
        assert!(cfg(&["--iface"], &[]).is_err());
        assert!(cfg(&["stray"], &[]).is_err());
    }

    #[test]
    fn counts_heartbeat_gaps() {
        let mut s = Stats::default();
        for seq in [0u32, 1, 2, 5, 6] {
            s.record(&Frame::classic(0x100, &heartbeat_payload(seq)).unwrap(), 0x100);
        }
        s.record(&Frame::classic(0x300, &[1]).unwrap(), 0x100);
        // An FD frame with the heartbeat id is not a heartbeat.
        s.record(&Frame::fd(0x100, &heartbeat_payload(9), false).unwrap(), 0x100);
        assert_eq!(s.total, 7);
        assert_eq!(s.hb_missed, 2);
        assert_eq!(s.hb_last, Some(6));
        assert_eq!(s.per_id[&0x300], 1);
        assert_eq!(s.fd, 1);
    }

    #[test]
    fn fd_options() {
        let c = cfg(&[], &[]).unwrap();
        assert_eq!((c.fd, c.telemetry_id()), (FdMode::Auto, 0x200));
        let c = cfg(&["--id", "0x101"], &[("CAN_DEMO_FD", "off")]).unwrap();
        assert_eq!((c.fd, c.telemetry_id()), (FdMode::Off, 0x201));
        let c = cfg(&["--fd=on", "--fd-id", "0x1abcde"], &[]).unwrap();
        assert_eq!((c.fd, c.telemetry_id()), (FdMode::On, 0x1abcde));
        assert!(cfg(&["--fd", "maybe"], &[]).is_err());
    }

    #[test]
    fn telemetry_round_trip_and_damage() {
        let t = Telemetry { seq: 300, uptime_ms: 123_456, frames_rx: 99, start_count: 4 };
        let p = telemetry_payload(&t);
        assert_eq!(&p[4..8], b"FDT1");
        assert_eq!(p[PATTERN_AT], 300u32 as u8); // 0x2C
        assert_eq!(p[63], (300u32 as u8).wrapping_add(31));
        let f = Frame::fd(0x201, &p, true).unwrap();
        assert_eq!(parse_telemetry(&f), Some(Ok(t)));

        let mut bad = p;
        bad[50] ^= 0xFF;
        let fb = Frame::fd(0x201, &bad, true).unwrap();
        assert_eq!(parse_telemetry(&fb), Some(Err(300)));
        let mut s = Stats::default();
        s.record(&f, 0x101);
        s.record(&fb, 0x101);
        assert_eq!((s.fd, s.fd_bad), (2, 1));

        // Classic frames and other FD frames are not telemetry.
        assert_eq!(parse_telemetry(&Frame::classic(0x201, &[0; 8]).unwrap()), None);
        assert_eq!(parse_telemetry(&Frame::fd(0x201, &[0; 64], true).unwrap()), None);
    }
}
