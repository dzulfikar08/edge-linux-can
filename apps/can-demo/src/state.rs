//! Persistent state: how many times this instance has started.
//!
//! systemd creates the directory (`StateDirectory=can-demo` in the unit) and
//! passes its path in `STATE_DIRECTORY`. On the device it lives on the data
//! partition, so the count survives reboots and A/B updates of the rootfs.

use std::fs;
use std::io;
use std::path::Path;

/// Increments `<dir>/<iface>.starts` and returns the new count. A missing or
/// unreadable file counts as 0. The new value is written to a temporary file
/// and renamed over the old one, so a power cut leaves the old or the new
/// value, never a half-written file.
pub fn bump_start_count(dir: &Path, iface: &str) -> io::Result<u64> {
    let path = dir.join(format!("{iface}.starts"));
    let old: u64 = fs::read_to_string(&path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    let new = old + 1;
    let tmp = dir.join(format!(".{iface}.starts.tmp"));
    fs::write(&tmp, format!("{new}\n"))?;
    fs::File::open(&tmp)?.sync_all()?;
    fs::rename(&tmp, &path)?;
    Ok(new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_starts_per_interface() {
        let dir = std::env::temp_dir().join(format!("can-demo-state-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(bump_start_count(&dir, "vcan0").unwrap(), 1);
        assert_eq!(bump_start_count(&dir, "vcan0").unwrap(), 2);
        assert_eq!(bump_start_count(&dir, "can0").unwrap(), 1);
        fs::write(dir.join("can0.starts"), "garbage").unwrap();
        assert_eq!(bump_start_count(&dir, "can0").unwrap(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }
}
