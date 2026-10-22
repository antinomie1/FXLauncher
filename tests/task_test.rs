use fxlauncher::task::{TaskManager, TaskStatus};

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
