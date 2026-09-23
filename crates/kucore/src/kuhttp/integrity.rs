//! Size and checksum verification.

use super::error::KuError;
use reqwest::header::HeaderMap;
use std::io::Read;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checksum {
    /// md5 / sha-1 / sha-256 / sha-512 / blake3
    pub algo: String,
    pub hex: String,
}

impl Checksum {
    /// Parse `algo=hex` / `algo:hex` (reuses KuCore's parser, adds BLAKE3).
    pub fn parse(s: &str) -> Result<Checksum, KuError> {
        let (a, h) = s.split_once(['=', ':']).unwrap_or(("sha-256", s));
        if a.trim().eq_ignore_ascii_case("blake3") {
            let h = h.trim().to_ascii_lowercase();
            if h.len() != 64 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(KuError::InvalidHeader("a BLAKE3 checksum must be 64 hex characters".into()));
            }
            return Ok(Checksum { algo: "blake3".into(), hex: h });
        }
        let (algo, hex) = crate::hash::parse_checksum(s).map_err(|e| KuError::InvalidHeader(e.to_string()))?;
        Ok(Checksum { algo, hex })
    }
}

/// Hash a file with the given algorithm, streaming (never loads it whole).
pub fn hash_file(path: &Path, algo: &str) -> Result<String, KuError> {
    if algo == "blake3" {
        let mut f = std::fs::File::open(path).map_err(|e| KuError::from_io(&e))?;
        let mut h = blake3::Hasher::new();
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = f.read(&mut buf).map_err(|e| KuError::from_io(&e))?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }
        return Ok(h.finalize().to_hex().to_string());
    }
    crate::hash::hash_file(path, algo).map_err(|e| KuError::Io(e.to_string()))
}

pub async fn verify(path: &Path, want: &Checksum) -> Result<(), KuError> {
    let p = path.to_path_buf();
    let algo = want.algo.clone();
    let actual = tokio::task::spawn_blocking(move || hash_file(&p, &algo)).await.map_err(|e| KuError::Io(e.to_string()))??;
    if actual != want.hex {
        return Err(KuError::VerificationFailed { algo: want.algo.clone(), expected: want.hex.clone(), actual });
    }
    Ok(())
}

/// Checksums servers announce for the full representation:
/// RFC 3230 `Digest`, and the common `X-Checksum-*` headers.
pub fn server_checksum(h: &HeaderMap) -> Option<(String, String)> {
    use base64::Engine as _;
    if let Some(d) = h.get("digest").and_then(|v| v.to_str().ok()) {
        for part in d.split(',') {
            let Some((a, v)) = part.trim().split_once('=') else { continue };
            let algo = match a.trim().to_ascii_lowercase().as_str() {
                "sha-256" => "sha-256",
                "sha-512" => "sha-512",
                "md5" => "md5",
                _ => continue,
            };
            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(v.trim()) {
                return Some((algo.into(), bytes.iter().map(|b| format!("{b:02x}")).collect()));
            }
        }
    }
    for (name, algo) in [("x-checksum-sha256", "sha-256"), ("x-checksum-sha512", "sha-512"), ("x-checksum-md5", "md5")] {
        if let Some(v) = h.get(name).and_then(|v| v.to_str().ok()) {
            let v = v.trim().to_ascii_lowercase();
            if v.chars().all(|c| c.is_ascii_hexdigit()) && !v.is_empty() {
                return Some((algo.into(), v));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_checksums_and_digest_headers() {
        assert_eq!(Checksum::parse(&format!("blake3={}", "a".repeat(64))).unwrap().algo, "blake3");
        assert!(Checksum::parse("sha-256=zz").is_err());
        let mut h = HeaderMap::new();
        // sha-256("abc") in base64
        h.insert("digest", "SHA-256=ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0=".parse().unwrap());
        assert_eq!(server_checksum(&h).unwrap().1, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn hashes_blake3() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("x");
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(hash_file(&p, "blake3").unwrap(), blake3::hash(b"abc").to_hex().to_string());
    }
}
