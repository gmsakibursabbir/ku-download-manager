//! File hashing for integrity verification.

use anyhow::{bail, Result};
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};
use std::io::Read;
use std::path::Path;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn digest<D: Digest>(path: &Path) -> Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = D::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

/// Normalize algorithm names to aria2's spelling (`sha-256`).
pub fn normalize_algo(a: &str) -> Option<&'static str> {
    match a.to_ascii_lowercase().replace(['-', '_'], "").as_str() {
        "md5" => Some("md5"),
        "sha1" => Some("sha-1"),
        "sha256" => Some("sha-256"),
        "sha512" => Some("sha-512"),
        _ => None,
    }
}

/// Parse `algo=hex` (or `algo:hex`) and validate the digest length.
pub fn parse_checksum(s: &str) -> Result<(String, String)> {
    let (a, h) = s.split_once(['=', ':']).unwrap_or(("sha-256", s));
    let Some(algo) = normalize_algo(a.trim()) else { bail!("Unsupported hash algorithm \"{a}\"") };
    let h = h.trim().to_ascii_lowercase();
    let want = match algo {
        "md5" => 32,
        "sha-1" => 40,
        "sha-256" => 64,
        _ => 128,
    };
    if h.len() != want || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("A {algo} checksum must be {want} hexadecimal characters");
    }
    Ok((algo.to_string(), h))
}

pub fn hash_file(path: &Path, algo: &str) -> Result<String> {
    match normalize_algo(algo) {
        Some("md5") => digest::<md5::Md5>(path),
        Some("sha-1") => digest::<Sha1>(path),
        Some("sha-256") => digest::<Sha256>(path),
        Some("sha-512") => digest::<Sha512>(path),
        _ => bail!("Unsupported hash algorithm \"{algo}\""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_and_parses() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("f");
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(
            hash_file(&p, "sha256").unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(hash_file(&p, "md5").unwrap(), "900150983cd24fb0d6963f7d28e17f72");
        assert!(parse_checksum("sha256=abc").is_err());
        let (a, _) = parse_checksum("SHA-1:a9993e364706816aba3e25717850c26c9cd0d89d").unwrap();
        assert_eq!(a, "sha-1");
    }
}
