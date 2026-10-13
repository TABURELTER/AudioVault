//! Domain entities and enumerations shared by every layer.
//!
//! Hierarchy: `Book` (logical work) → `AudioVariant` (a narration/rendition)
//! → `BookCopy` (a physical folder/file set on a `StorageVolume`) → `MediaFile`.

use serde::{Deserialize, Serialize};

pub type VolumeId = i64;
pub type BookId = i64;
pub type VariantId = i64;
pub type CopyId = i64;
pub type PersonId = i64;
pub type JobId = i64;
pub type MediaFileId = i64;

/// Declares a string-backed enum with stable DB representations.
macro_rules! str_enum {
    ($(#[$m:meta])* $name:ident { $($variant:ident => $s:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];
            pub fn as_str(self) -> &'static str {
                match self { $($name::$variant => $s),+ }
            }
            pub fn parse(s: &str) -> Option<Self> {
                match s { $($s => Some($name::$variant),)+ _ => None }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

str_enum!(
    /// Physical storage class; drives I/O scheduling.
    StorageClass {
        Hdd => "hdd",
        Ssd => "ssd",
        Removable => "removable",
        Network => "network",
        Unknown => "unknown",
    }
);

impl Default for StorageClass {
    fn default() -> Self {
        StorageClass::Unknown
    }
}

impl StorageClass {
    /// Rotational/unknown media get conservative sequential I/O.
    pub fn is_rotational_or_unknown(self) -> bool {
        matches!(
            self,
            StorageClass::Hdd
                | StorageClass::Removable
                | StorageClass::Unknown
                | StorageClass::Network
        )
    }
    pub fn label(self) -> &'static str {
        match self {
            StorageClass::Hdd => "HDD",
            StorageClass::Ssd => "SSD",
            StorageClass::Removable => "Removable",
            StorageClass::Network => "Network",
            StorageClass::Unknown => "Unknown",
        }
    }
}

str_enum!(
    CopyState {
        Present => "present",
        Missing => "missing",
        Removed => "removed",
    }
);

str_enum!(
    FileKind {
        Audio => "audio",
        Cue => "cue",
        Image => "image",
        Ebook => "ebook",
        Text => "text",
        Other => "other",
    }
);

impl FileKind {
    pub fn from_extension(ext: &str) -> FileKind {
        match ext.to_ascii_lowercase().as_str() {
            "mp3" | "m4a" | "m4b" | "aac" | "flac" | "ogg" | "oga" | "opus" | "wav" | "wma"
            | "ape" | "mp2" | "aiff" | "aif" | "wv" | "mka" | "alac" => FileKind::Audio,
            "cue" => FileKind::Cue,
            "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" => FileKind::Image,
            "fb2" | "epub" | "pdf" | "djvu" | "mobi" | "azw3" | "fb2.zip" => FileKind::Ebook,
            "txt" | "nfo" | "rtf" | "doc" | "docx" | "md" | "log" => FileKind::Text,
            _ => FileKind::Other,
        }
    }
}

str_enum!(
    MetadataField {
        Title => "title",
        Author => "author",
        Narrator => "narrator",
        Series => "series",
        SeriesIndex => "series_index",
        Year => "year",
        Language => "language",
        Genre => "genre",
        Description => "description",
        Publisher => "publisher",
        Cover => "cover",
    }
);

str_enum!(
    /// Origin of a metadata observation. Authority is defined in [`MetadataSource::authority`].
    MetadataSource {
        Manual => "manual",
        Fb2 => "fb2",
        EmbeddedTag => "tag",
        ExternalProvider => "provider",
        TorrentTitle => "torrent",
        FolderName => "folder",
        FileName => "filename",
    }
);

impl MetadataSource {
    /// Base authority; multiplied by the observation's own confidence.
    pub fn authority(self) -> f32 {
        match self {
            MetadataSource::Manual => 100.0,
            MetadataSource::Fb2 => 80.0,
            MetadataSource::EmbeddedTag => 70.0,
            MetadataSource::ExternalProvider => 60.0,
            MetadataSource::TorrentTitle => 50.0,
            MetadataSource::FolderName => 45.0,
            MetadataSource::FileName => 30.0,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            MetadataSource::Manual => "Manual edit",
            MetadataSource::Fb2 => "FB2",
            MetadataSource::EmbeddedTag => "Audio tags",
            MetadataSource::ExternalProvider => "Provider",
            MetadataSource::TorrentTitle => "Torrent title",
            MetadataSource::FolderName => "Folder name",
            MetadataSource::FileName => "File name",
        }
    }
}

str_enum!(
    DuplicateStatus {
        Open => "open",
        KeptA => "kept_a",
        KeptB => "kept_b",
        KeptBoth => "kept_both",
        DifferentVariants => "different_variants",
        Ignored => "ignored",
    }
);

str_enum!(
    DuplicateVerdict {
        ExactCopy => "exact",
        SameRecording => "same_recording",
        PossibleDuplicate => "possible",
        DifferentNarrator => "different_narrator",
        Unrelated => "unrelated",
    }
);

impl DuplicateVerdict {
    pub fn label(self) -> &'static str {
        match self {
            DuplicateVerdict::ExactCopy => "Byte-identical copy",
            DuplicateVerdict::SameRecording => "Same recording",
            DuplicateVerdict::PossibleDuplicate => "Possible duplicate",
            DuplicateVerdict::DifferentNarrator => "Same book, different narrator",
            DuplicateVerdict::Unrelated => "Unrelated",
        }
    }
}

str_enum!(
    JobKind {
        Scan => "scan",
        Metadata => "metadata",
        QuickHash => "quick_hash",
        FullHash => "full_hash",
        Fingerprint => "fingerprint",
        Duplicates => "duplicates",
        Thumbnails => "thumbnails",
        FileOperation => "file_operation",
        Transcode => "transcode",
        TorrentImport => "torrent_import",
        Reconcile => "reconcile",
        Optimize => "optimize_analysis",
    }
);

impl JobKind {
    /// Heavy jobs saturate a disk (sequential full reads/writes).
    pub fn is_heavy_io(self) -> bool {
        matches!(
            self,
            JobKind::FullHash | JobKind::Fingerprint | JobKind::Transcode | JobKind::FileOperation
        )
    }
    /// Whether an interrupted job may be re-queued automatically on startup.
    pub fn restartable(self) -> bool {
        !matches!(self, JobKind::FileOperation | JobKind::Transcode)
    }
    pub fn label(self) -> &'static str {
        match self {
            JobKind::Scan => "Scanning",
            JobKind::Metadata => "Reading metadata",
            JobKind::QuickHash => "Quick hashing",
            JobKind::FullHash => "Hashing",
            JobKind::Fingerprint => "Fingerprinting",
            JobKind::Duplicates => "Duplicate detection",
            JobKind::Thumbnails => "Cover thumbnails",
            JobKind::FileOperation => "File operation",
            JobKind::Transcode => "Transcoding",
            JobKind::TorrentImport => "Importing download",
            JobKind::Reconcile => "Reconciling library",
            JobKind::Optimize => "Storage analysis",
        }
    }
}

