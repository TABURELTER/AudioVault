//! Storage-aware I/O scheduler.
//!
//! Each storage volume gets its own concurrency budget, split into *heavy*
//! (sequential full reads/writes: hashing, fingerprinting, transcoding, file
//! copies) and *light* (directory walks, tag probes) slots:
//!
//! | storage class                  | heavy | light |
//! |--------------------------------|-------|-------|
//! | HDD / removable / network / ?  |   1   |   1   |
//! | SSD                            |   3   |   4   |
//!
//! On rotational media a heavy job and a light job never run together
//! (seek thrashing makes both slower than running them back to back).
//! Heavy jobs are deferred on volumes that are receiving a torrent download.
//! Jobs without a volume (e.g. catalog-wide duplicate detection) only use the
//! global budget.

use std::collections::HashMap;
use std::sync::Arc;

use audiovault_core::model::StorageClass;
use parking_lot::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub heavy: usize,
    pub light: usize,
    /// Heavy and light work may overlap on this volume.
    pub mixed: bool,
}

pub fn budget_for(class: StorageClass) -> Budget {
    match class {
        StorageClass::Ssd => Budget { heavy: 3, light: 4, mixed: true },
        _ => Budget { heavy: 1, light: 1, mixed: false },
    }
}

#[derive(Debug, Default)]
struct VolumeUse {
    heavy: usize,
    light: usize,
}

#[derive(Debug, Default)]
struct State {
    total: usize,
    per_volume: HashMap<i64, VolumeUse>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denied {
    GlobalLimit,
    VolumeBusy,
    VolumeDownloading,
}

#[derive(Clone)]
pub struct IoScheduler {
    state: Arc<Mutex<State>>,
    max_total: Arc<Mutex<usize>>,
}

/// Held while a job runs; releases its slot on drop.
pub struct Permit {
    state: Arc<Mutex<State>>,
    volume: Option<i64>,
    heavy: bool,
}

impl Drop for Permit {
    fn drop(&mut self) {
        let mut s = self.state.lock();
        s.total = s.total.saturating_sub(1);
        if let Some(v) = self.volume
            && let Some(u) = s.per_volume.get_mut(&v)
        {
            if self.heavy {
                u.heavy = u.heavy.saturating_sub(1);
            } else {
                u.light = u.light.saturating_sub(1);
            }
            if u.heavy == 0 && u.light == 0 {
                s.per_volume.remove(&v);
            }
        }
    }
}

impl IoScheduler {
    pub fn new(max_total: usize) -> Self {
        IoScheduler { state: Arc::default(), max_total: Arc::new(Mutex::new(max_total.max(1))) }
    }

    pub fn set_max_total(&self, n: usize) {
        *self.max_total.lock() = n.max(1);
    }

    pub fn running(&self) -> usize {
        self.state.lock().total
    }

    /// Try to obtain a slot. `downloading` = the volume is receiving a torrent.
    pub fn try_acquire(
        &self,
        volume: Option<(i64, StorageClass)>,
        heavy: bool,
        downloading: bool,
    ) -> std::result::Result<Permit, Denied> {
        let max_total = *self.max_total.lock();
        let mut s = self.state.lock();
        if s.total >= max_total {
            return Err(Denied::GlobalLimit);
        }
        if let Some((vid, class)) = volume {
            if heavy && downloading {
                return Err(Denied::VolumeDownloading);
            }
            let b = budget_for(class);
            let u = s.per_volume.entry(vid).or_default();
            let ok = if heavy {
                u.heavy < b.heavy && (b.mixed || u.light == 0)
            } else {
                u.light < b.light && (b.mixed || u.heavy == 0)
            };
            if !ok {
                return Err(Denied::VolumeBusy);
            }
            if heavy {
                u.heavy += 1;
            } else {
                u.light += 1;
            }
        }
        s.total += 1;
        Ok(Permit { state: self.state.clone(), volume: volume.map(|v| v.0), heavy })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hdd_serializes_everything() {
        let s = IoScheduler::new(8);
        let hdd = Some((1, StorageClass::Hdd));
        let a = s.try_acquire(hdd, true, false).unwrap();
        assert_eq!(s.try_acquire(hdd, true, false).err(), Some(Denied::VolumeBusy));
        assert_eq!(s.try_acquire(hdd, false, false).err(), Some(Denied::VolumeBusy), "no light work during heavy on HDD");
        // Another disk is independent.
        let _b = s.try_acquire(Some((2, StorageClass::Hdd)), true, false).unwrap();
        drop(a);
        let _c = s.try_acquire(hdd, false, false).unwrap();
        assert_eq!(s.running(), 2);
    }

    #[test]
    fn ssd_allows_parallelism() {
        let s = IoScheduler::new(16);
        let ssd = Some((7, StorageClass::Ssd));
        let permits: Vec<_> = (0..3).map(|_| s.try_acquire(ssd, true, false).unwrap()).collect();
        assert_eq!(s.try_acquire(ssd, true, false).err(), Some(Denied::VolumeBusy));
        let lights: Vec<_> = (0..4).map(|_| s.try_acquire(ssd, false, false).unwrap()).collect();
        assert_eq!(s.try_acquire(ssd, false, false).err(), Some(Denied::VolumeBusy));
        assert_eq!(permits.len() + lights.len(), 7);
    }

    #[test]
    fn global_limit_and_downloads() {
        let s = IoScheduler::new(1);
        let _a = s.try_acquire(None, false, false).unwrap();
        assert_eq!(s.try_acquire(Some((1, StorageClass::Ssd)), false, false).err(), Some(Denied::GlobalLimit));
        let s = IoScheduler::new(4);
        assert_eq!(s.try_acquire(Some((1, StorageClass::Ssd)), true, true).err(), Some(Denied::VolumeDownloading));
        assert!(s.try_acquire(Some((1, StorageClass::Ssd)), false, true).is_ok(), "light work allowed while downloading");
    }
}
