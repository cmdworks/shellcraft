//! Small process and time helpers shared by the crates.

use std::process::Child;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Seconds since the Unix epoch (0 if the clock is before it).
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `SHELLCRAFT_OFFLINE=1`: never touch the network (no registry refresh, no update check). The
/// tests and CI set it; users can too.
pub fn offline() -> bool {
    std::env::var_os("SHELLCRAFT_OFFLINE").is_some_and(|v| !v.is_empty() && v != "0")
}

/// Wait for `child` to exit, polling every `poll`. After `limit` it is killed and reaped.
/// Returns whether it finished on its own.
pub fn wait_timeout(child: &mut Child, limit: Duration, poll: Duration) -> std::io::Result<bool> {
    let start = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            return Ok(true);
        }
        if start.elapsed() >= limit {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(false);
        }
        std::thread::sleep(poll);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn finishes_or_is_killed() {
        let mut quick = Command::new("true").spawn().expect("spawn");
        assert!(
            wait_timeout(&mut quick, Duration::from_secs(2), Duration::from_millis(5))
                .expect("wait")
        );
        let mut slow = Command::new("sleep").arg("5").spawn().expect("spawn");
        let t = Instant::now();
        assert!(!wait_timeout(
            &mut slow,
            Duration::from_millis(100),
            Duration::from_millis(5)
        )
        .expect("wait"));
        assert!(t.elapsed() < Duration::from_secs(2));
    }
}
