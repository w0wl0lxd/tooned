// SPDX-License-Identifier: AGPL-3.0-only

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::tempdir;

const WATCH_TIMEOUT_SECS: u64 = 5;

#[test]
fn watch_with_stop_triggers_sync_on_new_file() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    // Create an empty index to satisfy `watch_with_stop`.
    tooned_index::scan_full(root, &tooned_index::IndexFilter::default()).expect("initial scan");
    let before = tooned_index::status(root).expect("status before watch");
    assert_eq!(before.file_count, 0);

    let stop = Arc::new(AtomicBool::new(false));
    let root_owned = root.to_path_buf();
    let stop_for_thread = Arc::clone(&stop);

    let handle = thread::spawn(move || {
        tooned_index::watch_with_stop(
            &root_owned,
            50,
            &stop_for_thread,
            &tooned_index::IndexFilter::default(),
        )
        .expect("watch loop should exit cleanly");
    });

    // Give the watcher time to register. FSEvents on macOS can take a
    // noticeable amount of time to start delivering events, especially on
    // CI runners, so allow a generous warm-up period.
    thread::sleep(Duration::from_millis(500));

    std::fs::write(root.join("new.json"), br#"{"id":1}"#).expect("write new file");

    // Poll status for the platform-specific timeout: filesystem watchers
    // (especially FSEvents on macOS) can deliver events with variable latency.
    let deadline = Instant::now() + Duration::from_secs(WATCH_TIMEOUT_SECS);
    while Instant::now() < deadline {
        thread::sleep(Duration::from_millis(100));
        if tooned_index::status(root).is_ok_and(|s| s.file_count == 1) {
            break;
        }
    }

    stop.store(true, Ordering::SeqCst);
    handle.join().expect("watch thread should join");

    let after = tooned_index::status(root).expect("status after watch");
    assert_eq!(after.file_count, 1, "watch should have synced the new file");
}

#[test]
fn watch_ignores_gitignored_directories() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    // A `.gitignore` that ignores `target/` should be respected by the watcher.
    std::fs::write(root.join(".gitignore"), b"target/\n").expect("write .gitignore");
    // Pre-create the ignored directory so the recursive watcher has a chance
    // to see activity inside it (the watcher will still filter it out).
    std::fs::create_dir(root.join("target")).expect("create target dir");

    // Initial scan must see the .gitignore but not the ignored directory.
    tooned_index::scan_full(root, &tooned_index::IndexFilter::default()).expect("initial scan");
    let before = tooned_index::status(root).expect("status before watch");
    assert_eq!(before.file_count, 0, "gitignored directory should not be indexed");

    let stop = Arc::new(AtomicBool::new(false));
    let root_owned = root.to_path_buf();
    let stop_for_thread = Arc::clone(&stop);

    let handle = thread::spawn(move || {
        tooned_index::watch_with_stop(
            &root_owned,
            50,
            &stop_for_thread,
            &tooned_index::IndexFilter::default(),
        )
        .expect("watch loop should exit cleanly");
    });

    thread::sleep(Duration::from_millis(500));

    std::fs::write(root.join("tracked.json"), br#"{"id":1}"#).expect("write tracked file");
    std::fs::write(root.join("target").join("ignored.json"), br#"{"id":2}"#)
        .expect("write ignored file");

    let deadline = Instant::now() + Duration::from_secs(WATCH_TIMEOUT_SECS);
    while Instant::now() < deadline {
        thread::sleep(Duration::from_millis(100));
        if tooned_index::status(root).is_ok_and(|s| s.file_count == 1) {
            break;
        }
    }

    stop.store(true, Ordering::SeqCst);
    handle.join().expect("watch thread should join");

    let after = tooned_index::status(root).expect("status after watch");
    assert_eq!(after.file_count, 1, "only the tracked file should be synced");

    let detail = tooned_index::show_file(root, "tracked.json").expect("tracked file detail");
    assert!(detail.file.path.ends_with("tracked.json"));
}
