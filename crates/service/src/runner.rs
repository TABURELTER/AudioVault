//! Persistent background job runner.
//!
//! Jobs are rows in `background_job`. A single dispatcher thread claims queued
//! jobs whenever the [`IoScheduler`] grants a slot and runs each on its own
//! named worker thread (the number of workers is bounded by the scheduler's
//! global budget). Cancellation and pause are cooperative: workers poll
//! [`JobContext::is_cancelled`], which consults an in-memory flag and the
//! database (so the UI can cancel by writing to the DB from any thread).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use audiovault_core::model::{JobState, StorageClass};
use audiovault_db::jobs::{self, JobRecord, NewJob};
use audiovault_db::{Db, torrents, volumes};
use audiovault_indexer::{IndexerError, JobContext};
use audiovault_platform::power::SleepInhibitor;
use parking_lot::{Condvar, Mutex};

use crate::events::{AppEvent, EventBus};
use crate::exec::Executor;
use crate::scheduler::IoScheduler;
use crate::{Result, ServiceError};

const DISPATCH_INTERVAL: Duration = Duration::from_millis(1000);
const PROGRESS_WRITE_INTERVAL: Duration = Duration::from_millis(400);
const CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(750);

struct Inner {
    db: Arc<Db>,
    sched: IoScheduler,
    exec: Arc<Executor>,
    events: EventBus,
    sleep: SleepInhibitor,
    inhibit_sleep: AtomicBool,
    wake_flag: Mutex<bool>,
    wake_cv: Condvar,
    stop: AtomicBool,
    running: Mutex<HashMap<i64, Arc<AtomicBool>>>,
    workers: Mutex<Vec<JoinHandle<()>>>,
}

#[derive(Clone)]
pub struct Runner {
    inner: Arc<Inner>,
}

pub struct JobCtx {
    db: Arc<Db>,
    job_id: i64,
    flag: Arc<AtomicBool>,
    last_poll: Mutex<Instant>,
    last_progress: Mutex<Instant>,
    events: EventBus,
}

impl JobContext for JobCtx {
    fn is_cancelled(&self) -> bool {
        if self.flag.load(Ordering::Relaxed) {
            return true;
        }
        let mut last = self.last_poll.lock();
        if last.elapsed() >= CANCEL_POLL_INTERVAL {
            *last = Instant::now();
            let requested = self
                .db
                .conn()
                .ok()
                .and_then(|c| jobs::is_cancel_requested(&c, self.job_id).ok())
                .unwrap_or(false);
            if requested {
                self.flag.store(true, Ordering::Relaxed);
                return true;
            }
        }
        false
    }

    fn progress(&self, fraction: f64, text: &str) {
        let mut last = self.last_progress.lock();
        let done = fraction >= 1.0;
        if !done && last.elapsed() < PROGRESS_WRITE_INTERVAL {
            return;
        }
        *last = Instant::now();
        if let Ok(c) = self.db.conn() {
            let _ = jobs::set_progress(&c, self.job_id, fraction.max(0.0), text);
        }
        self.events.emit(AppEvent::JobsChanged);
    }
}

impl JobCtx {
    /// Context for running executor code outside the runner (tests, tools).
    pub fn detached(db: Arc<Db>, job_id: i64) -> JobCtx {
        JobCtx {
            db,
            job_id,
            flag: Arc::new(AtomicBool::new(false)),
            last_poll: Mutex::new(Instant::now()),
            last_progress: Mutex::new(Instant::now() - PROGRESS_WRITE_INTERVAL),
            events: EventBus::default(),
        }
    }
}

impl Runner {
    pub fn new(db: Arc<Db>, exec: Arc<Executor>, events: EventBus, max_parallel: usize) -> Runner {
        Runner {
            inner: Arc::new(Inner {
                db,
                sched: IoScheduler::new(max_parallel),
                exec,
                events,
                sleep: SleepInhibitor::default(),
                inhibit_sleep: AtomicBool::new(true),
                wake_flag: Mutex::new(false),
                wake_cv: Condvar::new(),
                stop: AtomicBool::new(false),
                running: Mutex::new(HashMap::new()),
                workers: Mutex::new(Vec::new()),
            }),
        }
    }

    pub fn set_max_parallel(&self, n: usize) {
        self.inner.sched.set_max_total(n);
        self.wake();
    }

    pub fn set_inhibit_sleep(&self, on: bool) {
        self.inner.inhibit_sleep.store(on, Ordering::Relaxed);
    }

