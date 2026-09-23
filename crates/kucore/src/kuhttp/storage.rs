//! Segment-aware file storage.
//!
//! Strategy: one preallocated temporary file (`name.kudownload`) receiving
//! validated positional writes. Workers own disjoint ranges from the segment
//! map, so they never write the same bytes, and nothing is ever appended
//! blindly. Durability: progress is only persisted after `sync_data`.
//! Finalization is an atomic rename to the real name.

use super::config::WriteFault;
use super::error::KuError;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const TEMP_EXT: &str = "kudownload";

pub fn temp_path_for(final_path: &Path) -> PathBuf {
    let mut s = final_path.as_os_str().to_os_string();
    s.push(".");
    s.push(TEMP_EXT);
    PathBuf::from(s)
}

pub fn state_path_for(temp: &Path) -> PathBuf {
    let mut s = temp.as_os_str().to_os_string();
    s.push(".json");
    PathBuf::from(s)
}

/// Resolve `name` inside `dir`, refusing anything that could escape it.
pub fn safe_join(dir: &Path, name: &str) -> Result<PathBuf, KuError> {
    let clean = crate::classify::sanitize_filename(name);
    if clean.is_empty() {
        return Err(KuError::Io("invalid file name".into()));
    }
    let p = dir.join(&clean);
    if p.parent() != Some(dir) {
        return Err(KuError::Io("file name escapes the destination folder".into()));
    }
    Ok(p)
}

/// Separate handles for writing and syncing: synchronous handles serialise
/// all I/O on their file object, so a long fsync would otherwise stall every
/// worker; a small pool keeps parallel positional writes parallel.
const WRITE_HANDLES: usize = 4;

#[derive(Clone)]
pub struct Storage {
    pub path: PathBuf,
    file: Arc<File>,
    writers: Arc<Vec<File>>,
    next: Arc<std::sync::atomic::AtomicUsize>,
    fault: Option<WriteFault>,
}

impl Storage {
    /// Open (or create) the temporary file. `fresh` truncates it; a known
    /// size is preallocated so positional writes never extend the file.
    pub fn open(path: &Path, total: Option<u64>, fresh: bool, sparse: bool, fault: Option<WriteFault>) -> Result<Storage, KuError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| KuError::from_io(&e))?;
        }
        // Refuse to follow a symlink planted at the temp path.
        if let Ok(meta) = std::fs::symlink_metadata(path) {
            if meta.file_type().is_symlink() {
                return Err(KuError::Io("the temporary path is a symbolic link".into()));
            }
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(fresh)
            .open(path)
            .map_err(|e| KuError::from_io(&e))?;
        if sparse && fresh && total.is_some_and(|t| t > 0) {
            // Without this NTFS zero-fills everything below a write offset
            // first, serialising parallel segments. Cleared at finalize.
            set_sparse(&file, true);
        }
        if let Some(t) = total {
            let len = file.metadata().map_err(|e| KuError::from_io(&e))?.len();
            if len != t {
                file.set_len(t).map_err(|e| KuError::from_io(&e))?;
            }
        }
        let mut writers = Vec::with_capacity(WRITE_HANDLES);
        for _ in 0..WRITE_HANDLES {
            writers.push(OpenOptions::new().read(true).write(true).open(path).map_err(|e| KuError::from_io(&e))?);
        }
        Ok(Storage {
            path: path.to_path_buf(),
            file: Arc::new(file),
            writers: Arc::new(writers),
            next: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            fault,
        })
    }

    pub fn len(&self) -> u64 {
        self.file.metadata().map(|m| m.len()).unwrap_or(0)
    }

    /// Write `data` at `offset` (blocking pool; the buffer is moved, not copied).
    pub async fn write_at(&self, offset: u64, data: Vec<u8>) -> Result<Vec<u8>, KuError> {
        if let Some(f) = &self.fault {
            if let Some(e) = f(offset, data.len()) {
                return Err(KuError::from_io(&e));
            }
        }
        let writers = self.writers.clone();
        let i = self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % writers.len();
        tokio::task::spawn_blocking(move || {
            write_all_at(&writers[i], &data, offset)?;
            Ok::<_, std::io::Error>(data)
        })
        .await
        .map_err(|e| KuError::Io(e.to_string()))?
        .map_err(|e| KuError::from_io(&e))
    }

    pub async fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>, KuError> {
        let file = self.file.clone();
        tokio::task::spawn_blocking(move || {
            let mut buf = vec![0u8; len];
            read_exact_at(&file, &mut buf, offset)?;
            Ok::<_, std::io::Error>(buf)
        })
        .await
        .map_err(|e| KuError::Io(e.to_string()))?
        .map_err(|e| KuError::from_io(&e))
    }

    pub async fn sync(&self) -> Result<(), KuError> {
        let file = self.file.clone();
        tokio::task::spawn_blocking(move || file.sync_data())
            .await
            .map_err(|e| KuError::Io(e.to_string()))?
            .map_err(|e| KuError::from_io(&e))
    }

    /// Fully written: turn the sparse attribute off again.
    pub fn densify(&self) {
        set_sparse(&self.file, false);
    }

    /// Truncate to the real length (unknown-size downloads).
    pub fn set_len(&self, len: u64) -> Result<(), KuError> {
        self.file.set_len(len).map_err(|e| KuError::from_io(&e))
    }
}

