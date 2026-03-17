// Session management: implicit identity from process tree
// No registration required. The session ID is derived from the parent process PID,
// which stays constant across all tool calls within a single agent session.

use std::fmt;

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    #[allow(dead_code)]
    pub pid: u32,
    #[allow(dead_code)]
    pub ppid: u32,
    #[allow(dead_code)]
    pub command: String,
    pub args: String,
}

/// Walk /proc (Linux) or use sysctl (macOS) to get info for a single PID.
pub fn process_info(pid: u32) -> Option<ProcessInfo> {
    // Linux: read /proc/<pid>/status and /proc/<pid>/cmdline
    #[cfg(target_os = "linux")]
    {
        linux_process_info(pid)
    }
    #[cfg(target_os = "macos")]
    {
        macos_process_info(pid)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

#[cfg(target_os = "linux")]
fn linux_process_info(pid: u32) -> Option<ProcessInfo> {
    use std::fs;

    let status = fs::read_to_string(format!("/proc/{}/status", pid)).ok()?;
    let mut ppid = 0u32;
    let mut name = String::new();
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("PPid:") {
            ppid = rest.trim().parse().unwrap_or(0);
        }
        if let Some(rest) = line.strip_prefix("Name:") {
            name = rest.trim().to_string();
        }
    }

    // Read full cmdline (args separated by NUL bytes)
    let cmdline = fs::read(format!("/proc/{}/cmdline", pid)).ok()?;
    let args: Vec<&str> = cmdline
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| std::str::from_utf8(s).unwrap_or("?"))
        .collect();
    let args_str = args.join(" ");

    Some(ProcessInfo {
        pid,
        ppid,
        command: name,
        args: args_str,
    })
}

#[cfg(target_os = "macos")]
fn macos_process_info(pid: u32) -> Option<ProcessInfo> {
    // Use `ps` as the portable fallback on macOS
    let out = std::process::Command::new("ps")
        .args([
            "-p",
            &pid.to_string(),
            "-o",
            "pid,ppid,comm,args",
            "--no-headers",
        ])
        .output()
        .ok()?;
    let line = String::from_utf8_lossy(&out.stdout);
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let mut parts = line.splitn(4, char::is_whitespace);
    let _pid_str = parts.next()?;
    let ppid: u32 = parts.next()?.trim().parse().ok()?;
    let command = parts.next()?.trim().to_string();
    let args = parts.next().unwrap_or("").trim().to_string();
    Some(ProcessInfo {
        pid,
        ppid,
        command,
        args,
    })
}

/// The session ID is the PPID of the `plan` process — i.e. the calling agent's PID.
/// This is stable for the lifetime of the agent session regardless of how many
/// times `plan` is invoked.
pub fn session_id() -> u32 {
    // std::os::unix provides getppid
    #[cfg(unix)]
    {
        libc_ppid()
    }
    #[cfg(not(unix))]
    {
        std::process::id() // fallback: our own PID
    }
}

#[cfg(unix)]
fn libc_ppid() -> u32 {
    // Use nix or inline via std — std::os::unix doesn't expose getppid directly,
    // but we can read /proc/self/status on Linux or use the `libc` crate.
    // We already have no libc dep, so parse /proc/self/status on Linux,
    // or spawn `ps` on macOS.
    #[cfg(target_os = "linux")]
    {
        if let Ok(s) = std::fs::read_to_string("/proc/self/status") {
            for line in s.lines() {
                if let Some(rest) = line.strip_prefix("PPid:") {
                    if let Ok(n) = rest.trim().parse::<u32>() {
                        return n;
                    }
                }
            }
        }
        0
    }
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("ps")
            .args([
                "-p",
                &std::process::id().to_string(),
                "-o",
                "ppid",
                "--no-headers",
            ])
            .output();
        if let Ok(out) = out {
            if let Ok(s) = std::str::from_utf8(&out.stdout) {
                if let Ok(n) = s.trim().parse::<u32>() {
                    return n;
                }
            }
        }
        0
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        0
    }
}

/// Build the process ancestry chain starting from a given PID, walking upward.
/// Returns vec of ProcessInfo from the given pid up to init/PID1.
#[allow(dead_code)]
pub fn process_chain(start_pid: u32, max_depth: usize) -> Vec<ProcessInfo> {
    let mut chain = Vec::new();
    let mut pid = start_pid;
    for _ in 0..max_depth {
        if pid == 0 {
            break;
        }
        match process_info(pid) {
            Some(info) => {
                let next_ppid = info.ppid;
                chain.push(info);
                if next_ppid == 0 || next_ppid == pid {
                    break;
                }
                pid = next_ppid;
            }
            None => break,
        }
    }
    chain
}

/// Format a session ID (u32 PID) as a short hex string for display / storage.
#[allow(dead_code)]
pub fn session_id_hex(sid: u32) -> String {
    format!("{:x}", sid)
}

impl fmt::Display for ProcessInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.args)
    }
}
