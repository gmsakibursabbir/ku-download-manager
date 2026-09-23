//! Minimal bencode reader for showing .torrent metadata before adding it.

use anyhow::{bail, Result};
use serde::Serialize;
use sha1::{Digest, Sha1};

#[derive(Debug, Clone, PartialEq)]
enum B<'a> {
    Int(i64),
    Bytes(&'a [u8]),
    List(Vec<B<'a>>),
    Dict(Vec<(&'a [u8], B<'a>)>),
}

struct Parser<'a> {
    s: &'a [u8],
    pos: usize,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Result<u8> {
        self.s.get(self.pos).copied().ok_or_else(|| anyhow::anyhow!("unexpected end of torrent data"))
    }

    fn value(&mut self) -> Result<B<'a>> {
        self.depth += 1;
        if self.depth > 64 {
            bail!("torrent data nested too deeply");
        }
        let v = match self.peek()? {
            b'i' => {
                self.pos += 1;
                let end = self.find(b'e')?;
                let n = std::str::from_utf8(&self.s[self.pos..end])?.parse()?;
                self.pos = end + 1;
                B::Int(n)
            }
            b'l' => {
                self.pos += 1;
                let mut items = Vec::new();
                while self.peek()? != b'e' {
                    items.push(self.value()?);
                }
                self.pos += 1;
                B::List(items)
            }
            b'd' => {
                self.pos += 1;
                let mut items = Vec::new();
                while self.peek()? != b'e' {
                    let B::Bytes(k) = self.bytes()? else { unreachable!() };
                    items.push((k, self.value()?));
                }
                self.pos += 1;
                B::Dict(items)
            }
            b'0'..=b'9' => self.bytes()?,
            c => bail!("invalid torrent data (byte {c:#x} at {})", self.pos),
        };
        self.depth -= 1;
        Ok(v)
    }

    fn bytes(&mut self) -> Result<B<'a>> {
        let colon = self.find(b':')?;
        let len: usize = std::str::from_utf8(&self.s[self.pos..colon])?.parse()?;
        let start = colon + 1;
        let end = start.checked_add(len).filter(|e| *e <= self.s.len()).ok_or_else(|| anyhow::anyhow!("truncated torrent"))?;
        self.pos = end;
        Ok(B::Bytes(&self.s[start..end]))
    }

    fn find(&self, c: u8) -> Result<usize> {
        self.s[self.pos..]
            .iter()
            .position(|b| *b == c)
            .map(|p| p + self.pos)
            .ok_or_else(|| anyhow::anyhow!("malformed torrent data"))
    }
}

impl<'a> B<'a> {
    fn get(&self, key: &str) -> Option<&B<'a>> {
        match self {
            B::Dict(items) => items.iter().find(|(k, _)| *k == key.as_bytes()).map(|(_, v)| v),
            _ => None,
        }
    }
    fn str(&self) -> Option<String> {
        match self {
            B::Bytes(b) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        }
    }
    fn int(&self) -> Option<i64> {
        match self {
            B::Int(i) => Some(*i),
            _ => None,
        }
    }
}

#[derive(Serialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct TorrentFile {
    pub index: usize,
    pub path: String,
    pub length: i64,
}

#[derive(Serialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct TorrentInfo {
    pub name: String,
    pub info_hash: String,
    pub total: i64,
    pub piece_length: i64,
    pub private: bool,
    pub comment: Option<String>,
    pub created_by: Option<String>,
    pub trackers: Vec<String>,
    pub files: Vec<TorrentFile>,
}

pub fn parse(data: &[u8]) -> Result<TorrentInfo> {
    let mut p = Parser { s: data, pos: 0, depth: 0 };
    let root = p.value()?;
    let Some(info) = root.get("info") else { bail!("not a torrent file (no info dictionary)") };
    // Re-locate the raw bytes of the info dictionary for the info-hash.
    let raw_info = raw_info_slice(data)?;
    let hash = Sha1::digest(raw_info);
    let name = info.get("name.utf-8").or(info.get("name")).and_then(B::str).unwrap_or_default();
    let mut files = Vec::new();
    if let Some(B::List(list)) = info.get("files") {
        for (i, f) in list.iter().enumerate() {
            let parts: Vec<String> = match f.get("path.utf-8").or(f.get("path")) {
                Some(B::List(ps)) => ps.iter().filter_map(B::str).collect(),
                _ => Vec::new(),
            };
            files.push(TorrentFile {
                index: i + 1,
                path: std::iter::once(name.clone()).chain(parts).collect::<Vec<_>>().join("/"),
                length: f.get("length").and_then(B::int).unwrap_or(0),
            });
        }
    } else {
        files.push(TorrentFile { index: 1, path: name.clone(), length: info.get("length").and_then(B::int).unwrap_or(0) });
    }
    let mut trackers = Vec::new();
    if let Some(a) = root.get("announce").and_then(B::str) {
        trackers.push(a);
    }
    if let Some(B::List(tiers)) = root.get("announce-list") {
        for tier in tiers {
            if let B::List(urls) = tier {
                for u in urls.iter().filter_map(B::str) {
                    if !trackers.contains(&u) {
                        trackers.push(u);
                    }
                }
            }
        }
    }
    Ok(TorrentInfo {
        total: files.iter().map(|f| f.length).sum(),
        name,
        info_hash: hash.iter().map(|b| format!("{b:02x}")).collect(),
        piece_length: info.get("piece length").and_then(B::int).unwrap_or(0),
        private: info.get("private").and_then(B::int) == Some(1),
        comment: root.get("comment").and_then(B::str),
        created_by: root.get("created by").and_then(B::str),
        trackers,
        files,
    })
}

fn raw_info_slice(data: &[u8]) -> Result<&[u8]> {
    // Walk the top-level dictionary and return the exact span of "info".
    let mut p = Parser { s: data, pos: 1, depth: 0 };
    if data.first() != Some(&b'd') {
        bail!("not a torrent file");
    }
    while p.peek()? != b'e' {
        let B::Bytes(k) = p.bytes()? else { unreachable!() };
        let start = p.pos;
        p.value()?;
        if k == b"info" {
            return Ok(&data[start..p.pos]);
        }
    }
    bail!("no info dictionary")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multi_file_torrent() {
        let info = b"d5:filesld6:lengthi10e4:pathl1:aeed6:lengthi5e4:pathl3:sub1:beee4:name3:dir12:piece lengthi16384e6:pieces0:e";
        let mut t = b"d8:announce14:http://t/annce4:info".to_vec();
        t.extend_from_slice(info);
        t.push(b'e');
        let parsed = parse(&t).unwrap();
        assert_eq!(parsed.name, "dir");
        assert_eq!(parsed.total, 15);
        assert_eq!(parsed.files[1].path, "dir/sub/b");
        assert_eq!(parsed.trackers, vec!["http://t/annce".to_string()]);
        let expected: String = Sha1::digest(info).iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(parsed.info_hash, expected);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse(b"hello").is_err());
        assert!(parse(b"d4:infod4:name").is_err());
    }
}
