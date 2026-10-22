//! Global task management with non-blocking, pausable, and cancellable downloads.

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

#[derive(Debug, Clone)]
pub struct TaskSnapshot {
    pub id: u64,
    pub title: String,
    pub status: TaskStatus,
    pub transferred_bytes: u64,
    pub total_bytes: u64,
    pub speed_bps: u64,
    pub progress_percent: f32,
    pub error_msg: Option<String>,
}

pub struct TaskHandle {
    pub id: u64,
    pub title: String,
    pub status: Arc<Mutex<TaskStatus>>,
    pub transferred: Arc<AtomicU64>,
    pub total: Arc<AtomicU64>,
    pub speed_bps: Arc<AtomicU64>,
    pub paused: Arc<AtomicBool>,
    pub cancelled: Arc<AtomicBool>,
    pub pause_notifier: Arc<(Mutex<bool>, Condvar)>,
    pub error_msg: Arc<Mutex<Option<String>>>,
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

    pub fn snapshot(&self) -> TaskSnapshot {
        let status = *self.status.lock().unwrap();
        let transferred_bytes = self.transferred.load(Ordering::Relaxed);
        let total_bytes = self.total.load(Ordering::Relaxed);
        let speed_bps = self.speed_bps.load(Ordering::Relaxed);
        let error_msg = self.error_msg.lock().unwrap().clone();

        let progress_percent = if total_bytes > 0 {
            (transferred_bytes as f32 / total_bytes as f32) * 100.0
        } else {
            0.0
        };

        TaskSnapshot {
            id: self.id,
            title: self.title.clone(),
            status,
            transferred_bytes,
            total_bytes,
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
            status: Arc::new(Mutex::new(TaskStatus::Queued)),
            transferred: Arc::new(AtomicU64::new(0)),
            total: Arc::new(AtomicU64::new(0)),
            speed_bps: Arc::new(AtomicU64::new(0)),
            paused: Arc::new(AtomicBool::new(false)),
            cancelled: Arc::new(AtomicBool::new(false)),
            pause_notifier: Arc::new((Mutex::new(false), Condvar::new())),
            error_msg: Arc::new(Mutex::new(None)),
        });

        self.tasks.lock().unwrap().push(handle.clone());
        handle
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
}

/// Downloads a file in chunks while reporting progress and responding to pause/cancel signals.
pub fn download_file_with_task(
    url: &str,
    dest: &Path,
    expected_sha1: Option<&str>,
    task: &TaskHandle,
) -> Result<(), TaskError> {
    *task.status.lock().unwrap() = TaskStatus::Running;

    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }

    let temp_dest = dest.with_extension(format!(
        "{}.part",
        dest.extension().and_then(|s| s.to_str()).unwrap_or("tmp")
    ));

    let mut start_byte = 0u64;
    if temp_dest.exists() {
        if let Ok(metadata) = fs::metadata(&temp_dest) {
            start_byte = metadata.len();
        }
    }

    let mut req = ureq::get(url).timeout(Duration::from_secs(30));
    if start_byte > 0 {
        req = req.set("Range", &format!("bytes={start_byte}-"));
    }

    let resp = match req.call() {
        Ok(r) => r,
        Err(e) => {
            let err_str = e.to_string();
            *task.error_msg.lock().unwrap() = Some(err_str);
            *task.status.lock().unwrap() = TaskStatus::Failed;
            return Err(TaskError::Network(Box::new(e)));
        }
    };

    let content_len = resp
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);

    let total = if start_byte > 0 && resp.status() == 206 {
        start_byte + content_len
    } else {
        content_len
    };
    task.total.store(total, Ordering::SeqCst);
    task.transferred.store(start_byte, Ordering::SeqCst);

    let mut file = if start_byte > 0 && resp.status() == 206 {
        let mut f = OpenOptions::new().append(true).open(&temp_dest)?;
        f.seek(SeekFrom::End(0))?;
        f
    } else {
        task.transferred.store(0, Ordering::SeqCst);
        File::create(&temp_dest)?
    };

    let mut reader = resp.into_reader();
    let mut buffer = [0u8; 65536]; // 64 KB buffer
    let mut last_speed_check = Instant::now();
    let mut bytes_since_last_check = 0u64;

    loop {
        if task.is_cancelled() {
            drop(file);
            let _ = fs::remove_file(&temp_dest);
            *task.status.lock().unwrap() = TaskStatus::Cancelled;
            return Err(TaskError::Cancelled);
        }

        if task.is_paused() {
            *task.status.lock().unwrap() = TaskStatus::Paused;
            if !task.wait_if_paused() {
                drop(file);
                let _ = fs::remove_file(&temp_dest);
                *task.status.lock().unwrap() = TaskStatus::Cancelled;
                return Err(TaskError::Cancelled);
            }
            *task.status.lock().unwrap() = TaskStatus::Running;
        }

        let bytes_read = match reader.read(&mut buffer) {
            Ok(0) => break, // EOF reached
            Ok(n) => n,
            Err(e) => {
                let err_str = e.to_string();
                *task.error_msg.lock().unwrap() = Some(err_str);
                *task.status.lock().unwrap() = TaskStatus::Failed;
                return Err(TaskError::Io(e));
            }
        };

        file.write_all(&buffer[..bytes_read])?;
        task.transferred.fetch_add(bytes_read as u64, Ordering::Relaxed);
        bytes_since_last_check += bytes_read as u64;

        if last_speed_check.elapsed() >= Duration::from_millis(500) {
            let elapsed_secs = last_speed_check.elapsed().as_secs_f64();
            if elapsed_secs > 0.0 {
                let bps = (bytes_since_last_check as f64 / elapsed_secs) as u64;
                task.speed_bps.store(bps, Ordering::Relaxed);
            }
            bytes_since_last_check = 0;
            last_speed_check = Instant::now();
        }
    }

    file.flush()?;
    drop(file);

    // Verify SHA-1 if provided
    if let Some(expected) = expected_sha1 {
        let mut check_file = File::open(&temp_dest)?;
        let mut hasher = Sha1::new();
        let mut check_buf = [0u8; 65536];
        loop {
            let n = check_file.read(&mut check_buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&check_buf[..n]);
        }
        let actual = format!("{:x}", hasher.finalize());
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = fs::remove_file(&temp_dest);
            let err = TaskError::ChecksumMismatch {
                expected: expected.to_string(),
                actual,
            };
            *task.error_msg.lock().unwrap() = Some(err.to_string());
            *task.status.lock().unwrap() = TaskStatus::Failed;
            return Err(err);
        }
    }

    // Rename .part to final destination
    fs::rename(&temp_dest, dest)?;
    *task.status.lock().unwrap() = TaskStatus::Completed;
    task.speed_bps.store(0, Ordering::Relaxed);

    Ok(())
}

/// Spawns a background worker thread to execute the download task without blocking main thread.
pub fn spawn_download_task(
    url: String,
    dest: PathBuf,
    expected_sha1: Option<String>,
    task: Arc<TaskHandle>,
) -> thread::JoinHandle<Result<(), TaskError>> {
    thread::spawn(move || {
        let res = download_file_with_task(&url, &dest, expected_sha1.as_deref(), &task);
        if let Err(e) = &res {
            *task.error_msg.lock().unwrap() = Some(e.to_string());
            if !matches!(e, TaskError::Cancelled) {
                *task.status.lock().unwrap() = TaskStatus::Failed;
            }
        }
        res
    })
}
