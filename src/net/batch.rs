//! Multi-threaded batch downloads tracked by a single task, with mirror fallback and verification.

use crate::net::task::{TaskError, TaskHandle, compute_file_sha1, fetch_file};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

/// Number of concurrent download workers used by installers.
pub const DEFAULT_WORKERS: usize = 16;
const ATTEMPTS_PER_URL: usize = 2;

/// One file to download. `urls` are tried in order (mirror first, then official).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadItem {
    pub urls: Vec<String>,
    pub dest: PathBuf,
    pub sha1: Option<String>,
    pub size: u64,
}

impl DownloadItem {
    pub fn new(urls: Vec<String>, dest: PathBuf, sha1: Option<String>, size: u64) -> Self {
        let mut unique = Vec::with_capacity(urls.len());
        for url in urls {
            if !url.is_empty() && !unique.contains(&url) {
                unique.push(url);
            }
        }
        Self { urls: unique, dest, sha1, size }
    }

    /// Whether the destination already holds a valid copy (by SHA-1, else by size, else by existence).
    pub fn is_satisfied(&self) -> bool {
        let Ok(meta) = fs::metadata(&self.dest) else {
            return false;
        };
        if !meta.is_file() {
            return false;
        }
        if let Some(expected) = &self.sha1 {
            return compute_file_sha1(&self.dest).is_ok_and(|h| h.eq_ignore_ascii_case(expected));
        }
        self.size == 0 || meta.len() == self.size
    }
}

fn fetch_item(item: &DownloadItem, task: &TaskHandle) -> Result<(), TaskError> {
    let mut last_err = TaskError::Other(format!("没有可用的下载地址: {}", item.dest.display()));
    for url in &item.urls {
        for _ in 0..ATTEMPTS_PER_URL {
            match fetch_file(url, &item.dest, item.sha1.as_deref(), task) {
                Ok(()) => return Ok(()),
                Err(TaskError::Cancelled) => return Err(TaskError::Cancelled),
                Err(e) => last_err = e,
            }
        }
    }
    Err(TaskError::Other(format!("{} ({last_err})", item.dest.display())))
}

/// Downloads every missing or invalid item with `workers` threads, updating the task's
/// byte and item counters. Stops at the first unrecoverable error or on cancel.
pub fn download_all(items: Vec<DownloadItem>, task: &TaskHandle, workers: usize) -> Result<(), TaskError> {
    let pending: Vec<DownloadItem> = items.into_iter().filter(|i| !i.is_satisfied()).collect();
    let total_bytes = pending.iter().map(|i| i.size).sum();
    task.reset_progress(total_bytes, pending.len() as u64);
    if pending.is_empty() {
        return Ok(());
    }

    let pending = Arc::new(pending);
    let next = Arc::new(AtomicUsize::new(0));
    let first_error: Arc<Mutex<Option<TaskError>>> = Arc::new(Mutex::new(None));
    let workers = workers.clamp(1, pending.len());

    thread::scope(|scope| {
        for _ in 0..workers {
            let (pending, next, first_error) = (pending.clone(), next.clone(), first_error.clone());
            scope.spawn(move || {
                loop {
                    if first_error.lock().unwrap().is_some() {
                        return;
                    }
                    let index = next.fetch_add(1, Ordering::SeqCst);
                    let Some(item) = pending.get(index) else {
                        return;
                    };
                    let result = task.checkpoint().and_then(|()| fetch_item(item, task));
                    match result {
                        Ok(()) => {
                            task.done_items.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(e) => {
                            first_error.lock().unwrap().get_or_insert(e);
                            return;
                        }
                    }
                }
            });
        }
    });

    match first_error.lock().unwrap().take() {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
