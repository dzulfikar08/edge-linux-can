//! can-demo: a heartbeat sender plus a listener that counts frames on a
//! SocketCAN interface. Built to run under systemd: logs go to stdout with
//! sd-daemon priority prefixes (picked up by journald), and it reports
//! READY=1 / WATCHDOG=1 over $NOTIFY_SOCKET when that is set.

mod can;
mod notify;
mod state;

use std::collections::BTreeMap;
use std::io;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use can::{CanFrame, CanSocket};

#[derive(Debug, Clone, PartialEq)]
struct Config {
    iface: String,
    heartbeat_id: u32,
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
            interval: Duration::from_millis(1000),
            report_every: Duration::from_secs(10),
            send: true,
            listen: true,
        }
    }
}

const USAGE: &str = "\
usage: can-demo [--iface NAME] [--id HEX] [--interval-ms N] [--report-s N] [--mode both|send|listen]

Environment (overridden by flags): CAN_DEMO_IFACE, CAN_DEMO_ID, CAN_DEMO_INTERVAL_MS,
CAN_DEMO_REPORT_S, CAN_DEMO_MODE.";

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

fn heartbeat_seq(frame: &CanFrame) -> Option<u32> {
    let p = frame.payload();
    (p.len() == 8 && &p[4..] == b"HBT1").then(|| u32::from_le_bytes([p[0], p[1], p[2], p[3]]))
}

fn run_sender(cfg: Config, stop: Arc<AtomicBool>) -> io::Result<()> {
    let sock = CanSocket::open(&cfg.iface)?;
    let mut seq: u32 = 0;
    let mut failing = false;
    while !stop.load(Ordering::Relaxed) {
        let frame = CanFrame::new(cfg.heartbeat_id, &heartbeat_payload(seq)).expect("valid id");
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
    per_id: BTreeMap<u32, u64>,
    hb_last: Option<u32>,
    hb_missed: u64,
}

impl Stats {
    fn record(&mut self, frame: &CanFrame, heartbeat_id: u32) {
        self.total += 1;
        if frame.is_error() {
            self.errors += 1;
            return;
        }
        *self.per_id.entry(frame.id()).or_default() += 1;
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
            "frames total={} errors={} heartbeat_last_seq={} heartbeat_missed={} ids[{}]{}",
            self.total,
            self.errors,
            self.hb_last.map_or("-".into(), |s| s.to_string()),
            self.hb_missed,
            ids.join(" "),
            more
        )
    }
}

fn run_listener(cfg: &Config, stop: &AtomicBool) -> io::Result<()> {
    let sock = CanSocket::open(&cfg.iface)?;
    sock.set_read_timeout(Duration::from_secs(1))?;
    let mut stats = Stats::default();
    let mut last_report = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        match sock.read_frame() {
            Ok(frame) => {
                if !stats.per_id.contains_key(&frame.id()) && !frame.is_error() {
                    log(INFO, &format!("listener: first frame from id {:#x} len {}", frame.id(), frame.len));
                }
                stats.record(&frame, cfg.heartbeat_id);
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
            "can-demo {} starting: iface={} heartbeat_id={:#x} interval={}ms send={} listen={}",
            env!("CARGO_PKG_VERSION"),
            cfg.iface,
            cfg.heartbeat_id,
            cfg.interval.as_millis(),
            cfg.send,
            cfg.listen
        ),
    );
    if let Some(dir) = std::env::var_os("STATE_DIRECTORY") {
        match state::bump_start_count(std::path::Path::new(&dir), &cfg.iface) {
            Ok(n) => log(INFO, &format!("start #{n} on {} (count kept in {})", cfg.iface, dir.to_string_lossy())),
            Err(e) => log(WARN, &format!("cannot update start count in {}: {e}", dir.to_string_lossy())),
        }
    }

    let stop = Arc::new(AtomicBool::new(false));
    let sender = cfg.send.then(|| {
        let (c, s) = (cfg.clone(), stop.clone());
        thread::spawn(move || run_sender(c, s))
    });
    notify::ready();

    let result = if cfg.listen {
        run_listener(&cfg, &stop)
    } else {
        loop {
            notify::watchdog();
            if sender.as_ref().is_some_and(|h| h.is_finished()) {
                break Ok(());
            }
            thread::sleep(Duration::from_secs(1));
        }
    };
    stop.store(true, Ordering::Relaxed);
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
            s.record(&CanFrame::new(0x100, &heartbeat_payload(seq)).unwrap(), 0x100);
        }
        s.record(&CanFrame::new(0x200, &[1]).unwrap(), 0x100);
        assert_eq!(s.total, 6);
        assert_eq!(s.hb_missed, 2);
        assert_eq!(s.hb_last, Some(6));
        assert_eq!(s.per_id[&0x200], 1);
    }
}