    /// Start the dispatcher thread.
    pub fn start(&self) -> Result<()> {
        let inner = self.inner.clone();
        let h = std::thread::Builder::new()
            .name("job-dispatcher".into())
            .spawn(move || dispatcher_loop(inner))?;
        self.inner.workers.lock().push(h);
        Ok(())
    }

    pub fn wake(&self) {
        *self.inner.wake_flag.lock() = true;
        self.inner.wake_cv.notify_all();
    }

    pub fn enqueue(&self, job: &NewJob) -> Result<i64> {
        let id = {
            let conn = self.inner.db.conn()?;
            jobs::enqueue(&conn, job)?
        };
        self.inner.events.emit(AppEvent::JobsChanged);
        self.wake();
        Ok(id)
    }

    pub fn cancel(&self, id: i64) -> Result<()> {
        {
            let conn = self.inner.db.conn()?;
            jobs::request_cancel(&conn, id)?;
        }
        if let Some(f) = self.inner.running.lock().get(&id) {
            f.store(true, Ordering::Relaxed);
        }
        self.inner.events.emit(AppEvent::JobsChanged);
        Ok(())
    }

    pub fn pause(&self, id: i64) -> Result<()> {
        {
            let conn = self.inner.db.conn()?;
            jobs::pause(&conn, id)?;
        }
        if let Some(f) = self.inner.running.lock().get(&id) {
            f.store(true, Ordering::Relaxed);
        }
        self.inner.events.emit(AppEvent::JobsChanged);
        Ok(())
    }

    pub fn resume(&self, id: i64) -> Result<()> {
        {
            let conn = self.inner.db.conn()?;
            jobs::resume(&conn, id)?;
        }
        self.inner.events.emit(AppEvent::JobsChanged);
        self.wake();
        Ok(())
    }

    pub fn retry(&self, id: i64) -> Result<()> {
        {
            let conn = self.inner.db.conn()?;
            jobs::retry(&conn, id)?;
        }
        self.inner.events.emit(AppEvent::JobsChanged);
        self.wake();
        Ok(())
    }

    pub fn running_count(&self) -> usize {
        self.inner.running.lock().len()
    }

    /// Stop dispatching, signal running jobs and wait (bounded) for workers.
    /// Interrupted restartable jobs resume on next start.
    pub fn shutdown(&self, wait: Duration) {
        self.inner.stop.store(true, Ordering::SeqCst);
        for f in self.inner.running.lock().values() {
            f.store(true, Ordering::Relaxed);
        }
        self.wake();
        let deadline = Instant::now() + wait;
        let handles: Vec<JoinHandle<()>> = std::mem::take(&mut *self.inner.workers.lock());
        for h in handles {
            while !h.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(25));
            }
            if h.is_finished() {
                let _ = h.join();
            }
        }
    }
}

fn dispatcher_loop(inner: Arc<Inner>) {
    tracing::info!("job dispatcher started");
    while !inner.stop.load(Ordering::SeqCst) {
        if let Err(e) = dispatch_once(&inner) {
            tracing::warn!(error = %e, "job dispatch failed");
        }
        // Reap finished worker handles.
        inner
            .workers
            .lock()
            .retain(|h| !h.is_finished() || h.thread().name() == Some("job-dispatcher"));
        let mut flag = inner.wake_flag.lock();
        if !*flag {
            inner.wake_cv.wait_for(&mut flag, DISPATCH_INTERVAL);
        }
        *flag = false;
    }
    tracing::info!("job dispatcher stopped");
}

