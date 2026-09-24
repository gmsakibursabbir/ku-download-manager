//! Optional virus scan of finished downloads (like IDM's "scan with
//! antivirus"): Microsoft Defender on Windows or any scanner the user picks.
//! Configured only in the app's own settings (never via the local API).

use anyhow::{anyhow, bail, Result};
use std::path::{Path, PathBuf};
use tokio::process::Command;

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Clean,
    Threat(String),
}

/// Latest Defender command-line scanner.
#[cfg(windows)]
fn defender() -> Option<PathBuf> {
    let platform = PathBuf::from(std::env::var_os("ProgramData").unwrap_or_else(|| "C:\\ProgramData".into())).join(r"Microsoft\Windows Defender\Platform");
    let mut versions: Vec<PathBuf> = std::fs::read_dir(&platform).ok()?.flatten().map(|e| e.path()).filter(|p| p.join("MpCmdRun.exe").is_file()).collect();
    versions.sort();
    versions.pop().map(|p| p.join("MpCmdRun.exe")).or_else(|| {
        let p = PathBuf::from(std::env::var_os("ProgramFiles").unwrap_or_else(|| "C:\\Program Files".into())).join(r"Windows Defender\MpCmdRun.exe");
        p.is_file().then_some(p)
    })
}

#[cfg(not(windows))]
fn defender() -> Option<PathBuf> {
    None
}

/// Split an argument template like `--scan "{file}" -q` (double quotes group).
pub fn split_args(template: &str, file: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let (mut quoted, mut any) = (false, false);
    for c in template.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => cur.push(c),
        }
    }
    if any || !cur.is_empty() {
        out.push(cur);
    }
    let f = file.display().to_string();
    let mut args: Vec<String> = out.into_iter().map(|a| a.replace("{file}", &f)).collect();
    if !template.contains("{file}") {
        args.push(f);
    }
    args
}

/// Scan `file` with `kind` ("defender" | "custom").
pub async fn scan(kind: &str, program: &str, args: &str, file: &Path) -> Result<Outcome> {
    let (exe, argv) = match kind {
        "defender" => {
            let exe = defender().ok_or_else(|| anyhow!("Microsoft Defender's scanner (MpCmdRun.exe) was not found."))?;
            (exe, vec!["-Scan".into(), "-ScanType".into(), "3".into(), "-File".into(), file.display().to_string()])
        }
        "custom" => {
            let p = PathBuf::from(program.trim());
            if !p.is_file() {
                bail!("The virus scanner program was not found: {}", p.display());
            }
            (p, split_args(if args.trim().is_empty() { "\"{file}\"" } else { args }, file))
        }
        _ => bail!("Virus scanning is off"),
    };
    let mut cmd = Command::new(&exe);
    cmd.args(&argv).kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let out = tokio::time::timeout(std::time::Duration::from_secs(15 * 60), cmd.output()).await.map_err(|_| anyhow!("The virus scan took longer than 15 minutes"))??;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let code = out.status.code().unwrap_or(-1);
    match (kind, code) {
        (_, 0) => Ok(Outcome::Clean),
        // MpCmdRun: 2 = threats found (Defender handles them as configured).
        ("defender", 2) => Ok(Outcome::Threat(text.lines().find(|l| l.contains("Threat")).unwrap_or("Microsoft Defender found a threat.").trim().to_string())),
        ("defender", c) => bail!("Microsoft Defender could not scan the file (exit code {c})."),
        (_, c) => Ok(Outcome::Threat(format!("The scanner reported a problem (exit code {c}). {}", text.lines().last().unwrap_or("").trim()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argument_templates() {
        let f = Path::new("/tmp/a b.zip");
        assert_eq!(split_args("\"{file}\"", f), ["/tmp/a b.zip"]);
        assert_eq!(split_args("--scan \"{file}\" -q", f), ["--scan", "/tmp/a b.zip", "-q"]);
        assert_eq!(split_args("-r", f), ["-r", "/tmp/a b.zip"]);
        assert_eq!(split_args("", f), ["/tmp/a b.zip"]);
    }
}
