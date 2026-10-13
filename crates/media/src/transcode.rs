//! FFmpeg transcoding for the storage optimizer.
//!
//! Safety rules enforced here:
//! * the source files are never modified or deleted;
//! * output is written to a hidden temporary file next to the destination
//!   (`.name.avpart.ext`), validated (probe + duration check) and only then
//!   atomically renamed into place;
//! * cancellation kills FFmpeg and removes the partial output.
//!
//! The command line is produced by the pure [`build_plan`] function so it can
//! be unit-tested without FFmpeg installed.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use audiovault_core::optimize::TargetFormat;

use crate::probe::probe_file;
use crate::{Cancel, MediaError, Result};

#[cfg(windows)]
const FFMPEG_EXE: &str = "ffmpeg.exe";
#[cfg(not(windows))]
const FFMPEG_EXE: &str = "ffmpeg";

/// Find an FFmpeg executable. Order: explicitly configured path → bundled
/// resource directories → system `PATH`. Nothing is ever downloaded.
pub fn locate_ffmpeg(configured: Option<&Path>, bundled_dirs: &[PathBuf]) -> Option<PathBuf> {
    if let Some(p) = configured {
        if p.is_file() {
            return Some(p.to_path_buf());
        }
        let candidate = p.join(FFMPEG_EXE);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    for dir in bundled_dirs {
        let candidate = dir.join(FFMPEG_EXE);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|d| d.join(FFMPEG_EXE))
        .find(|c| c.is_file())
}

fn base_command(program: &Path) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.stdin(Stdio::null());
    cmd
}

/// First line of `ffmpeg -version`, e.g. "ffmpeg version 8.0.1 ...".
pub fn ffmpeg_version(ffmpeg: &Path) -> Result<String> {
    let out = base_command(ffmpeg)
        .arg("-hide_banner")
        .arg("-version")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| MediaError::ToolMissing(format!("{}: {e}", ffmpeg.display())))?;
    if !out.status.success() {
        return Err(MediaError::ToolMissing(format!(
            "{} -version failed",
            ffmpeg.display()
        )));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text.lines().next().unwrap_or("ffmpeg").trim().to_string())
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranscodeInput {
    pub path: PathBuf,
    pub duration_ms: u64,
    /// Chapter title when several inputs are joined (defaults to file stem).
    pub chapter_title: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TranscodeMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub narrator: Option<String>,
    pub album: Option<String>,
    pub year: Option<i32>,
    pub genre: Option<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranscodeRequest {
    pub inputs: Vec<TranscodeInput>,
    /// Final destination path. Must not exist yet.
    pub output: PathBuf,
    pub target: TargetFormat,
    /// `None` with `reencode == false` means stream copy.
    pub bitrate_kbps: Option<u32>,
    pub reencode: bool,
    pub mono: bool,
    pub metadata: TranscodeMetadata,
    pub cover: Option<PathBuf>,
}

impl TranscodeRequest {
    pub fn expected_duration_ms(&self) -> u64 {
        self.inputs.iter().map(|i| i.duration_ms).sum()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chapter {
    pub start_ms: u64,
    pub end_ms: u64,
    pub title: String,
}

/// Fully materialized FFmpeg invocation.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscodePlan {
    pub args: Vec<OsString>,
    pub concat_list_path: PathBuf,
    pub concat_list: String,
    pub ffmetadata_path: PathBuf,
    pub ffmetadata: String,
    pub temp_output: PathBuf,
}

/// Hidden temporary output path next to `final_path`: `.name.avpart.ext`.
pub fn temp_output_path(final_path: &Path) -> PathBuf {
    let dir = final_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = final_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".into());
    let ext = final_path
        .extension()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = if ext.is_empty() {
        format!(".{stem}.avpart")
    } else {
        format!(".{stem}.avpart.{ext}")
    };
    dir.join(name)
}

/// One line of an FFmpeg concat-demuxer list. Single quotes are escaped the
/// way the demuxer expects (`'` → `'\''`).
pub fn concat_line(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    format!("file '{}'\n", s.replace('\'', "'\\''"))
}

/// Escape a value for the FFMETADATA1 format.
pub fn escape_ffmetadata(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '=' | ';' | '#' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            '\n' => out.push_str("\\\n"),
            '\r' => {}
            c => out.push(c),
        }
    }
    out
}

