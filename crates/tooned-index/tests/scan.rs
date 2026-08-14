// SPDX-License-Identifier: AGPL-3.0-only

//! Integration test: full scan populates `files`/`shapes`/`conversions`
//! correctly, respecting `.gitignore` via the `ignore` crate (T050).

use std::fmt::Write as _;
use std::fs;

use tempfile::tempdir;

fn uniform_array_json(rows: usize) -> String {
    let mut s = String::from("[");
    for i in 0..rows {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(s, r#"{{"id":{i},"name":"row-{i}","active":true,"score":{i}}}"#);
    }
    s.push(']');
    s
}

#[test]
fn full_scan_populates_files_table_for_every_scanned_file() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("data.json"), uniform_array_json(20)).expect("write fixture");
    fs::write(dir.path().join("notes.txt"), "just some prose, nothing structured")
        .expect("write fixture");

    let summary = tooned_index::scan_full(dir.path(), &tooned_index::IndexFilter::default())
        .expect("scan_full");
    assert_eq!(summary.files_scanned, 2);
    assert_eq!(summary.files_classified, 1, "only data.json should be a recognized doctype");

    let detail = tooned_index::show_file(dir.path(), "data.json").expect("show_file(data.json)");
    assert_eq!(detail.file.doc_type.as_deref(), Some("json"));
    assert!(!detail.file.content_hash.is_empty());
    assert!(detail.file.size_bytes > 0);

    let notes = tooned_index::show_file(dir.path(), "notes.txt").expect("show_file(notes.txt)");
    assert_eq!(notes.file.doc_type, None);
}

#[test]
fn full_scan_populates_shapes_and_conversions_for_a_convertible_file() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("data.json"), uniform_array_json(20)).expect("write fixture");

    tooned_index::scan_full(dir.path(), &tooned_index::IndexFilter::default()).expect("scan_full");
    let detail = tooned_index::show_file(dir.path(), "data.json").expect("show_file");

    assert_eq!(detail.shapes.len(), 1);
    let shape = detail.shapes.first().expect("one shape row");
    assert_eq!(shape.shape_class, "uniform");
    match shape.uniformity_pct {
        Some(pct) => assert!(pct >= 0.9),
        None => panic!("expected uniformity_pct to be set for a uniform shape"),
    }

    assert_eq!(detail.conversions.len(), 1);
    let conversion = detail.conversions.first().expect("one conversion row");
    assert!(conversion.toon_bytes < conversion.json_bytes);
    assert!(conversion.savings_pct > 0.0);
}

#[test]
fn full_scan_respects_gitignore_via_the_ignore_crate() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join(".gitignore"), "ignored.json\n").expect("write .gitignore");
    fs::write(dir.path().join("ignored.json"), uniform_array_json(5)).expect("write fixture");
    fs::write(dir.path().join("kept.json"), uniform_array_json(5)).expect("write fixture");

    let summary = tooned_index::scan_full(dir.path(), &tooned_index::IndexFilter::default())
        .expect("scan_full");

    assert!(tooned_index::show_file(dir.path(), "kept.json").is_ok(), "kept.json must be indexed");
    assert!(
        tooned_index::show_file(dir.path(), "ignored.json").is_err(),
        "ignored.json must NOT be indexed (gitignored)"
    );
    // .gitignore itself is a dotfile (hidden) and .tooned/ is now gitignored
    // too, so only kept.json should have been scanned.
    assert_eq!(summary.files_scanned, 1);
}

#[test]
fn full_scan_never_indexes_its_own_tooned_directory() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("data.json"), uniform_array_json(3)).expect("write fixture");

    tooned_index::scan_full(dir.path(), &tooned_index::IndexFilter::default())
        .expect("first scan_full");
    // A second scan must not try to walk into (and re-index) its own
    // `.tooned/index.db` file.
    let summary = tooned_index::scan_full(dir.path(), &tooned_index::IndexFilter::default())
        .expect("second scan_full");
    assert_eq!(summary.files_scanned, 1);
}

#[test]
fn scan_with_exclude_skips_matching_files() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("data.json"), uniform_array_json(5)).expect("write fixture");
    fs::write(dir.path().join("excluded.json"), uniform_array_json(5)).expect("write fixture");

    let filter = tooned_index::IndexFilter {
        type_filter: None,
        excludes: vec!["excluded.json".to_string()],
        respect_gitignore: true,
    };
    let summary = tooned_index::scan_full(dir.path(), &filter).expect("scan_full");

    assert!(tooned_index::show_file(dir.path(), "data.json").is_ok(), "data.json must be indexed");
    assert!(
        tooned_index::show_file(dir.path(), "excluded.json").is_err(),
        "excluded.json must NOT be indexed"
    );
    assert_eq!(summary.files_scanned, 1);
}

#[test]
fn scan_with_type_filter_skips_non_matching_types() {
    let dir = tempdir().expect("tempdir");
    fs::write(dir.path().join("data.json"), uniform_array_json(5)).expect("write fixture");
    fs::write(dir.path().join("config.toml"), "[section]\nkey = \"value\"").expect("write fixture");

    let filter = tooned_index::IndexFilter {
        type_filter: Some(tooned_index::DocTypeFilter::Json),
        excludes: vec![],
        respect_gitignore: true,
    };
    let summary = tooned_index::scan_full(dir.path(), &filter).expect("scan_full");

    assert!(tooned_index::show_file(dir.path(), "data.json").is_ok(), "data.json must be indexed");
    assert!(
        tooned_index::show_file(dir.path(), "config.toml").is_err(),
        "config.toml must NOT be indexed (type filter)"
    );
    assert_eq!(summary.files_scanned, 1);
}
