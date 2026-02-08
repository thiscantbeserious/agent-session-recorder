//! Integration tests for ProcessGuard

use agr::utils::process_guard::ProcessGuard;

#[test]
fn new_guard_is_not_interrupted() {
    let guard = ProcessGuard::new();
    assert!(!guard.is_interrupted());
}

#[test]
fn wait_or_kill_returns_when_child_exits() {
    let guard = ProcessGuard::new();
    // Spawn a process that exits immediately
    let mut child = std::process::Command::new("true")
        .spawn()
        .expect("failed to spawn `true`");
    let status = guard.wait_or_kill(&mut child).unwrap();
    assert!(status.success());
}

#[test]
fn wait_or_kill_returns_failure_status() {
    let guard = ProcessGuard::new();
    let mut child = std::process::Command::new("false")
        .spawn()
        .expect("failed to spawn `false`");
    let status = guard.wait_or_kill(&mut child).unwrap();
    assert!(!status.success());
}

#[cfg(unix)]
#[test]
fn orphan_detection_with_current_parent_is_false() {
    // We're not orphaned right now — parent is the test runner
    let guard = ProcessGuard::new();
    // is_interrupted should be false (no signal, not orphaned)
    assert!(!guard.is_interrupted());
}
