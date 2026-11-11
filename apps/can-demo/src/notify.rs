//! Tiny sd_notify(3) client: READY=1 and WATCHDOG=1 over $NOTIFY_SOCKET.
//! Does nothing when not started by systemd.

use std::os::unix::net::UnixDatagram;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

fn send(msg: &str) {
    let Some(path) = std::env::var_os("NOTIFY_SOCKET") else { return };
    let Ok(sock) = UnixDatagram::unbound() else { return };
    let path = path.to_string_lossy().into_owned();
    if let Some(abstract_name) = path.strip_prefix('@') {
        use std::os::linux::net::SocketAddrExt;
        if let Ok(addr) = std::os::unix::net::SocketAddr::from_abstract_name(abstract_name) {
            let _ = sock.send_to_addr(msg.as_bytes(), &addr);
        }
    } else {
        let _ = sock.send_to(msg.as_bytes(), &path);
    }
}

pub fn ready() {
    send("READY=1");
}

/// Pets the systemd watchdog at most every half of WATCHDOG_USEC.
pub fn watchdog() {
    static STATE: OnceLock<Option<(Duration, Mutex<Option<Instant>>)>> = OnceLock::new();
    let state = STATE.get_or_init(|| {
        let usec: u64 = std::env::var("WATCHDOG_USEC").ok()?.parse().ok()?;
        Some((Duration::from_micros(usec / 2), Mutex::new(None)))
    });
    if let Some((half, last)) = state {
        let mut last = last.lock().unwrap();
        if last.is_none_or(|t| t.elapsed() >= *half) {
            send("WATCHDOG=1");
            *last = Some(Instant::now());
        }
    }
}
