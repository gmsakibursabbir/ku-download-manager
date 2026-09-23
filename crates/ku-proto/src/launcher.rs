//! Start the desktop app in the background when a client needs KuCore.

use crate::client::{Client, ClientError};
use crate::paths;
use std::process::Command;
use std::time::{Duration, Instant};

/// Return a connected client, launching KuDownloader (hidden) if needed.
pub fn ensure_running(wait: Duration) -> Result<Client, ClientError> {
    if let Ok(c) = Client::discover() {
        return Ok(c);
    }
    let exe = paths::app_executable()
        .ok_or_else(|| ClientError::Io("KuDownloader executable not found next to this program".into()))?;
    spawn_detached(&exe).map_err(|e| ClientError::Io(format!("failed to start KuDownloader: {e}")))?;
    let start = Instant::now();
    while start.elapsed() < wait {
        std::thread::sleep(Duration::from_millis(250));
        if let Ok(c) = Client::discover() {
            return Ok(c);
        }
    }
    Err(ClientError::NotRunning)
}

fn spawn_detached(exe: &std::path::Path) -> std::io::Result<()> {
    let mut cmd = Command::new(exe);
    cmd.arg("--hidden");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        // Browsers run native hosts inside a job object; break away so the app
        // survives when the host exits. Fall back if the job forbids it.
        let base = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
        if cmd.creation_flags(base | CREATE_BREAKAWAY_FROM_JOB).spawn().is_ok() {
            return Ok(());
        }
        cmd.creation_flags(base).spawn().map(|_| ())
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .process_group(0)
            .spawn()
            .map(|_| ())
    }
}