fn dispatch_once(inner: &Arc<Inner>) -> Result<()> {
    let (queued, vols, downloading) = {
        let conn = inner.db.conn()?;
        let q = jobs::queued(&conn, 64)?;
        if q.is_empty() {
            return Ok(());
        }
        let v: HashMap<i64, (StorageClass, bool)> = volumes::all(&conn)?
            .into_iter()
            .map(|v| (v.id, (v.storage_class, v.is_online)))
            .collect();
        let d: HashSet<i64> = torrents::volumes_downloading(&conn)?.into_iter().collect();
        (q, v, d)
    };
    for job in queued {
        if inner.stop.load(Ordering::SeqCst) {
            break;
        }
        if inner.running.lock().contains_key(&job.id) {
            continue;
        }
        let vol = match job.volume_id {
            Some(vid) => match vols.get(&vid) {
                Some((class, true)) => Some((vid, *class)),
                // Storage offline: the job waits in the queue until it returns.
                Some((_, false)) => continue,
                None => {
                    let conn = inner.db.conn()?;
                    jobs::finish(
                        &conn,
                        job.id,
                        JobState::Failed,
                        Some("storage was removed from the catalog"),
                    )?;
                    continue;
                }
            },
            None => None,
        };
        let heavy = job.kind.is_heavy_io();
        let is_downloading = vol.is_some_and(|(v, _)| downloading.contains(&v));
        let permit = match inner.sched.try_acquire(vol, heavy, is_downloading) {
            Ok(p) => p,
            Err(crate::scheduler::Denied::GlobalLimit) => break,
            Err(_) => continue,
        };
        let claimed = {
            let conn = inner.db.conn()?;
            jobs::mark_running(&conn, job.id)?
        };
        if !claimed {
            continue;
        }
        let flag = Arc::new(AtomicBool::new(false));
        inner.running.lock().insert(job.id, flag.clone());
        inner.events.emit(AppEvent::JobsChanged);
        let inner2 = inner.clone();
        let spawn = std::thread::Builder::new()
            .name(format!("job-{}-{}", job.kind.as_str(), job.id))
            .spawn(move || {
                let guard = if heavy && inner2.inhibit_sleep.load(Ordering::Relaxed) {
                    Some(inner2.sleep.acquire(job.kind.label()))
                } else {
                    None
                };
                run_job(&inner2, job, flag);
                drop(guard);
                drop(permit);
                *inner2.wake_flag.lock() = true;
                inner2.wake_cv.notify_all();
            });
        match spawn {
            Ok(h) => inner.workers.lock().push(h),
            Err(e) => {
                tracing::error!(error = %e, "cannot spawn worker thread");
                break;
            }
        }
    }
    Ok(())
}

fn is_retryable(e: &ServiceError) -> bool {
    matches!(
        e,
        ServiceError::Io(_) | ServiceError::Indexer(IndexerError::Io(_))
    )
}

fn run_job(inner: &Arc<Inner>, job: JobRecord, flag: Arc<AtomicBool>) {
    let id = job.id;
    let ctx = JobCtx {
        db: inner.db.clone(),
        job_id: id,
        flag,
        last_poll: Mutex::new(Instant::now()),
        last_progress: Mutex::new(Instant::now() - PROGRESS_WRITE_INTERVAL),
        events: inner.events.clone(),
    };
    tracing::info!(job = id, kind = job.kind.as_str(), "job started");
    let started = Instant::now();
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| inner.exec.run(&job, &ctx)))
            .unwrap_or_else(|p| {
                let msg = p
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| p.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "panic".into());
                Err(ServiceError::Invalid(format!("internal error: {msg}")))
            });
    let outcome = (|| -> Result<()> {
        let conn = inner.db.conn()?;
        match &result {
            Ok(()) => jobs::finish(&conn, id, JobState::Completed, None)?,
            Err(ServiceError::Cancelled | ServiceError::Indexer(IndexerError::Cancelled)) => {
                if inner.stop.load(Ordering::SeqCst) && job.kind.restartable() {
                    // App shutting down: resume next start.
                    jobs::requeue(&conn, id, Some("interrupted by application exit"))?;
                } else {
                    jobs::finish(&conn, id, JobState::Cancelled, None)?;
                }
            }
            Err(
                e @ (ServiceError::Offline(_)
                | ServiceError::Indexer(IndexerError::VolumeUnavailable(_))),
            ) => {
                // Waits in the queue until the storage is back.
                jobs::requeue(&conn, id, Some(&e.to_string()))?;
            }
            Err(e)
                if is_retryable(e) && job.attempts < job.max_attempts && job.kind.restartable() =>
            {
                jobs::requeue(&conn, id, Some(&e.to_string()))?;
            }
            Err(e) => jobs::finish(&conn, id, JobState::Failed, Some(&e.to_string()))?,
        }
        Ok(())
    })();
    if let Err(e) = outcome {
        tracing::error!(job = id, error = %e, "cannot record job result");
    }
    match &result {
        Ok(()) => tracing::info!(
            job = id,
            secs = started.elapsed().as_secs_f32(),
            "job completed"
        ),
        Err(e) => tracing::warn!(job = id, error = %e, "job ended with error"),
    }
    inner.running.lock().remove(&id);
    inner.events.emit(AppEvent::JobsChanged);
}