/// Chapters derived from consecutive input durations.
pub fn chapters_from_inputs(inputs: &[TranscodeInput]) -> Vec<Chapter> {
    let mut start = 0u64;
    inputs
        .iter()
        .enumerate()
        .map(|(i, input)| {
            let title = input
                .chapter_title
                .clone()
                .filter(|t| !t.trim().is_empty())
                .unwrap_or_else(|| {
                    input
                        .path
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| format!("Chapter {}", i + 1))
                });
            let ch = Chapter {
                start_ms: start,
                end_ms: start + input.duration_ms,
                title,
            };
            start += input.duration_ms;
            ch
        })
        .collect()
}

pub fn build_ffmetadata(meta: &TranscodeMetadata, chapters: &[Chapter]) -> String {
    let mut s = String::from(";FFMETADATA1\n");
    let mut kv = |k: &str, v: &Option<String>| {
        if let Some(v) = v.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            s.push_str(k);
            s.push('=');
            s.push_str(&escape_ffmetadata(v));
            s.push('\n');
        }
    };
    kv("title", &meta.title);
    kv("artist", &meta.author);
    kv("album_artist", &meta.author);
    kv("album", &meta.album.clone().or_else(|| meta.title.clone()));
    kv("composer", &meta.narrator);
    kv(
        "genre",
        &meta.genre.clone().or_else(|| Some("Audiobook".into())),
    );
    kv("comment", &meta.comment);
    kv("date", &meta.year.map(|y| y.to_string()));
    for ch in chapters {
        s.push_str("[CHAPTER]\nTIMEBASE=1/1000\n");
        s.push_str(&format!("START={}\nEND={}\n", ch.start_ms, ch.end_ms));
        s.push_str(&format!("title={}\n", escape_ffmetadata(&ch.title)));
    }
    s
}

/// Build the complete FFmpeg plan. `work_dir` holds the list/metadata files.
pub fn build_plan(req: &TranscodeRequest, work_dir: &Path) -> Result<TranscodePlan> {
    if req.inputs.is_empty() {
        return Err(MediaError::Failed("no input files".into()));
    }
    if req.reencode && req.bitrate_kbps.is_none() {
        return Err(MediaError::Failed("re-encoding requires a bitrate".into()));
    }
    if !req.reencode && req.target == TargetFormat::Opus {
        return Err(MediaError::Failed(
            "stream copy into Opus is not supported".into(),
        ));
    }
    let concat_list: String = req.inputs.iter().map(|i| concat_line(&i.path)).collect();
    let joined = req.inputs.len() > 1;
    let chapters = if joined {
        chapters_from_inputs(&req.inputs)
    } else {
        Vec::new()
    };
    let ffmetadata = build_ffmetadata(&req.metadata, &chapters);
    let concat_list_path = work_dir.join("inputs.ffconcat.txt");
    let ffmetadata_path = work_dir.join("metadata.ffmeta.txt");
    let temp_output = temp_output_path(&req.output);

    let mut a: Vec<OsString> = Vec::new();
    let mut push = |s: &str| a.push(OsString::from(s));
    for s in [
        "-hide_banner",
        "-nostdin",
        "-loglevel",
        "error",
        "-nostats",
        "-progress",
        "pipe:1",
        "-y",
    ] {
        push(s);
    }
    for s in ["-f", "concat", "-safe", "0", "-i"] {
        push(s);
    }
    a.push(concat_list_path.clone().into_os_string());
    a.push("-f".into());
    a.push("ffmetadata".into());
    a.push("-i".into());
    a.push(ffmetadata_path.clone().into_os_string());
    let use_cover = req.cover.is_some()
        && (req.target == TargetFormat::M4bAac || req.target == TargetFormat::Mp3);
    if use_cover && let Some(cover) = &req.cover {
        a.push("-i".into());
        a.push(cover.clone().into_os_string());
    }
    let mut push = |s: &str| a.push(OsString::from(s));
    push("-map");
    push("0:a:0");
    if use_cover {
        push("-map");
        push("2:v:0");
    }
    push("-map_metadata");
    push("1");
    push("-map_chapters");
    // Joined inputs get chapters from the ffmetadata file; a single input
    // keeps its own chapters.
    push(if joined { "1" } else { "0" });
    if req.reencode {
        let kbps = req.bitrate_kbps.unwrap_or(64);
        match req.target {
            TargetFormat::M4bAac => {
                push("-c:a");
                push("aac");
            }
            TargetFormat::Opus => {
                push("-c:a");
                push("libopus");
                push("-application");
                push("voip");
                push("-vbr");
                push("on");
            }
            TargetFormat::Mp3 => {
                push("-c:a");
                push("libmp3lame");
            }
        }
        push("-b:a");
        a.push(format!("{kbps}k").into());
        let mut push = |s: &str| a.push(OsString::from(s));
        if req.mono {
            push("-ac");
            push("1");
        }
    } else {
        push("-c:a");
        push("copy");
    }
    let mut push = |s: &str| a.push(OsString::from(s));
    if use_cover {
        push("-c:v");
        push("mjpeg");
        push("-disposition:v:0");
        push("attached_pic");
    } else {
        push("-vn");
    }
    match req.target {
        TargetFormat::M4bAac => {
            push("-movflags");
            push("+faststart");
            push("-f");
            push("mp4");
        }
        TargetFormat::Opus => {
            push("-f");
            push("ogg");
        }
        TargetFormat::Mp3 => {
            push("-f");
            push("mp3");
            push("-id3v2_version");
            push("3");
        }
    }
    a.push(temp_output.clone().into_os_string());

    Ok(TranscodePlan {
        args: a,
        concat_list_path,
        concat_list,
        ffmetadata_path,
        ffmetadata,
        temp_output,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProgressEvent {
    /// Output position in milliseconds.
    Position(u64),
    End,
}

/// Parse one `-progress` key=value line.
pub fn parse_progress_line(line: &str) -> Option<ProgressEvent> {
    let (k, v) = line.trim().split_once('=')?;
    match k {
        // Despite the name, FFmpeg reports out_time_ms in microseconds.
        "out_time_us" | "out_time_ms" => v
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|x| *x >= 0)
            .map(|us| ProgressEvent::Position(us as u64 / 1000)),
        "progress" if v.trim() == "end" => Some(ProgressEvent::End),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranscodeOutcome {
    pub output: PathBuf,
    pub size: u64,
    pub duration_ms: u64,
}

/// Validate that `actual` is within max(2 %, 2 s) of `expected`.
pub fn duration_matches(expected_ms: u64, actual_ms: u64) -> bool {
    if expected_ms == 0 {
        return actual_ms > 0;
    }
    let tolerance = (expected_ms / 50).max(2000);
    expected_ms.abs_diff(actual_ms) <= tolerance
}

struct TempGuard<'a> {
    paths: Vec<&'a Path>,
    armed: bool,
}

impl Drop for TempGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            for p in &self.paths {
                let _ = std::fs::remove_file(p);
            }
        }
    }
}

