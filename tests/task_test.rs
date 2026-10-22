use fxlauncher::task::{TaskManager, TaskStatus, compute_file_sha1};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_compute_file_sha1() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("hello.txt");
    fs::write(&file, b"hello world\n").unwrap();

    let sha1 = compute_file_sha1(&file).unwrap();
    // echo -n "hello world\n" | sha1sum -> 22596363b3de40b06f981fb85d82312e8c0ed511
    assert_eq!(sha1, "22596363b3de40b06f981fb85d82312e8c0ed511");
}

#[test]
fn test_task_manager_lifecycle() {
    let manager = TaskManager::new();
    let task = manager.create_task("测试下载任务");

    assert_eq!(task.id, 1);
    assert_eq!(*task.status.lock().unwrap(), TaskStatus::Queued);

    let snapshot = task.snapshot();
    assert_eq!(snapshot.id, 1);
    assert_eq!(snapshot.title, "测试下载任务");
    assert_eq!(snapshot.status, TaskStatus::Queued);

    // Test pause
    *task.status.lock().unwrap() = TaskStatus::Running;
    assert!(manager.pause_task(1));
    assert_eq!(*task.status.lock().unwrap(), TaskStatus::Paused);
    assert!(task.is_paused());

    // Test resume
    assert!(manager.resume_task(1));
    assert_eq!(*task.status.lock().unwrap(), TaskStatus::Running);
    assert!(!task.is_paused());

    // Test cancel
    assert!(manager.cancel_task(1));
    assert_eq!(*task.status.lock().unwrap(), TaskStatus::Cancelled);
    assert!(task.is_cancelled());

    // Listing
    let list = manager.list_snapshots();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, 1);
    assert_eq!(list[0].status, TaskStatus::Cancelled);
}
