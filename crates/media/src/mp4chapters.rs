//! Minimal MP4/M4B box walker that counts chapters without loading the file:
//! Nero `chpl` chapters (moov/udta/chpl) and QuickTime chapter tracks
//! (a `tref/chap` reference to a text track whose sample count is the
//! number of chapters). Only box headers and tiny boxes are read.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const MAX_DEPTH: usize = 8;

#[derive(Default)]
struct State {
    nero: Option<u32>,
    chap_refs: Vec<u32>,
    track_samples: HashMap<u32, u32>,
    current_track: Option<u32>,
}

fn read_exact_at(f: &mut File, pos: u64, buf: &mut [u8]) -> std::io::Result<()> {
    f.seek(SeekFrom::Start(pos))?;
    f.read_exact(buf)
}

fn walk(f: &mut File, start: u64, end: u64, depth: usize, st: &mut State) -> std::io::Result<()> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    let mut pos = start;
    while pos + 8 <= end {
        let mut hdr = [0u8; 8];
        read_exact_at(f, pos, &mut hdr)?;
        let mut size = u32::from_be_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]) as u64;
        let kind = [hdr[4], hdr[5], hdr[6], hdr[7]];
        let mut header_len = 8u64;
        if size == 1 {
            let mut large = [0u8; 8];
            read_exact_at(f, pos + 8, &mut large)?;
            size = u64::from_be_bytes(large);
            header_len = 16;
        } else if size == 0 {
            size = end - pos;
        }
        if size < header_len || pos + size > end {
            break; // corrupt box; stop quietly
        }
        let body = pos + header_len;
        let body_end = pos + size;
        match &kind {
            b"moov" | b"udta" | b"mdia" | b"minf" | b"stbl" | b"edts" => walk(f, body, body_end, depth + 1, st)?,
            b"trak" => {
                st.current_track = None;
                walk(f, body, body_end, depth + 1, st)?;
                st.current_track = None;
            }
            b"meta" => walk(f, body + 4, body_end, depth + 1, st)?, // full box
            b"tkhd" => {
                let mut v = [0u8; 1];
                read_exact_at(f, body, &mut v)?;
                // version 1: 8-byte times → track_ID at offset 4+8+8; version 0: 4+4+4.
                let off = if v[0] == 1 { 4 + 16 } else { 4 + 8 };
                let mut id = [0u8; 4];
                read_exact_at(f, body + off, &mut id)?;
                st.current_track = Some(u32::from_be_bytes(id));
            }
            b"tref" => {
                let mut p = body;
                while p + 8 <= body_end {
                    let mut h = [0u8; 8];
                    read_exact_at(f, p, &mut h)?;
                    let s = u32::from_be_bytes([h[0], h[1], h[2], h[3]]) as u64;
                    if s < 8 || p + s > body_end {
                        break;
                    }
                    if &h[4..8] == b"chap" {
                        let n = ((s - 8) / 4).min(64);
                        for i in 0..n {
                            let mut id = [0u8; 4];
                            read_exact_at(f, p + 8 + i * 4, &mut id)?;
                            st.chap_refs.push(u32::from_be_bytes(id));
                        }
                    }
                    p += s;
                }
            }
            b"stsz" => {
                let mut b = [0u8; 12];
                read_exact_at(f, body, &mut b)?;
                let count = u32::from_be_bytes([b[8], b[9], b[10], b[11]]);
                if let Some(t) = st.current_track {
                    st.track_samples.insert(t, count);
                }
            }
            b"chpl" => {
                let mut v = [0u8; 4];
                read_exact_at(f, body, &mut v)?;
                let count_pos = if v[0] != 0 { body + 8 } else { body + 4 };
                let mut n = [0u8; 1];
                read_exact_at(f, count_pos, &mut n)?;
                st.nero = Some(n[0] as u32);
            }
            _ => {}
        }
        pos = body_end;
    }
    Ok(())
}

/// Number of chapters in an MP4-family file (0 when none or unreadable).
pub fn count_chapters(path: &Path) -> u32 {
    let Ok(mut f) = File::open(path) else { return 0 };
    let Ok(len) = f.metadata().map(|m| m.len()) else { return 0 };
    let mut st = State::default();
    if walk(&mut f, 0, len, 0, &mut st).is_err() {
        return st.nero.unwrap_or(0);
    }
    let qt: u32 = st.chap_refs.iter().filter_map(|id| st.track_samples.get(id)).copied().max().unwrap_or(0);
    qt.max(st.nero.unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut v = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        v.extend_from_slice(kind);
        v.extend_from_slice(body);
        v
    }

    #[test]
    fn counts_nero_and_quicktime_chapters() {
        // Nero chpl with 3 chapters (version 0).
        let chpl = bx(b"chpl", &[0, 0, 0, 0, 3]);
        // Audio track 1 referencing chapter track 2; track 2 has 5 samples.
        let tkhd1 = bx(b"tkhd", &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        let tref = bx(b"tref", &bx(b"chap", &2u32.to_be_bytes()));
        let trak1 = bx(b"trak", &[tkhd1, tref].concat());
        let tkhd2 = bx(b"tkhd", &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2]);
        let stsz = bx(b"stsz", &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5]);
        let mdia = bx(b"mdia", &bx(b"minf", &bx(b"stbl", &stsz)));
        let trak2 = bx(b"trak", &[tkhd2, mdia].concat());
        let moov = bx(b"moov", &[trak1, trak2, bx(b"udta", &chpl)].concat());
        let file = [bx(b"ftyp", b"M4B \0\0\0\0"), moov].concat();
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.m4b");
        std::fs::write(&p, file).unwrap();
        assert_eq!(count_chapters(&p), 5);
    }

    #[test]
    fn garbage_is_zero() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.m4b");
        std::fs::write(&p, b"\xff\xff\xff\xffjunkjunk").unwrap();
        assert_eq!(count_chapters(&p), 0);
    }
}
