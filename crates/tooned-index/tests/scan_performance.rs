// SPDX-License-Identifier: AGPL-3.0-only

//! Timed performance test (T061b, SC-005): a full scan of a fixture project
//! with 1,000+ files completes well under a minute, and `index sync` after
//! touching only a handful of files is markedly faster than the initial
//! full scan. Fixture files are generated programmatically into a tempdir
//! rather than committed to the repo.

use std::fs;
use std::time::{Duration, Instant, SystemTime};

use tempfile::tempdir;

const FILE_COUNT: usize = 1_200;

fn uniform_row_json(i: usize) -> String {
    format!(r#"{{"id":{i},"name":"row-{i}","active":true,"score":{i}}}"#)
}

fn populate_fixture_project(root: &std::path::Path) -> std::io::Result<()> {
    for i in 0..FILE_COUNT {
        let contents = format!("[{},{}]", uniform_row_json(i), uniform_row_json(i + 1));
        fs::write(root.join(format!("file-{i:05}.json")), contents)?;
    }
    Ok(())
}

#[test]
fn full_scan_of_1000_plus_files_completes_well_under_a_minute() {
    let dir = tempdir().expect("tempdir");
    populate_fixture_project(dir.path()).expect("populate fixture project");

    let start = Instant::now();
    let summary = tooned_index::scan_full(dir.path()).expect("scan_full");
    let elapsed = start.elapsed();

    assert_eq!(summary.files_scanned, FILE_COUNT);
    assert!(
        elapsed < Duration::from_secs(30),
        "full scan of {FILE_COUNT} files took {elapsed:?}, expected well under a minute"
    );
}

#[test]
fn incremental_sync_after_touching_a_few_files_is_markedly_faster_than_full_scan() {
    let dir = tempdir().expect("tempdir");
    populate_fixture_project(dir.path()).expect("populate fixture project");

    let full_scan_start = Instant::now();
    tooned_index::scan_full(dir.path()).expect("initial scan_full");
    let full_scan_elapsed = full_scan_start.elapsed();

    // Establish a warm baseline for re-classifying every file from disk.
    // This mirrors a naive re-hash-everything sync and is measured against the
    // same cached filesystem state, giving a fair comparison without the noise
    // of a cold first scan.
    let baseline_start = Instant::now();
    tooned_index::scan_full(dir.path()).expect("warm baseline scan_full");
    let baseline_elapsed = baseline_start.elapsed();

    // Touch (content + mtime change) only a handful of files.
    let future = SystemTime::now() + Duration::from_mins(2);
    for i in 0..5 {
        let path = dir.path().join(format!("file-{i:05}.json"));
        fs::write(&path, format!("[{}]", uniform_row_json(i + 999))).expect("rewrite fixture file");
        // Open with write permission; Windows requires a writable handle to call
        // `SetFileTime` via `set_modified`.
        let file = fs::OpenOptions::new().write(true).open(&path).expect("open fixture file");
        file.set_modified(future).expect("set_modified");
    }

    let sync_start = Instant::now();
    let summary = tooned_index::sync(dir.path()).expect("sync");
    let sync_elapsed = sync_start.elapsed();

    assert_eq!(summary.updated, 5);
    assert_eq!(summary.unchanged, FILE_COUNT - 5);
    assert!(
        sync_elapsed < full_scan_elapsed,
        "incremental sync ({sync_elapsed:?}) must be faster than the initial full scan ({full_scan_elapsed:?})"
    );
    // A generous ceiling well above what stat-first skipping should need,
    // measured against a warm full reclassification so the comparison isn't
    // skewed by cold filesystem-cache noise on slower CI runners.
    assert!(
        sync_elapsed < baseline_elapsed * 3 / 4,
        "sync ({sync_elapsed:?}) should be markedly faster than a warm full reclassification ({baseline_elapsed:?}), \
         not just marginally faster -- stat-first mtime-check must actually skip re-hashing"
    );
}
