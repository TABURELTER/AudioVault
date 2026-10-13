//! BLAKE3 content hashing.
//!
//! *Quick hash*: size + first and last 64 KiB — cheap (two small reads) and
//! used for move detection and duplicate blocking. *Full hash*: streaming
//! BLAKE3 over the whole file; heavy I/O, scheduled per storage device.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::{Cancel, MediaError, Result};

const EDGE: u64 = 64 * 1024;
const BUF: usize = 1024 * 1024;

pub fn quick_hash(path: &Path) -> Result<String> {
    let mut f = File::open(path)?;
    let size = f.metadata()?.len();
    let mut h = blake3::Hasher::new();
    h.update(b"avq1");
    h.update(&size.to_le_bytes());
    let mut buf = vec![0u8; EDGE as usize];
    let n = read_up_to(&mut f, &mut buf)?;
    h.update(&buf[..n]);
    if size > EDGE * 2 {
        f.seek(SeekFrom::Start(size - EDGE))?;
        let n = read_up_to(&mut f, &mut buf)?;
        h.update(&buf[..n]);
    } else if size > EDGE {
        let n = read_up_to(&mut f, &mut buf)?;
        h.update(&buf[..n]);
    }
    Ok(h.finalize().to_hex()[..32].to_string())
}

fn read_up_to(f: &mut File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut total = 0;
    while total < buf.len() {
        match f.read(&mut buf[total..])? {
            0 => break,
            n => total += n,
        }
    }
    Ok(total)
}

/// Full-content BLAKE3. `progress(bytes_done)` is called about every 8 MiB.
pub fn full_hash(
    path: &Path,
    cancel: &dyn Cancel,
    progress: &mut dyn FnMut(u64),
) -> Result<String> {
    let mut f = File::open(path)?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; BUF];
    let mut done: u64 = 0;
    let mut since_report: u64 = 0;
    loop {
        if cancel.is_cancelled() {
            return Err(MediaError::Cancelled);
        }
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        done += n as u64;
        since_report += n as u64;
        if since_report >= 8 * BUF as u64 {
            progress(done);
            since_report = 0;
        }
    }
    progress(done);
    Ok(h.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NeverCancel;

    #[test]
    fn hashes_are_stable_and_discriminating() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.bin");
        let b = dir.path().join("b.bin");
        let mut data = vec![7u8; 300_000];
        std::fs::write(&a, &data).unwrap();
        data[150_000] = 8; // middle byte differs: quick hash equal, full hash differs
        std::fs::write(&b, &data).unwrap();
        assert_eq!(quick_hash(&a).unwrap(), quick_hash(&b).unwrap());
        let fa = full_hash(&a, &NeverCancel, &mut |_| {}).unwrap();
        let fb = full_hash(&b, &NeverCancel, &mut |_| {}).unwrap();
        assert_ne!(fa, fb);
        assert_eq!(fa, full_hash(&a, &NeverCancel, &mut |_| {}).unwrap());
    }

    #[test]
    fn cancellation() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.bin");
        std::fs::write(&a, vec![1u8; 10]).unwrap();
        let cancel = std::sync::atomic::AtomicBool::new(true);
        assert!(matches!(
            full_hash(&a, &cancel, &mut |_| {}),
            Err(MediaError::Cancelled)
        ));
    }
}