#[cfg(windows)]
pub fn set_sparse(f: &File, on: bool) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Ioctl::{FILE_SET_SPARSE_BUFFER, FSCTL_SET_SPARSE};
    use windows_sys::Win32::System::IO::DeviceIoControl;
    let buf = FILE_SET_SPARSE_BUFFER { SetSparse: on as u8 };
    let mut ret = 0u32;
    // Best effort: FAT/exFAT do not support sparse files.
    unsafe {
        DeviceIoControl(
            f.as_raw_handle() as _,
            FSCTL_SET_SPARSE,
            &buf as *const _ as *const _,
            std::mem::size_of::<FILE_SET_SPARSE_BUFFER>() as u32,
            std::ptr::null_mut(),
            0,
            &mut ret,
            std::ptr::null_mut(),
        );
    }
}

#[cfg(not(windows))]
pub fn set_sparse(_f: &File, _on: bool) {}

#[cfg(unix)]
fn write_all_at(f: &File, buf: &[u8], offset: u64) -> std::io::Result<()> {
    use std::os::unix::fs::FileExt;
    f.write_all_at(buf, offset)
}

#[cfg(windows)]
fn write_all_at(f: &File, mut buf: &[u8], mut offset: u64) -> std::io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !buf.is_empty() {
        let n = f.seek_write(buf, offset)?;
        if n == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::WriteZero, "write returned 0"));
        }
        buf = &buf[n..];
        offset += n as u64;
    }
    Ok(())
}

#[cfg(unix)]
fn read_exact_at(f: &File, buf: &mut [u8], offset: u64) -> std::io::Result<()> {
    use std::os::unix::fs::FileExt;
    f.read_exact_at(buf, offset)
}

#[cfg(windows)]
fn read_exact_at(f: &File, mut buf: &mut [u8], mut offset: u64) -> std::io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !buf.is_empty() {
        let n = f.seek_read(buf, offset)?;
        if n == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "short read"));
        }
        buf = &mut buf[n..];
        offset += n as u64;
    }
    Ok(())
}

/// Free bytes on the volume holding `dir` (None if unknown).
pub fn available_space(dir: &Path) -> Option<u64> {
    let mut d = dir.to_path_buf();
    while !d.exists() {
        d = d.parent()?.to_path_buf();
    }
    fs4::available_space(&d).ok()
}

/// Atomically move the finished temp file into place. Returns the final path
/// actually used (a free " (n)" variant if the name got taken meanwhile).
pub fn finalize(temp: &Path, wanted: &Path, overwrite: bool) -> Result<PathBuf, KuError> {
    let mut target = wanted.to_path_buf();
    if target.exists() && !overwrite {
        let stem = wanted.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let ext = wanted.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        let dir = wanted.parent().unwrap_or(Path::new("."));
        target = (1..10_000)
            .map(|i| dir.join(format!("{stem} ({i}){ext}")))
            .find(|p| !p.exists())
            .ok_or_else(|| KuError::Io("no free file name".into()))?;
    }
    std::fs::rename(temp, &target).map_err(|e| KuError::from_io(&e))?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_join_rejects_traversal() {
        let d = Path::new("C:/dl");
        assert_eq!(safe_join(d, "../../evil.txt").unwrap(), d.join("evil.txt"));
        assert_eq!(safe_join(d, "a/b/c.zip").unwrap(), d.join("c.zip"));
        assert!(safe_join(d, "..").is_err());
    }

    #[tokio::test]
    async fn positional_writes_and_finalize() {
        let dir = tempfile::tempdir().unwrap();
        let fin = dir.path().join("f.bin");
        let tmp = temp_path_for(&fin);
        let s = Storage::open(&tmp, Some(10), true, true, None).unwrap();
        s.write_at(5, b"world".to_vec()).await.unwrap();
        s.write_at(0, b"hello".to_vec()).await.unwrap();
        s.sync().await.unwrap();
        assert_eq!(s.read_at(0, 10).await.unwrap(), b"helloworld");
        drop(s);
        std::fs::write(&fin, b"taken").unwrap();
        let out = finalize(&tmp, &fin, false).unwrap();
        assert_eq!(out, dir.path().join("f (1).bin"));
        assert_eq!(std::fs::read(out).unwrap(), b"helloworld");
        assert!(available_space(dir.path()).unwrap() > 0);
    }
}
