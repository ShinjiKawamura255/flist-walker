mod perf;

#[cfg(not(windows))]
use super::filelist_reader::windows_path_to_wsl;
use super::filelist_reader::{
    open_validated_filelist, resolve_filelist_entry_candidates, validate_filelist_encoding,
};
use super::filelist_writer::{
    annotate_write_target_error, filelist_modified_time, normalize_filelist_entry_for_text_compare,
    visit_ancestor_directories,
};
use super::*;
use anyhow::Context;
use std::collections::HashSet;
use std::fs;
use std::io::BufRead;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn test_root(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("fff-rs-{name}-{nonce}"))
}

fn sleep_for_timestamp_tick() {
    std::thread::sleep(Duration::from_millis(1100));
}

fn canonical_or_original(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn same_path(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    canonical_or_original(left) == canonical_or_original(right)
}

fn contains_path<T: AsRef<Path>>(entries: &[T], expected: &Path) -> bool {
    entries
        .iter()
        .any(|entry| same_path(entry.as_ref(), expected))
}

fn expected_backslash_relative_path(root: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        root.join(r"nested\item.txt")
    }
    #[cfg(not(windows))]
    {
        root.join("nested/item.txt")
    }
}

fn assert_filelist_source_matches(source: &IndexSource, expected: &Path) {
    match source {
        IndexSource::FileList(actual) => {
            assert!(
                same_path(actual, expected),
                "unexpected FileList source: actual={} expected={}",
                actual.display(),
                expected.display()
            );
        }
        other => panic!("expected FileList source, got {other:?}"),
    }
}

