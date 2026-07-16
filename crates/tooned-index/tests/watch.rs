// SPDX-License-Identifier: AGPL-3.0-only

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::tempdir;

#[test]
fn watch_with_stop_triggers_sync_on_new_file() {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();

    // Create an empty index to satisfy `watch_with_stop`.
    tooned_index::scan_full(root).expect("initial scan");
    let before = tooned_index::status(root).expect("status before watch");
    assert_eq!(before.file_count, 0);

    let stop = Arc::new(AtomicBool::new(false));
    let root_owned = root.to_path_buf();
    let stop_for_thread = Arc::clone(&stop);

    let handle = thread::spawn(move || {
        tooned_index::watch_with_stop(&root_owned, 50, &stop_for_thread)
            .expect("watch loop should exit cleanly");
    });

    // Give the watcher time to register.
    thread::sleep(Duration::from_millis(150));

    std::fs::write(root.join("new.json"), br#"{"id":1}"#).expect("write new file");

    // Poll status for up to a few seconds: filesystem watchers (especially
    // kqueue on macOS) can deliver events with variable latency.
    let deadline = Instant::now() + Duration::from_secs(5);
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