/// Run a transcode. `progress` receives a 0..=1 fraction.
pub fn run_transcode(
    ffmpeg: &Path,
    req: &TranscodeRequest,
    work_dir: &Path,
    cancel: &dyn Cancel,
    progress: &mut dyn FnMut(f32),
) -> Result<TranscodeOutcome> {
    if req.output.exists() {
        return Err(MediaError::Failed(format!(
            "destination already exists: {}",
            req.output.display()
        )));
    }
    for input in &req.inputs {
        if !input.path.is_file() {
            return Err(MediaError::Failed(format!(
                "source file missing: {}",
                input.path.display()
            )));
        }
    }
    if let Some(parent) = req.output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::create_dir_all(work_dir)?;
    let plan = build_plan(req, work_dir)?;
    std::fs::write(&plan.concat_list_path, plan.concat_list.as_bytes())?;
    std::fs::write(&plan.ffmetadata_path, plan.ffmetadata.as_bytes())?;
    let _work_files = TempGuard {
        paths: vec![&plan.concat_list_path, &plan.ffmetadata_path],
        armed: true,
    };
    let mut out_guard = TempGuard {
        paths: vec![&plan.temp_output],
        armed: true,
    };

    let mut child = base_command(ffmpeg)
        .args(&plan.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| MediaError::ToolMissing(format!("{}: {e}", ffmpeg.display())))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| MediaError::Failed("no ffmpeg stdout".into()))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| MediaError::Failed("no ffmpeg stderr".into()))?;
    let (tx, rx) = mpsc::channel::<ProgressEvent>();
    let reader = std::thread::Builder::new()
        .name("ffmpeg-progress".into())
        .spawn(move || {
            for line in BufReader::new(stdout)
                .lines()
                .map_while(std::result::Result::ok)
            {
                if let Some(ev) = parse_progress_line(&line)
                    && tx.send(ev).is_err()
                {
                    break;
                }
            }
        })?;
    let err_reader = std::thread::Builder::new()
        .name("ffmpeg-stderr".into())
        .spawn(move || {
            // Keep only the tail of stderr so a chatty FFmpeg cannot grow memory.
            let mut tail: VecDeque<u8> = VecDeque::new();
            let mut buf = [0u8; 4096];
            while let Ok(n) = stderr.read(&mut buf) {
                if n == 0 {
                    break;
                }
                tail.extend(&buf[..n]);
                while tail.len() > 8192 {
                    tail.pop_front();
                }
            }
            String::from_utf8_lossy(&tail.into_iter().collect::<Vec<_>>()).into_owned()
        })?;

    let expected = req.expected_duration_ms().max(1);
    let status = loop {
        if cancel.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            let _ = err_reader.join();
            return Err(MediaError::Cancelled);
        }
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(ProgressEvent::Position(ms)) => {
                progress((ms as f32 / expected as f32).clamp(0.0, 0.99))
            }
            Ok(ProgressEvent::End) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if let Some(status) = child.try_wait()? {
                    break status;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
    };
    let _ = reader.join();
    let stderr_tail = err_reader.join().unwrap_or_default();
    if !status.success() {
        let msg = stderr_tail.trim();
        return Err(MediaError::Failed(format!(
            "ffmpeg exited with {status}: {}",
            if msg.is_empty() { "no output" } else { msg }
        )));
    }

    // Validate the output before it becomes visible.
    let probe = probe_file(&plan.temp_output, false)
        .map_err(|e| MediaError::Failed(format!("transcoded output is unreadable: {e}")))?;
    let actual = probe.duration_ms.unwrap_or(0);
    if !duration_matches(req.expected_duration_ms(), actual) {
        return Err(MediaError::Failed(format!(
            "output duration {actual} ms does not match source {} ms",
            req.expected_duration_ms()
        )));
    }
    let size = std::fs::metadata(&plan.temp_output)?.len();
    if size == 0 {
        return Err(MediaError::Failed("transcoded output is empty".into()));
    }
    if req.output.exists() {
        return Err(MediaError::Failed(format!(
            "destination appeared during transcode: {}",
            req.output.display()
        )));
    }
    std::fs::rename(&plan.temp_output, &req.output)?;
    out_guard.armed = false;
    progress(1.0);
    Ok(TranscodeOutcome {
        output: req.output.clone(),
        size,
        duration_ms: actual,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(n: usize, target: TargetFormat, reencode: bool) -> TranscodeRequest {
        TranscodeRequest {
            inputs: (0..n)
                .map(|i| TranscodeInput {
                    path: PathBuf::from(format!("/books/Глава {i:02}.mp3")),
                    duration_ms: 60_000,
                    chapter_title: None,
                })
                .collect(),
            output: PathBuf::from("/out/Книга.m4b"),
            target,
            bitrate_kbps: Some(64),
            reencode,
            mono: true,
            metadata: TranscodeMetadata {
                title: Some("Метро 2033".into()),
                author: Some("Дмитрий Глуховский".into()),
                narrator: Some("Чтец; = #1".into()),
                ..Default::default()
            },
            cover: Some(PathBuf::from("/books/cover.jpg")),
        }
    }

    fn args(plan: &TranscodePlan) -> Vec<String> {
        plan.args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn temp_path_is_hidden_sibling() {
        let p = temp_output_path(Path::new("/out/Книга.m4b"));
        assert_eq!(p, Path::new("/out/.Книга.avpart.m4b"));
    }

    #[test]
    fn escaping() {
        assert_eq!(
            escape_ffmetadata("a=b;c#d\\e\nf"),
            "a\\=b\\;c\\#d\\\\e\\\nf"
        );
        assert_eq!(
            concat_line(Path::new("/x/it's.mp3")),
            "file '/x/it'\\''s.mp3'\n"
        );
    }

    #[test]
    fn aac_plan_joins_with_chapters_and_cover() {
        let r = req(3, TargetFormat::M4bAac, true);
        let plan = build_plan(&r, Path::new("/work")).unwrap();
        let a = args(&plan);
        assert!(a.windows(2).any(|w| w == ["-c:a", "aac"]));
        assert!(a.windows(2).any(|w| w == ["-b:a", "64k"]));
        assert!(a.windows(2).any(|w| w == ["-ac", "1"]));
        assert!(a.windows(2).any(|w| w == ["-map_chapters", "1"]));
        assert!(
            a.windows(2)
                .any(|w| w == ["-disposition:v:0", "attached_pic"])
        );
        assert!(a.last().unwrap().ends_with(".Книга.avpart.m4b"));
        assert_eq!(plan.concat_list.lines().count(), 3);
        assert_eq!(plan.ffmetadata.matches("[CHAPTER]").count(), 3);
        assert!(plan.ffmetadata.contains("START=60000\nEND=120000"));
        assert!(plan.ffmetadata.contains("composer=Чтец\\; \\= \\#1"));
        assert!(plan.ffmetadata.contains("title=Глава 00"));
    }

    #[test]
    fn single_input_keeps_own_chapters() {
        let plan = build_plan(&req(1, TargetFormat::M4bAac, true), Path::new("/w")).unwrap();
        let a = args(&plan);
        assert!(a.windows(2).any(|w| w == ["-map_chapters", "0"]));
        assert!(!plan.ffmetadata.contains("[CHAPTER]"));
    }

    #[test]
    fn opus_plan_has_no_cover_and_repack_copies() {
        let plan = build_plan(&req(2, TargetFormat::Opus, true), Path::new("/w")).unwrap();
        let a = args(&plan);
        assert!(a.windows(2).any(|w| w == ["-c:a", "libopus"]));
        assert!(a.contains(&"-vn".to_string()));
        assert!(a.windows(2).any(|w| w == ["-f", "ogg"]));

        let mut r = req(2, TargetFormat::M4bAac, false);
        r.bitrate_kbps = None;
        let a = args(&build_plan(&r, Path::new("/w")).unwrap());
        assert!(a.windows(2).any(|w| w == ["-c:a", "copy"]));
        assert!(!a.contains(&"-b:a".to_string()));
        assert!(build_plan(&req(0, TargetFormat::M4bAac, true), Path::new("/w")).is_err());
    }

    #[test]
    fn progress_parsing() {
        assert_eq!(
            parse_progress_line("out_time_us=1500000"),
            Some(ProgressEvent::Position(1500))
        );
        assert_eq!(
            parse_progress_line("out_time_ms=2000000"),
            Some(ProgressEvent::Position(2000))
        );
        assert_eq!(parse_progress_line("out_time_us=-922337"), None);
        assert_eq!(
            parse_progress_line("progress=end"),
            Some(ProgressEvent::End)
        );
        assert_eq!(parse_progress_line("progress=continue"), None);
        assert_eq!(parse_progress_line("bitrate=64k"), None);
    }

    #[test]
    fn duration_tolerance() {
        assert!(duration_matches(3_600_000, 3_620_000));
        assert!(!duration_matches(3_600_000, 3_000_000));
        assert!(duration_matches(10_000, 11_500));
        assert!(!duration_matches(10_000, 13_000));
    }

    /// Real end-to-end run when FFmpeg is available on this machine.
    #[test]
    fn real_ffmpeg_joins_wavs_into_m4b() {
        let Some(ffmpeg) = locate_ffmpeg(None, &[]) else {
            eprintln!("ffmpeg not found; skipping");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("01 начало.wav");
        let b = dir.path().join("02 it's end.wav");
        crate::probe::tests::write_wav(&a, 2.0, 440.0);
        crate::probe::tests::write_wav(&b, 3.0, 660.0);
        let r = TranscodeRequest {
            inputs: vec![
                TranscodeInput {
                    path: a.clone(),
                    duration_ms: 2000,
                    chapter_title: None,
                },
                TranscodeInput {
                    path: b.clone(),
                    duration_ms: 3000,
                    chapter_title: None,
                },
            ],
            output: dir.path().join("out").join("Книга.m4b"),
            target: TargetFormat::M4bAac,
            bitrate_kbps: Some(48),
            reencode: true,
            mono: true,
            metadata: TranscodeMetadata {
                title: Some("Книга".into()),
                ..Default::default()
            },
            cover: None,
        };
        let mut last = 0.0;
        let out = run_transcode(
            &ffmpeg,
            &r,
            &dir.path().join("work"),
            &crate::NeverCancel,
            &mut |p| last = p,
        )
        .unwrap();
        assert!(out.output.is_file());
        assert!(a.is_file() && b.is_file(), "sources must be untouched");
        assert!(!temp_output_path(&r.output).exists());
        assert_eq!(last, 1.0);
        let probe = probe_file(&out.output, false).unwrap();
        assert_eq!(probe.chapter_count, 2);
        assert_eq!(probe.tags.title.as_deref(), Some("Книга"));
        // Refuses to overwrite.
        assert!(
            run_transcode(
                &ffmpeg,
                &r,
                &dir.path().join("work"),
                &crate::NeverCancel,
                &mut |_| {}
            )
            .is_err()
        );
    }
}
