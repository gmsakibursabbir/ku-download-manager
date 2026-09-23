//! Post-completion power actions (shutdown / sleep).

use anyhow::{bail, Result};

pub fn perform(action: &str) -> Result<()> {
    match action {
        "shutdown" => shutdown(),
        "sleep" => sleep(),
        other => bail!("unknown power action {other}"),
    }
}

#[cfg(windows)]
fn shutdown() -> Result<()> {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("shutdown")
        .args(["/s", "/t", "0"])
        .creation_flags(0x0800_0000)
        .status()?;
    Ok(())
}

#[cfg(windows)]
fn sleep() -> Result<()> {
    // SetSuspendState(hibernate=false, force=false, wakeup_events_disabled=false)
    let ok = unsafe { windows_sys::Win32::System::Power::SetSuspendState(0, 0, 0) };
    if ok == 0 {
        bail!("Windows refused to enter sleep");
    }
    Ok(())
}

#[cfg(not(windows))]
fn shutdown() -> Result<()> {
    let st = std::process::Command::new("systemctl").arg("poweroff").status()?;
    if !st.success() {
        bail!("systemctl poweroff failed");
    }
    Ok(())
}

#[cfg(not(windows))]
fn sleep() -> Result<()> {
    let st = std::process::Command::new("systemctl").arg("suspend").status()?;
    if !st.success() {
        bail!("systemctl suspend failed");
    }
    Ok(())
}
