//! Change notifications from the service layer to the UI.
//!
//! The UI registers one sink; events are coalescing hints ("something in X
//! changed, re-query"), never carriers of large data.

use std::sync::Arc;

use parking_lot::RwLock;

#[derive(Debug, Clone, PartialEq)]
pub enum AppEvent {
    JobsChanged,
    LibraryChanged,
    VolumesChanged,
    TorrentsChanged,
    DuplicatesChanged,
    IssuesChanged,
    /// A message worth showing to the user (toast / status bar).
    Notice { level: NoticeLevel, text: String },
    /// A second instance asked us to come to the foreground.
    Activate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeLevel {
    Info,
    Warning,
    Error,
}

type Sink = Arc<dyn Fn(AppEvent) + Send + Sync>;

#[derive(Clone, Default)]
pub struct EventBus {
    sink: Arc<RwLock<Option<Sink>>>,
}

impl EventBus {
    pub fn set_sink(&self, f: impl Fn(AppEvent) + Send + Sync + 'static) {
        *self.sink.write() = Some(Arc::new(f));
    }

    pub fn emit(&self, e: AppEvent) {
        let sink = self.sink.read().clone();
        if let Some(s) = sink {
            s(e);
        }
    }

    pub fn info(&self, text: impl Into<String>) {
        self.emit(AppEvent::Notice { level: NoticeLevel::Info, text: text.into() });
    }

    pub fn warn(&self, text: impl Into<String>) {
        self.emit(AppEvent::Notice { level: NoticeLevel::Warning, text: text.into() });
    }

    pub fn error(&self, text: impl Into<String>) {
        self.emit(AppEvent::Notice { level: NoticeLevel::Error, text: text.into() });
    }
}
