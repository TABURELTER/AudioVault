//! Prevent system sleep while long operations (downloads, transcodes, full
//! scans) run. Reference counted: sleep is allowed again when the last guard drops.

use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct SleepInhibitor {
    inner: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    count: usize,
    backend: Option<Backend>,
}

pub struct SleepGuard {
    inner: Arc<Mutex<State>>,
}

impl SleepInhibitor {
    pub fn acquire(&self, reason: &str) -> SleepGuard {
        if let Ok(mut st) = self.inner.lock() {
            st.count += 1;
            if st.backend.is_none() {
                st.backend = Backend::start(reason);
            }
        }
        SleepGuard { inner: self.inner.clone() }
    }

    pub fn active(&self) -> bool {
        self.inner.lock().map(|s| s.count > 0).unwrap_or(false)
    }
}

impl Drop for SleepGuard {
    fn drop(&mut self) {
        if let Ok(mut st) = self.inner.lock() {
            st.count = st.count.saturating_sub(1);
            if st.count == 0
                && let Some(b) = st.backend.take()
            {
                b.stop();
            }
        }
    }
}

#[cfg(windows)]
struct Backend {
    stop: std::sync::mpsc::Sender<()>,
}

#[cfg(windows)]
impl Backend {
    fn start(_reason: &str) -> Option<Backend> {
        use windows_sys::Win32::System::Power::{ES_CONTINUOUS, ES_SYSTEM_REQUIRED, SetThreadExecutionState};
        // SetThreadExecutionState is per-thread, so a dedicated thread holds it.
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        std::thread::Builder::new()
            .name("sleep-inhibitor".into())
            .spawn(move || {
                // SAFETY: plain Win32 call with constant flags.
                unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) };
                let _ = rx.recv();
                // SAFETY: as above; restores normal power management.
                unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
            })
            .ok()?;
        Some(Backend { stop: tx })
    }
    fn stop(self) {
        let _ = self.stop.send(());
    }
}

#[cfg(target_os = "macos")]
struct Backend {
    child: std::process::Child,
}

#[cfg(target_os = "macos")]
impl Backend {
    fn start(_reason: &str) -> Option<Backend> {
        // `caffeinate -i -w <pid>` holds an idle-sleep assertion and exits with us.
        let pid = std::process::id().to_string();
        std::process::Command::new("/usr/bin/caffeinate")
            .args(["-i", "-w", &pid])
            .spawn()
            .ok()
            .map(|child| Backend { child })
    }
    fn stop(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
struct Backend;

#[cfg(not(any(windows, target_os = "macos")))]
impl Backend {
    fn start(_reason: &str) -> Option<Backend> {
        None
    }
    fn stop(self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refcounted() {
        let s = SleepInhibitor::default();
        let a = s.acquire("a");
        let b = s.acquire("b");
        drop(a);
        assert!(s.active());
        drop(b);
        assert!(!s.active());
    }
}
