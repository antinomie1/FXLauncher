//! Global task management with non-blocking, pausable, and cancellable downloads.

use crate::net::client::DEFAULT_USER_AGENT;
use sha1::{Digest, Sha1};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TaskError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Network request error: {0}")]
    Network(#[from] Box<ureq::Error>),
    #[error("Download was cancelled by user")]
    Cancelled,
    #[error("SHA-1 checksum verification failed. Expected: {expected}, got: {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    #[error("{0}")]
    Other(String),
}

impl TaskError {
    /// Wraps any displayable error as a task failure message.
    pub fn other(e: impl std::fmt::Display) -> Self {
        Self::Other(e.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Queued,
    Running,
    Paused,
    Cancelled,
    Completed,
    Failed,
}

impl TaskStatus {
    /// Whether the task reached a terminal state.
    pub fn is_finished(self) -> bool {
        matches!(self, Self::Cancelled | Self::Completed | Self::Failed)
    }
}

#[derive(Debug, Clone)]
pub struct TaskSnapshot {
    pub id: u64,
    pub title: String,
    pub stage: String,
    pub status: TaskStatus,
    pub transferred_bytes: u64,
    pub total_bytes: u64,
    pub done_items: u64,
    pub total_items: u64,
    pub speed_bps: u64,
    pub progress_percent: f32,
    pub error_msg: Option<String>,
}

pub struct TaskHandle {
    pub id: u64,
    pub title: String,
    pub stage: Arc<Mutex<String>>,
    pub status: Arc<Mutex<TaskStatus>>,
    pub transferred: Arc<AtomicU64>,
    pub total: Arc<AtomicU64>,
    pub done_items: Arc<AtomicU64>,
    pub total_items: Arc<AtomicU64>,
    pub speed_bps: Arc<AtomicU64>,
    pub paused: Arc<AtomicBool>,
    pub cancelled: Arc<AtomicBool>,
    pub pause_notifier: Arc<(Mutex<bool>, Condvar)>,
    pub error_msg: Arc<Mutex<Option<String>>>,
    speed_sample: Mutex<(Instant, u64)>,
}

impl TaskHandle {
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn wait_if_paused(&self) -> bool {
        let (lock, cvar) = &*self.pause_notifier;
        let mut is_paused = lock.lock().unwrap();
        while *is_paused && !self.is_cancelled() {
            is_paused = cvar.wait(is_paused).unwrap();
        }
        !self.is_cancelled()
    }

    pub fn set_status(&self, status: TaskStatus) {
        *self.status.lock().unwrap() = status;
    }

    pub fn set_stage(&self, stage: &str) {
        *self.stage.lock().unwrap() = stage.to_string();
    }

    /// Marks the task failed with a user-visible message (unless it was cancelled).
    pub fn fail(&self, msg: &str) {
        *self.error_msg.lock().unwrap() = Some(msg.to_string());
        if !self.is_cancelled() {
            self.set_status(TaskStatus::Failed);
        }
        self.speed_bps.store(0, Ordering::Relaxed);
    }

    /// Resets byte and item counters for a new stage.
    pub fn reset_progress(&self, total_bytes: u64, total_items: u64) {
        self.transferred.store(0, Ordering::SeqCst);
        self.total.store(total_bytes, Ordering::SeqCst);
        self.done_items.store(0, Ordering::SeqCst);
        self.total_items.store(total_items, Ordering::SeqCst);
        *self.speed_sample.lock().unwrap() = (Instant::now(), 0);
    }

    /// Adds transferred bytes and refreshes the speed estimate (safe from many worker threads).
    pub fn add_progress(&self, bytes: u64) {
        let now_total = self.transferred.fetch_add(bytes, Ordering::Relaxed) + bytes;
        let mut sample = self.speed_sample.lock().unwrap();
        let elapsed = sample.0.elapsed();
        if elapsed >= Duration::from_millis(500) {
            let delta = now_total.saturating_sub(sample.1);
            self.speed_bps
                .store((delta as f64 / elapsed.as_secs_f64()) as u64, Ordering::Relaxed);
            *sample = (Instant::now(), now_total);
        }
    }

    /// Blocks while paused; returns `Err(Cancelled)` when the task was cancelled.
    pub fn checkpoint(&self) -> Result<(), TaskError> {
        if self.is_paused() {
            self.set_status(TaskStatus::Paused);
            if !self.wait_if_paused() {
                return Err(TaskError::Cancelled);
            }
            self.set_status(TaskStatus::Running);
        }
        if self.is_cancelled() {
            return Err(TaskError::Cancelled);
        }
        Ok(())
    }

    pub fn snapshot(&self) -> TaskSnapshot {
        let status = *self.status.lock().unwrap();
        let transferred_bytes = self.transferred.load(Ordering::Relaxed);
        let total_bytes = self.total.load(Ordering::Relaxed);
        let done_items = self.done_items.load(Ordering::Relaxed);
        let total_items = self.total_items.load(Ordering::Relaxed);
        let speed_bps = self.speed_bps.load(Ordering::Relaxed);
        let error_msg = self.error_msg.lock().unwrap().clone();

        let progress_percent = if status == TaskStatus::Completed {
            100.0
        } else if total_bytes > 0 {
            (transferred_bytes as f32 / total_bytes as f32 * 100.0).min(100.0)
        } else if total_items > 0 {
            done_items as f32 / total_items as f32 * 100.0
        } else {
            0.0
        };

        TaskSnapshot {
            id: self.id,
            title: self.title.clone(),
            stage: self.stage.lock().unwrap().clone(),
            status,
            transferred_bytes,
            total_bytes,
            done_items,
            total_items,
            speed_bps,
            progress_percent,
            error_msg,
        }
    }
}

pub struct TaskManager {
    next_id: AtomicU64,
    tasks: Arc<Mutex<Vec<Arc<TaskHandle>>>>,
}

impl Default for TaskManager {
    fn default() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            tasks: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl TaskManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_task(&self, title: &str) -> Arc<TaskHandle> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let handle = Arc::new(TaskHandle {
            id,
            title: title.to_string(),
            stage: Arc::new(Mutex::new(String::new())),
            status: Arc::new(Mutex::new(TaskStatus::Queued)),
            transferred: Arc::new(AtomicU64::new(0)),
            total: Arc::new(AtomicU64::new(0)),
            done_items: Arc::new(AtomicU64::new(0)),
            total_items: Arc::new(AtomicU64::new(0)),
            speed_bps: Arc::new(AtomicU64::new(0)),
            paused: Arc::new(AtomicBool::new(false)),
            cancelled: Arc::new(AtomicBool::new(false)),
            pause_notifier: Arc::new((Mutex::new(false), Condvar::new())),
            error_msg: Arc::new(Mutex::new(None)),
            speed_sample: Mutex::new((Instant::now(), 0)),
        });

        self.tasks.lock().unwrap().push(handle.clone());
        handle
    }

    /// Creates a task and runs `job` on a background thread, recording completion or failure.
    pub fn run_task<F>(&self, title: &str, job: F) -> Arc<TaskHandle>
    where
        F: FnOnce(&TaskHandle) -> Result<(), TaskError> + Send + 'static,
    {
        let task = self.create_task(title);
        let worker = task.clone();
        thread::spawn(move || {
            worker.set_status(TaskStatus::Running);
            match job(&worker) {
                Ok(()) => {
                    worker.set_status(TaskStatus::Completed);
                    worker.speed_bps.store(0, Ordering::Relaxed);
                }
                Err(TaskError::Cancelled) => worker.set_status(TaskStatus::Cancelled),
                Err(e) => worker.fail(&e.to_string()),
            }
        });
        task
    }

    pub fn pause_task(&self, id: u64) -> bool {
        let tasks = self.tasks.lock().unwrap();
        if let Some(task) = tasks.iter().find(|t| t.id == id) {
            task.paused.store(true, Ordering::SeqCst);
            let (lock, _) = &*task.pause_notifier;
            *lock.lock().unwrap() = true;
            let mut status = task.status.lock().unwrap();
            if *status == TaskStatus::Running {
                *status = TaskStatus::Paused;
            }
            return true;
        }
        false
    }

    pub fn resume_task(&self, id: u64) -> bool {
        let tasks = self.tasks.lock().unwrap();
        if let Some(task) = tasks.iter().find(|t| t.id == id) {
            task.paused.store(false, Ordering::SeqCst);
            let (lock, cvar) = &*task.pause_notifier;
            *lock.lock().unwrap() = false;
            cvar.notify_all();
            let mut status = task.status.lock().unwrap();
            if *status == TaskStatus::Paused {
                *status = TaskStatus::Running;
            }
            return true;
        }
        false
    }

    pub fn cancel_task(&self, id: u64) -> bool {
        let tasks = self.tasks.lock().unwrap();
        if let Some(task) = tasks.iter().find(|t| t.id == id) {
            task.cancelled.store(true, Ordering::SeqCst);
            let (lock, cvar) = &*task.pause_notifier;
            *lock.lock().unwrap() = false;
            cvar.notify_all();
            *task.status.lock().unwrap() = TaskStatus::Cancelled;
            return true;
        }
        false
    }

    pub fn get_task(&self, id: u64) -> Option<Arc<TaskHandle>> {
        self.tasks
            .lock()
            .unwrap()
            .iter()
            .find(|t| t.id == id)
            .cloned()
    }

    pub fn list_snapshots(&self) -> Vec<TaskSnapshot> {
        let tasks = self.tasks.lock().unwrap();
        tasks.iter().map(|t| t.snapshot()).collect()
    }

    /// Removes completed, failed and cancelled tasks from the list.
    pub fn clear_finished(&self) {
        self.tasks
            .lock()
            .unwrap()
            .retain(|t| !t.status.lock().unwrap().is_finished());
    }

    /// Whether any task is still queued, running or paused.
    pub fn has_active(&self) -> bool {
        self.tasks
            .lock()
            .unwrap()
            .iter()
            .any(|t| !t.status.lock().unwrap().is_finished())
    }
}

/// Computes the SHA-1 hexadecimal checksum of a file.
pub fn compute_file_sha1(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Streams one URL into `dest` (resuming a `.part` file), adding bytes to the task's counters.
/// Does not change the task status; the caller decides how errors are reported.
pub fn fetch_file(url: &str, dest: &Path, expected_sha1: Option<&str>, task: &TaskHandle) -> Result<(), TaskError> {
    if let Some(parent) = dest.parent() {
        crate::env::paths::ensure_dir(parent)?;
    }

    let temp_dest = dest.with_extension(format!(
        "{}.part",
        dest.extension().and_then(|s| s.to_str()).unwrap_or("tmp")
    ));
    let start_byte = fs::metadata(&temp_dest).map(|m| m.len()).unwrap_or(0);

    let mut req = ureq::get(url)
        .set("User-Agent", DEFAULT_USER_AGENT)
        .timeout(Duration::from_secs(60));
    if start_byte > 0 {
        req = req.set("Range", &format!("bytes={start_byte}-"));
    }
    let resp = req.call().map_err(Box::new)?;

    let resumed = start_byte > 0 && resp.status() == 206;
    let mut file = if resumed {
        task.add_progress(start_byte);
        let mut f = OpenOptions::new().append(true).open(&temp_dest)?;
        f.seek(SeekFrom::End(0))?;
        f
    } else {
        File::create(&temp_dest)?
    };

    let mut reader = resp.into_reader();
    let mut buffer = [0u8; 65536];
    loop {
        if let Err(e) = task.checkpoint() {
            drop(file);
            let _ = fs::remove_file(&temp_dest);
            return Err(e);
        }
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        file.write_all(&buffer[..n])?;
        task.add_progress(n as u64);
    }
    file.flush()?;
    drop(file);

    if let Some(expected) = expected_sha1 {
        let actual = compute_file_sha1(&temp_dest)?;
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = fs::remove_file(&temp_dest);
            return Err(TaskError::ChecksumMismatch {
                expected: expected.to_string(),
                actual,
            });
        }
    }

    fs::rename(&temp_dest, dest)?;
    Ok(())
}

/// Downloads a file in chunks while reporting progress and responding to pause/cancel signals.
pub fn download_file_with_task(
    url: &str,
    dest: &Path,
    expected_sha1: Option<&str>,
    task: &TaskHandle,
) -> Result<(), TaskError> {
    task.set_status(TaskStatus::Running);
    task.reset_progress(0, 1);

    // Learn the size up front so the progress bar is meaningful.
    if let Ok(resp) = ureq::head(url).set("User-Agent", DEFAULT_USER_AGENT).call()
        && let Some(len) = resp.header("Content-Length").and_then(|v| v.parse::<u64>().ok())
    {
        task.total.store(len, Ordering::SeqCst);
    }

    match fetch_file(url, dest, expected_sha1, task) {
        Ok(()) => {
            task.done_items.store(1, Ordering::SeqCst);
            task.set_status(TaskStatus::Completed);
            task.speed_bps.store(0, Ordering::Relaxed);
            Ok(())
        }
        Err(TaskError::Cancelled) => {
            task.set_status(TaskStatus::Cancelled);
            Err(TaskError::Cancelled)
        }
        Err(e) => {
            task.fail(&e.to_string());
            Err(e)
        }
    }
}

/// Spawns a background worker thread to execute the download task without blocking main thread.
pub fn spawn_download_task(
    url: String,
    dest: PathBuf,
    expected_sha1: Option<String>,
    task: Arc<TaskHandle>,
) -> thread::JoinHandle<Result<(), TaskError>> {
    thread::spawn(move || download_file_with_task(&url, &dest, expected_sha1.as_deref(), &task))
}
