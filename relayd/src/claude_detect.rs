use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{debug, warn};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ClaudeProcessInfo {
    pub pid: u32,
    pub cwd: PathBuf,
    pub has_claude_md: bool,
    pub uptime_secs: u64,
}

#[derive(Clone)]
pub struct ClaudeDetector {
    processes: Arc<Mutex<Vec<ClaudeProcessInfo>>>,
}

impl ClaudeDetector {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(Vec::new())),
        }
    }

    #[allow(dead_code)]
    pub fn current(&self) -> Vec<ClaudeProcessInfo> {
        self.processes.lock().unwrap().clone()
    }

    pub fn start_polling(&self, interval: Duration) {
        let detector = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(interval);
            loop {
                tick.tick().await;
                let results = scan_claude_processes();
                *detector.processes.lock().unwrap() = results;
            }
        });
    }

    /// Check if any detected Claude process has a cwd matching the given path.
    pub fn is_claude_detected_at(&self, cwd: &std::path::Path) -> bool {
        let processes = self.processes.lock().unwrap();
        processes.iter().any(|p| p.cwd == cwd)
    }
}

pub fn scan_claude_processes() -> Vec<ClaudeProcessInfo> {
    let output = match Command::new("ps")
        .args(["axo", "pid,comm"])
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            warn!("Failed to run ps: {}", e);
            return Vec::new();
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut results = Vec::new();

    for line in stdout.lines().skip(1) {
        let line = line.trim();
        if !line.to_lowercase().contains("claude") {
            continue;
        }

        let parts: Vec<&str> = line.splitn(2, char::is_whitespace).collect();
        if parts.len() < 2 {
            continue;
        }

        let pid: u32 = match parts[0].trim().parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        let cwd = match get_process_cwd(pid) {
            Some(c) => c,
            None => continue,
        };

        let has_claude_md = cwd.join("CLAUDE.md").exists();
        let uptime_secs = get_process_uptime(pid).unwrap_or(0);

        results.push(ClaudeProcessInfo {
            pid,
            cwd,
            has_claude_md,
            uptime_secs,
        });
    }

    debug!("Found {} Claude processes", results.len());
    results
}

fn get_process_cwd(pid: u32) -> Option<PathBuf> {
    // macOS: use lsof to find cwd
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("lsof")
            .args(["-p", &pid.to_string(), "-Fn", "-d", "cwd"])
            .output()
            .ok()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if let Some(path) = line.strip_prefix('n') {
                let p = PathBuf::from(path);
                if p.is_absolute() {
                    return Some(p);
                }
            }
        }
        None
    }

    // Linux: read /proc/{pid}/cwd symlink
    #[cfg(target_os = "linux")]
    {
        let link = format!("/proc/{}/cwd", pid);
        std::fs::read_link(link).ok()
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

fn get_process_uptime(pid: u32) -> Option<u64> {
    let output = Command::new("ps")
        .args(["-o", "etime=", "-p", &pid.to_string()])
        .output()
        .ok()?;

    let etime = String::from_utf8_lossy(&output.stdout).trim().to_string();
    parse_etime(&etime)
}

/// Parse ps etime format: [[dd-]hh:]mm:ss
fn parse_etime(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let (days, rest) = if let Some(idx) = s.find('-') {
        let d: u64 = s[..idx].parse().ok()?;
        (d, &s[idx + 1..])
    } else {
        (0, s)
    };

    let parts: Vec<&str> = rest.split(':').collect();
    let (hours, minutes, seconds) = match parts.len() {
        3 => {
            let h: u64 = parts[0].parse().ok()?;
            let m: u64 = parts[1].parse().ok()?;
            let s: u64 = parts[2].parse().ok()?;
            (h, m, s)
        }
        2 => {
            let m: u64 = parts[0].parse().ok()?;
            let s: u64 = parts[1].parse().ok()?;
            (0, m, s)
        }
        _ => return None,
    };

    Some(days * 86400 + hours * 3600 + minutes * 60 + seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_etime_seconds() {
        assert_eq!(parse_etime("00:05"), Some(5));
    }

    #[test]
    fn parse_etime_minutes() {
        assert_eq!(parse_etime("03:25"), Some(205));
    }

    #[test]
    fn parse_etime_hours() {
        assert_eq!(parse_etime("01:03:25"), Some(3805));
    }

    #[test]
    fn parse_etime_days() {
        assert_eq!(parse_etime("2-01:03:25"), Some(2 * 86400 + 3805));
    }
}