str_enum!(
    JobState {
        Queued => "queued",
        Running => "running",
        Paused => "paused",
        Completed => "completed",
        Failed => "failed",
        Cancelled => "cancelled",
        Interrupted => "interrupted",
    }
);

impl JobState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            JobState::Completed | JobState::Failed | JobState::Cancelled
        )
    }
}

str_enum!(
    FileOpKind {
        Rename => "rename",
        Move => "move",
        Copy => "copy",
        MetadataWrite => "metadata_write",
        Trash => "trash",
        DeletePermanent => "delete_permanent",
        TranscodeReplace => "transcode_replace",
    }
);

impl FileOpKind {
    pub fn is_destructive(self) -> bool {
        matches!(
            self,
            FileOpKind::Trash | FileOpKind::DeletePermanent | FileOpKind::TranscodeReplace
        )
    }
}

str_enum!(
    /// File operation lifecycle. `Planned → InProgress → Completed | Failed`;
    /// `InProgress` found on startup becomes `NeedsVerification`.
    FileOpState {
        Planned => "planned",
        InProgress => "in_progress",
        Completed => "completed",
        Failed => "failed",
        RolledBack => "rolled_back",
        NeedsVerification => "needs_verification",
        Undone => "undone",
    }
);

impl FileOpState {
    /// Allowed state transitions; anything else is a logic error.
    pub fn can_transition_to(self, next: FileOpState) -> bool {
        use FileOpState::*;
        matches!(
            (self, next),
            (Planned, InProgress)
                | (Planned, Failed)
                | (InProgress, Completed)
                | (InProgress, Failed)
                | (InProgress, RolledBack)
                | (InProgress, NeedsVerification)
                | (NeedsVerification, Completed)
                | (NeedsVerification, Failed)
                | (NeedsVerification, RolledBack)
                | (Completed, Undone)
        )
    }
}