#[test]
fn find_filelist_prefers_uppercase_name() {
    let root = test_root("find-upper");
    fs::create_dir_all(&root).expect("create dir");
    fs::write(root.join("FileList.txt"), "a.txt\n").expect("write upper");
    fs::write(root.join("filelist.txt"), "b.txt\n").expect("write lower");

    let found = find_filelist(&root).expect("find filelist");
    assert_eq!(
        found.file_name().and_then(|s| s.to_str()),
        Some("FileList.txt")
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn find_filelist_accepts_lowercase_name() {
    let root = test_root("find-lower");
    fs::create_dir_all(&root).expect("create dir");
    fs::write(root.join("filelist.txt"), "a.txt\n").expect("write lower");

    let found = find_filelist(&root).expect("find filelist");
    assert!(same_path(&found, &root.join("filelist.txt")));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn parse_filelist_resolves_relative_and_absolute_paths() {
    let root = test_root("parse");
    fs::create_dir_all(&root).expect("create dir");
    let rel_file = root.join("alpha.txt");
    let abs_file = root.join("beta.txt");
    let missing = root.join("missing.txt");
    fs::write(&rel_file, "x").expect("write rel");
    fs::write(&abs_file, "y").expect("write abs");
    let filelist = root.join("FileList.txt");
    fs::write(
        &filelist,
        format!(
            "# comment\nalpha.txt\n{}\nmissing.txt\n",
            abs_file.display()
        ),
    )
    .expect("write filelist");

    let parsed = parse_filelist(&filelist, &root, true, true).expect("parse filelist");
    assert!(parsed.contains(&rel_file));
    assert!(parsed.contains(&abs_file));
    assert!(parsed.contains(&missing));
    assert_eq!(parsed.len(), 3);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn build_index_uses_filelist_when_present() {
    let root = test_root("build-filelist");
    fs::create_dir_all(&root).expect("create dir");
    let listed = root.join("listed.txt");
    let hidden = root.join("hidden.txt");
    fs::write(&listed, "ok").expect("write listed");
    fs::write(&hidden, "no").expect("write hidden");
    fs::write(root.join("FileList.txt"), "listed.txt\n").expect("write filelist");

    let out = build_index(&root, true, true, true).expect("build index");
    assert!(contains_path(&out, &listed));
    assert!(!contains_path(&out, &hidden));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn build_index_walks_when_filelist_missing() {
    let root = test_root("build-walker");
    let nested = root.join("dir");
    fs::create_dir_all(&nested).expect("create nested dir");
    let file = nested.join("app.py");
    fs::write(&file, "print('hi')").expect("write file");

    let out = build_index(&root, true, true, true).expect("build index");
    assert!(contains_path(&out, &file));
    assert!(contains_path(&out, &nested));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn build_index_with_metadata_reports_filelist_source() {
    let root = test_root("source-filelist");
    fs::create_dir_all(&root).expect("create dir");
    let listed = root.join("listed.txt");
    fs::write(&listed, "ok").expect("write listed");
    fs::write(root.join("filelist.txt"), "listed.txt\n").expect("write filelist");

    let out = build_index_with_metadata(&root, true, true, true).expect("build index");
    assert_filelist_source_matches(&out.source, &root.join("filelist.txt"));
    assert!(contains_path(&out.entries, &listed));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn build_index_with_metadata_reports_walker_source() {
    let root = test_root("source-walker");
    fs::create_dir_all(root.join("sub")).expect("create sub");

    let out = build_index_with_metadata(&root, true, true, true).expect("build index");
    assert!(matches!(out.source, IndexSource::Walker));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn walkers_are_separated_for_files_and_dirs() {
    let root = test_root("walk-separate");
    let folder = root.join("docs");
    fs::create_dir_all(&folder).expect("create folder");
    let file = folder.join("a.txt");
    fs::write(&file, "x").expect("write file");

    let files = walk_files(&root);
    let dirs = walk_dirs(&root);
    assert!(files.contains(&file));
    assert!(!files.contains(&folder));
    assert!(dirs.contains(&folder));
    assert!(!dirs.contains(&file));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn build_index_can_disable_filelist() {
    let root = test_root("disable-filelist");
    fs::create_dir_all(&root).expect("create dir");
    let listed = root.join("listed.txt");
    let extra = root.join("extra.txt");
    fs::write(&listed, "ok").expect("write listed");
    fs::write(&extra, "ok").expect("write extra");
    fs::write(root.join("FileList.txt"), "listed.txt\n").expect("write filelist");

    let out = build_index_with_metadata(&root, false, true, true).expect("build index");
    assert!(matches!(out.source, IndexSource::Walker));
    assert!(contains_path(&out.entries, &listed));
    assert!(contains_path(&out.entries, &extra));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn build_filelist_text_uses_relative_paths_when_possible() {
    let root = test_root("filelist-text");
    let folder = root.join("a");
    fs::create_dir_all(&folder).expect("create folder");
    let file = folder.join("b.txt");
    fs::write(&file, "x").expect("write file");

    let text = build_filelist_text(&[file.clone(), folder.clone()], &root);
    assert!(text.contains(&format!("a{}b.txt", std::path::MAIN_SEPARATOR)));
    assert!(text.contains("a\n"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn write_filelist_writes_file() {
    let root = test_root("write-filelist");
    let folder = root.join("x");
    fs::create_dir_all(&folder).expect("create folder");
    let file = folder.join("run.exe");
    fs::write(&file, "bin").expect("write file");

    let out = write_filelist(&root, &[file.clone(), folder.clone()], "FileList.txt", true)
        .expect("write");
    assert!(out.exists());
    let content = fs::read_to_string(&out).expect("read filelist");
    assert!(content.contains(&format!("x{}run.exe", std::path::MAIN_SEPARATOR)));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn write_filelist_appends_child_filelist_to_ancestor_filelists_without_touching_mtime() {
    let top = test_root("write-filelist-propagate");
    let parent = top.join("parent");
    let root = parent.join("child");
    let sibling = parent.join("keep.txt");
    fs::create_dir_all(&root).expect("create child root");
    fs::write(&sibling, "keep").expect("write sibling");
    let parent_filelist = parent.join("FileList.txt");
    fs::write(&parent_filelist, "keep.txt\n").expect("write parent filelist");
    sleep_for_timestamp_tick();
    let before_modified = filelist_modified_time(&parent_filelist).expect("mtime before");

    let child_entry = root.join("src/main.rs");
    fs::create_dir_all(child_entry.parent().expect("child parent")).expect("create src");
    fs::write(&child_entry, "fn main() {}").expect("write child entry");

    let out = write_filelist(&root, &[child_entry], "FileList.txt", true).expect("write child");

    let parent_content = fs::read_to_string(&parent_filelist).expect("read parent filelist");
    assert!(parent_content.contains("keep.txt"));
    assert!(parent_content.contains(&format!("child{}FileList.txt", std::path::MAIN_SEPARATOR)));
    let after_modified = filelist_modified_time(&parent_filelist).expect("mtime after");
    assert_eq!(after_modified, before_modified);

    let parsed_parent =
        parse_filelist(&parent_filelist, &parent, true, true).expect("parse parent filelist");
    assert!(contains_path(&parsed_parent, &out));
    let _ = fs::remove_dir_all(&top);
}

#[test]
fn write_filelist_does_not_duplicate_existing_ancestor_child_reference() {
    let top = test_root("write-filelist-propagate-dedup");
    let parent = top.join("parent");
    let root = parent.join("child");
    fs::create_dir_all(&root).expect("create child root");
    let child_entry = root.join("src/main.rs");
    fs::create_dir_all(child_entry.parent().expect("child parent")).expect("create src");
    fs::write(&child_entry, "fn main() {}").expect("write child entry");
    let child_filelist = root.join("FileList.txt");
    let parent_filelist = parent.join("FileList.txt");
    fs::create_dir_all(&parent).expect("create parent");
    fs::write(
        &parent_filelist,
        format!("./child{}FileList.txt\n", std::path::MAIN_SEPARATOR),
    )
    .expect("write parent filelist");

    write_filelist(&root, &[child_entry], "FileList.txt", true).expect("write child");

    let parent_content = fs::read_to_string(&parent_filelist).expect("read parent filelist");
    assert_eq!(
        parent_content
            .lines()
            .filter(|line| line.contains("child") && line.contains("FileList.txt"))
            .count(),
        1
    );
    let parsed_parent =
        parse_filelist(&parent_filelist, &parent, true, true).expect("parse parent filelist");
    assert!(contains_path(&parsed_parent, &child_filelist));
    let _ = fs::remove_dir_all(&top);
}

#[test]
fn ancestor_filelist_propagation_needed_skips_already_referenced_child() {
    let top = test_root("ancestor-propagation-needed-skip");
    let parent = top.join("parent");
    let root = parent.join("child");
    fs::create_dir_all(&root).expect("create child root");
    fs::write(
        parent.join("FileList.txt"),
        format!("./child{}FileList.txt\n", std::path::MAIN_SEPARATOR),
    )
    .expect("write parent filelist");

    assert!(!ancestor_filelist_propagation_needed(&root));
    let _ = fs::remove_dir_all(&top);
}

#[test]
fn normalize_filelist_entry_for_text_compare_collapses_relative_variants() {
    assert_eq!(
        normalize_filelist_entry_for_text_compare("./child\\FileList.txt"),
        Some("child/FileList.txt".to_string())
    );
    assert_eq!(
        normalize_filelist_entry_for_text_compare("\"child/FileList.txt\""),
        Some("child/FileList.txt".to_string())
    );
    assert_eq!(
        normalize_filelist_entry_for_text_compare("child/./nested/../FileList.txt"),
        Some("child/FileList.txt".to_string())
    );
}

#[test]
fn visit_ancestor_directories_stops_when_callback_requests_break() {
    let path = PathBuf::from("/tmp/flistwalker/a/b/c");
    let mut visited = Vec::new();

    visit_ancestor_directories(path.as_path(), |ancestor| {
        visited.push(ancestor.to_path_buf());
        ancestor != Path::new("/tmp/flistwalker/a")
    });

    assert_eq!(
        visited,
        vec![
            PathBuf::from("/tmp/flistwalker/a/b"),
            PathBuf::from("/tmp/flistwalker/a")
        ]
    );
}

#[test]
fn annotate_write_target_error_adds_permission_hint() {
    let path = PathBuf::from(r"C:\FileList.txt");
    let err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "os error 5");
    let msg = annotate_write_target_error(&path, err).to_string();
    assert!(msg.contains("permission denied while writing"));
    assert!(msg.contains(r"C:\FileList.txt"));
    assert!(msg.contains("UAC"));
}

#[test]
fn build_filelist_text_keeps_lexical_relative_for_missing_entry() {
    let root = test_root("filelist-text-missing");
    fs::create_dir_all(&root).expect("create dir");
    let missing = root.join("missing.txt");

    let text = build_filelist_text(&[missing], &root);

    assert_eq!(text, "missing.txt\n");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn build_filelist_text_deduplicates_lexically_equivalent_relative_paths() {
    let root = test_root("filelist-text-lexical-dedup");
    fs::create_dir_all(root.join("a")).expect("create a dir");
    fs::write(root.join("b.txt"), "x").expect("write b");

    let p1 = root.join("a").join("..").join("b.txt");
    let p2 = root.join("b.txt");
    let text = build_filelist_text(&[p1, p2], &root);

    assert_eq!(text, "b.txt\n");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn parse_and_write_filelist() {
    let root = test_root("parse-write");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).expect("create dir");
    fs::write(root.join("src/main.rs"), "fn main(){}").expect("write");

    let out = write_filelist(&root, &[root.join("src/main.rs")], "FileList.txt", true)
        .expect("write filelist");
    let parsed = parse_filelist(&out, &root, true, true).expect("parse filelist");
    assert_eq!(parsed.len(), 1);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn write_filelist_cancellable_stops_before_replacing_output() {
    let root = test_root("write-filelist-cancel");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create dir");
    let existing = root.join("FileList.txt");
    fs::write(&existing, "old.txt\n").expect("write existing filelist");

    let err = write_filelist_cancellable(
        &root,
        &[root.join("src/main.rs")],
        "FileList.txt",
        false,
        &|| true,
    )
    .expect_err("canceled write should fail");

    assert!(err.to_string().contains("canceled"));
    let content = fs::read_to_string(&existing).expect("read existing filelist");
    assert_eq!(content, "old.txt\n");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn parse_filelist_stream_can_be_canceled() {
    let root = test_root("parse-stream-cancel");
    fs::create_dir_all(&root).expect("create dir");
    let file = root.join("a.txt");
    fs::write(&file, "x").expect("write file");
    let filelist = root.join("FileList.txt");
    fs::write(&filelist, "a.txt\n").expect("write filelist");

    let mut visited = 0usize;
    let err = parse_filelist_stream(
        &filelist,
        &root,
        true,
        true,
        || true,
        |_path, _is_dir| {
            visited = visited.saturating_add(1);
        },
    )
    .expect_err("canceled parse should fail");

    assert_eq!(visited, 0);
    assert_eq!(err.to_string(), "superseded");
    let _ = fs::remove_dir_all(&root);
}

fn assert_tc161_rejected_bytes(label: &str, bytes: &[u8]) {
    let root = test_root(label);
    fs::create_dir_all(&root).expect("create dir");
    let filelist = root.join("FileList.txt");
    fs::write(&filelist, bytes).expect("write raw filelist");
    let mut callbacks = 0usize;

    let err = parse_filelist_stream(
        &filelist,
        &root,
        true,
        true,
        || false,
        |_path, _is_dir| callbacks = callbacks.saturating_add(1),
    )
    .expect_err("unsupported FileList bytes must fail");

    let message = err.to_string();
    assert_eq!(callbacks, 0, "{label} emitted a candidate before failure");
    assert!(
        message.contains(&filelist.display().to_string()),
        "{label} error omitted FileList path: {message}"
    );
    assert!(
        message.contains("expected UTF-8 (optional BOM)"),
        "{label} error omitted encoding contract: {message}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc161_utf8_bom_crlf_and_non_ascii_path_match_plain_utf8() {
    let root = test_root("tc161-utf8-bom");
    fs::create_dir_all(&root).expect("create dir");
    let expected = root.join("日本語.txt");
    fs::write(&expected, "x").expect("write non-ASCII file");
    let filelist = root.join("FileList.txt");
    let mut bom_text = vec![0xEF, 0xBB, 0xBF];
    bom_text.extend_from_slice("日本語.txt\r\n".as_bytes());
    fs::write(&filelist, bom_text).expect("write BOM filelist");

    let bom_entries = parse_filelist(&filelist, &root, true, false).expect("parse BOM filelist");
    fs::write(&filelist, "日本語.txt\n").expect("write plain UTF-8 filelist");
    let plain_entries =
        parse_filelist(&filelist, &root, true, false).expect("parse plain filelist");

    assert_eq!(bom_entries, vec![expected]);
    assert_eq!(bom_entries, plain_entries);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc161_bom_only_filelist_is_empty() {
    let root = test_root("tc161-bom-only");
    fs::create_dir_all(&root).expect("create dir");
    let filelist = root.join("FileList.txt");
    fs::write(&filelist, [0xEF, 0xBB, 0xBF]).expect("write BOM-only filelist");

    let entries = parse_filelist(&filelist, &root, true, true).expect("parse BOM-only filelist");

    assert!(entries.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc161_rejects_utf16_legacy_invalid_utf8_nul_and_truncated_sequences() {
    assert_tc161_rejected_bytes("tc161-utf16le", &[0xFF, 0xFE, b'a', 0x00, b'\n', 0x00]);
    assert_tc161_rejected_bytes("tc161-utf16be", &[0xFE, 0xFF, 0x00, b'a', 0x00, b'\n']);
    assert_tc161_rejected_bytes("tc161-shift-jis", &[0x93, 0xFA, 0x96, 0x7B, b'\n']);
    assert_tc161_rejected_bytes("tc161-nul", b"valid.txt\0hidden.txt\n");
    assert_tc161_rejected_bytes(
        "tc161-truncated",
        &[b'a', b'.', b't', b'x', b't', 0xE3, 0x81],
    );
}

#[test]
fn tc161_stable_invalid_root_emits_no_valid_prefix_and_reports_byte_offset() {
    let root = test_root("tc161-invalid-after-valid-prefix");
    fs::create_dir_all(&root).expect("create dir");
    let filelist = root.join("FileList.txt");
    fs::write(&filelist, b"valid.txt\n\xFF\n").expect("write invalid suffix");
    let mut callbacks = 0usize;

    let err = parse_filelist_stream(
        &filelist,
        &root,
        true,
        true,
        || false,
        |_path, _is_dir| callbacks = callbacks.saturating_add(1),
    )
    .expect_err("stable invalid suffix must fail before callbacks");

    assert_eq!(callbacks, 0);
    assert!(err.to_string().contains("byte 10"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc161_rejects_over_one_mib_line_before_callback() {
    const MAX_LINE_BYTES: usize = 1024 * 1024;
    let mut bytes = vec![b'a'; MAX_LINE_BYTES + 1];
    bytes.push(b'\n');
    let root = test_root("tc161-line-limit");
    fs::create_dir_all(&root).expect("create dir");
    let filelist = root.join("FileList.txt");
    fs::write(&filelist, bytes).expect("write oversized line");
    let mut callbacks = 0usize;

    let err = parse_filelist_stream(
        &filelist,
        &root,
        true,
        true,
        || false,
        |_path, _is_dir| callbacks = callbacks.saturating_add(1),
    )
    .expect_err("oversized line must fail");

    assert_eq!(callbacks, 0);
    assert!(err.to_string().contains("1 MiB"));
    assert!(err.to_string().contains(&filelist.display().to_string()));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc161_long_line_cancellation_occurs_before_callback() {
    let root = test_root("tc161-long-line-cancel");
    fs::create_dir_all(&root).expect("create dir");
    let filelist = root.join("FileList.txt");
    fs::write(&filelist, vec![b'a'; 320 * 1024]).expect("write long line");
    let checks = AtomicUsize::new(0);
    let mut callbacks = 0usize;

    let err = parse_filelist_stream(
        &filelist,
        &root,
        true,
        true,
        || checks.fetch_add(1, Ordering::Relaxed) >= 4,
        |_path, _is_dir| callbacks = callbacks.saturating_add(1),
    )
    .expect_err("long-line validation must observe cancellation");

    assert_eq!(callbacks, 0);
    assert!(checks.load(Ordering::Relaxed) >= 5);
    assert_eq!(err.to_string(), "superseded");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc161_invalid_nested_filelist_preserves_parent_entries() {
    let root = test_root("tc161-invalid-nested");
    let child = root.join("child");
    fs::create_dir_all(&child).expect("create child");
    let child_filelist = child.join("FileList.txt");
    fs::write(&child_filelist, [0xFF, 0xFE, b'x', 0x00]).expect("write invalid child");
    let retained = child.join("from-parent.txt");
    let mut entries = vec![retained.clone(), child_filelist.clone()];
    let before = entries.clone();

    let err = apply_filelist_hierarchy_overrides(
        &root.join("FileList.txt"),
        &root,
        &mut entries,
        true,
        true,
        || false,
    )
    .expect_err("invalid child FileList must fail");

    assert_eq!(entries, before);
    assert!(err.to_string().contains("expected UTF-8 (optional BOM)"));
    assert!(err
        .to_string()
        .contains(&child_filelist.display().to_string()));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn tc161_bom_ancestor_dedupes_existing_child_reference() {
    let top = test_root("tc161-bom-ancestor-dedupe");
    let parent = top.join("parent");
    let root = parent.join("child");
    fs::create_dir_all(&root).expect("create child root");
    let parent_filelist = parent.join("FileList.txt");
    let mut parent_bytes = vec![0xEF, 0xBB, 0xBF];
    parent_bytes.extend_from_slice(b"child/FileList.txt\n");
    fs::write(&parent_filelist, &parent_bytes).expect("write BOM parent");

    write_filelist(&root, &[], "FileList.txt", true).expect("write child filelist");

    let parent_content = fs::read(&parent_filelist).expect("read parent bytes");
    assert_eq!(parent_content, parent_bytes);
    let decoded = std::str::from_utf8(&parent_content).expect("parent remains UTF-8");
    assert_eq!(decoded.matches("child/FileList.txt").count(), 1);
    let _ = fs::remove_dir_all(&top);
}

#[test]
fn tc161_nul_ancestor_is_not_rewritten() {
    let top = test_root("tc161-nul-ancestor");
    let parent = top.join("parent");
    let root = parent.join("child");
    fs::create_dir_all(&root).expect("create child root");
    let parent_filelist = parent.join("FileList.txt");
    let invalid_parent = b"keep.txt\0hidden.txt\n";
    fs::write(&parent_filelist, invalid_parent).expect("write NUL parent");

    write_filelist(&root, &[], "FileList.txt", true).expect("child write remains successful");

    assert_eq!(
        fs::read(&parent_filelist).expect("read parent"),
        invalid_parent
    );
    let _ = fs::remove_dir_all(&top);
}

#[test]
fn tc161_writer_round_trip_is_utf8_without_bom() {
    let root = test_root("tc161-writer-roundtrip");
    fs::create_dir_all(&root).expect("create root");
    let expected = root.join("日本語.txt");
    fs::write(&expected, "x").expect("write non-ASCII entry");

    let filelist = write_filelist(
        &root,
        std::slice::from_ref(&expected),
        "FileList.txt",
        false,
    )
    .expect("write FileList");
    let bytes = fs::read(&filelist).expect("read FileList bytes");
    assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
    assert!(std::str::from_utf8(&bytes).is_ok());
    assert_eq!(
        parse_filelist(&filelist, &root, true, false).expect("round-trip parse"),
        vec![expected]
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn parse_filelist_accepts_backslash_relative_path() {
    let root = test_root("parse-backslash");
    let nested = root.join("nested");
    fs::create_dir_all(&nested).expect("create dir");
    let file = nested.join("item.txt");
    fs::write(&file, "x").expect("write file");
    let filelist = root.join("FileList.txt");
    fs::write(&filelist, "nested\\item.txt\n").expect("write filelist");

    let parsed = parse_filelist(&filelist, &root, true, false).expect("parse filelist");
    assert_eq!(parsed, vec![file]);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn parse_filelist_accepts_quoted_path() {
    let root = test_root("parse-quoted");
    fs::create_dir_all(&root).expect("create dir");
    let file = root.join("quoted.txt");
    fs::write(&file, "x").expect("write file");
    let filelist = root.join("FileList.txt");
    fs::write(&filelist, "\"quoted.txt\"\n").expect("write filelist");

    let parsed = parse_filelist(&filelist, &root, true, false).expect("parse filelist");
    assert_eq!(parsed, vec![file]);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn parse_filelist_stream_returns_unknown_kind_when_both_types_are_enabled() {
    let root = test_root("parse-stream-kind-unknown");
    fs::create_dir_all(&root).expect("create dir");
    let file = root.join("a.txt");
    fs::write(&file, "x").expect("write file");
    let filelist = root.join("FileList.txt");
    fs::write(&filelist, "a.txt\n").expect("write filelist");

    let mut kinds = Vec::new();
    parse_filelist_stream(
        &filelist,
        &root,
        true,
        true,
        || false,
        |_path, is_dir| {
            kinds.push(is_dir);
        },
    )
    .expect("parse filelist");

    assert_eq!(kinds, vec![None]);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn regression_parse_filelist_stream_prefers_platform_candidate_without_exists_probe() {
    let root = test_root("parse-stream-platform-candidate");
    fs::create_dir_all(root.join("nested")).expect("create dir");
    let filelist = root.join("FileList.txt");
    fs::write(&filelist, "nested\\item.txt\n").expect("write filelist");

    let mut entries = Vec::new();
    parse_filelist_stream(
        &filelist,
        &root,
        true,
        true,
        || false,
        |path, is_dir| {
            entries.push((path, is_dir));
        },
    )
    .expect("parse filelist");

    assert_eq!(
        entries,
        vec![(expected_backslash_relative_path(&root), None)]
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn parse_filelist_stream_returns_known_kind_when_filter_requires_type() {
    let root = test_root("parse-stream-kind-known");
    fs::create_dir_all(root.join("d")).expect("create dir");
    let file = root.join("a.txt");
    fs::write(&file, "x").expect("write file");
    let filelist = root.join("FileList.txt");
    fs::write(&filelist, "a.txt\nd\n").expect("write filelist");

    let mut entries = Vec::new();
    parse_filelist_stream(
        &filelist,
        &root,
        false,
        true,
        || false,
        |path, is_dir| {
            entries.push((path, is_dir));
        },
    )
    .expect("parse filelist");

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, root.join("d"));
    assert_eq!(entries[0].1, Some(true));
    let _ = fs::remove_dir_all(&root);
}

#[test]
#[cfg(not(windows))]
fn windows_path_to_wsl_converts_drive_path() {
    let converted = windows_path_to_wsl(r"C:\Users\alice\work\file.txt");
    assert_eq!(
        converted,
        Some(PathBuf::from("/mnt/c/Users/alice/work/file.txt"))
    );
}

#[test]
fn find_filelist_in_first_level_only_checks_root() {
    let root = test_root("find-first-level");
    let child = root.join("child");
    fs::create_dir_all(&child).expect("create child");
    fs::write(child.join("filelist.txt"), "a.txt\n").expect("write filelist");

    let found = find_filelist_in_first_level(&root);
    assert!(found.is_none());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn build_index_overrides_subtree_with_newer_nested_filelist() {
    let root = test_root("nested-filelist-newer");
    let child = root.join("child");
    fs::create_dir_all(&child).expect("create child");
    let keep = root.join("keep.txt");
    let child_old = child.join("old.txt");
    let child_new = child.join("new.txt");
    fs::write(&keep, "x").expect("write keep");
    fs::write(&child_old, "x").expect("write old");
    fs::write(&child_new, "x").expect("write new");
    fs::write(
        root.join("FileList.txt"),
        "keep.txt\nchild\nchild/old.txt\nchild/filelist.txt\n",
    )
    .expect("write root filelist");
    sleep_for_timestamp_tick();
    fs::write(child.join("filelist.txt"), "new.txt\n").expect("write child filelist");

    let out = build_index_with_metadata(&root, true, true, true).expect("build index");
    assert_filelist_source_matches(&out.source, &root.join("FileList.txt"));
    assert!(contains_path(&out.entries, &keep));
    assert!(contains_path(&out.entries, &child_new));
    assert!(!contains_path(&out.entries, &child_old));
    assert!(!contains_path(&out.entries, &child));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn build_index_ignores_older_nested_filelist() {
    let root = test_root("nested-filelist-older");
    let child = root.join("child");
    fs::create_dir_all(&child).expect("create child");
    let child_old = child.join("old.txt");
    let child_new = child.join("new.txt");
    fs::write(&child_old, "x").expect("write old");
    fs::write(&child_new, "x").expect("write new");
    fs::write(child.join("filelist.txt"), "new.txt\n").expect("write child filelist");
    sleep_for_timestamp_tick();
    fs::write(root.join("FileList.txt"), "child/old.txt\n").expect("write root filelist");

    let out = build_index_with_metadata(&root, true, true, false).expect("build index");
    assert_filelist_source_matches(&out.source, &root.join("FileList.txt"));
    assert!(contains_path(&out.entries, &child_old));
    assert!(!contains_path(&out.entries, &child_new));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn build_index_applies_newest_filelist_per_depth() {
    let root = test_root("nested-filelist-depth");
    let child = root.join("child");
    let grand = child.join("grand");
    fs::create_dir_all(&grand).expect("create dirs");
    let top = root.join("top.txt");
    let root_only = child.join("child_from_root.txt");
    let child_only = child.join("child_from_child.txt");
    let grand_child = grand.join("grand_from_child.txt");
    let grand_only = grand.join("grand_from_grand.txt");
    for file in [&top, &root_only, &child_only, &grand_child, &grand_only] {
        fs::write(file, "x").expect("write file");
    }

    fs::write(
        root.join("FileList.txt"),
        "top.txt\nchild/child_from_root.txt\nchild/grand/grand_from_root.txt\nchild/filelist.txt\n",
    )
    .expect("write root filelist");
    sleep_for_timestamp_tick();
    fs::write(
        child.join("filelist.txt"),
        "child_from_child.txt\ngrand/grand_from_child.txt\ngrand/filelist.txt\n",
    )
    .expect("write child filelist");
    sleep_for_timestamp_tick();
    fs::write(grand.join("filelist.txt"), "grand_from_grand.txt\n").expect("write grand filelist");

    let out = build_index_with_metadata(&root, true, true, false).expect("build index");
    assert!(contains_path(&out.entries, &top));
    assert!(contains_path(&out.entries, &child_only));
    assert!(contains_path(&out.entries, &grand_only));
    assert!(!contains_path(&out.entries, &root_only));
    assert!(!contains_path(&out.entries, &grand_child));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn apply_overrides_can_cancel_during_nested_filelist_parse() {
    let root = test_root("nested-filelist-cancel");
    let child = root.join("child");
    fs::create_dir_all(&child).expect("create child");
    fs::write(child.join("a.txt"), "x").expect("write child file");
    fs::write(child.join("filelist.txt"), "a.txt\n").expect("write child filelist");

    let mut entries = vec![child.join("filelist.txt")];
    let err = apply_filelist_hierarchy_overrides(
        &root.join("FileList.txt"),
        &root,
        &mut entries,
        true,
        true,
        || true,
    )
    .expect_err("override should be cancelable");

    assert!(err.to_string().contains("superseded"));
    let _ = fs::remove_dir_all(&root);
}