str_enum!(
    TorrentState {
        Queued => "queued",
        Initializing => "initializing",
        Downloading => "downloading",
        Paused => "paused",
        Seeding => "seeding",
        Completed => "completed",
        Error => "error",
        Removed => "removed",
    }
);

str_enum!(
    IssueKind {
        UnknownAuthor => "unknown_author",
        MissingTitle => "missing_title",
        TransliteratedFolder => "transliterated_folder",
        ReleaseGarbage => "release_garbage",
        TracksDisagree => "tracks_disagree",
        Fb2TagConflict => "fb2_tag_conflict",
        MissingNarrator => "missing_narrator",
        SeriesNumbering => "series_numbering",
        ProbeErrors => "probe_errors",
    }
);

impl IssueKind {
    pub fn label(self) -> &'static str {
        match self {
            IssueKind::UnknownAuthor => "Unknown author",
            IssueKind::MissingTitle => "Missing title",
            IssueKind::TransliteratedFolder => "Folder uses transliteration",
            IssueKind::ReleaseGarbage => "Title contains release garbage",
            IssueKind::TracksDisagree => "Tracks disagree on album",
            IssueKind::Fb2TagConflict => "FB2 and audio tags conflict",
            IssueKind::MissingNarrator => "Narrator missing",
            IssueKind::SeriesNumbering => "Series numbering inconsistent",
            IssueKind::ProbeErrors => "Unreadable/corrupt files",
        }
    }
}

/// Aggregated technical summary of a physical copy.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CopyTech {
    pub audio_file_count: u32,
    pub total_size: u64,
    pub audio_size: u64,
    pub total_duration_ms: u64,
    pub primary_codec: Option<String>,
    pub container: Option<String>,
    pub avg_bitrate_kbps: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub is_lossless: bool,
    pub has_cover: bool,
    pub has_fb2: bool,
    pub has_chapters: bool,
    pub probe_errors: u32,
}

/// Lossless codecs (case-insensitive).
pub fn is_lossless_codec(codec: &str) -> bool {
    matches!(
        codec.to_ascii_lowercase().as_str(),
        "flac" | "alac" | "wav" | "pcm" | "ape" | "wavpack" | "wv" | "aiff" | "tta"
    ) || codec.to_ascii_lowercase().starts_with("pcm")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enums_roundtrip() {
        for k in JobKind::ALL {
            assert_eq!(JobKind::parse(k.as_str()), Some(*k));
        }
        for s in FileOpState::ALL {
            assert_eq!(FileOpState::parse(s.as_str()), Some(*s));
        }
        assert_eq!(FileKind::from_extension("M4B"), FileKind::Audio);
        assert_eq!(FileKind::from_extension("fb2"), FileKind::Ebook);
        assert_eq!(FileKind::from_extension("jpg"), FileKind::Image);
    }

    #[test]
    fn file_op_transitions() {
        use FileOpState::*;
        assert!(Planned.can_transition_to(InProgress));
        assert!(InProgress.can_transition_to(Completed));
        assert!(InProgress.can_transition_to(NeedsVerification));
        assert!(!Planned.can_transition_to(Completed));
        assert!(!Completed.can_transition_to(InProgress));
        assert!(!Failed.can_transition_to(Completed));
    }
}
