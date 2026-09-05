use super::*;
use std::collections::HashSet;

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn fixture() -> Fixture {
    static NEXT_FIXTURE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "flistwalker-follow-{}-{}-{}",
        std::process::id(),
        NEXT_FIXTURE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(path.join("root/real/sub")).unwrap();
    fs::create_dir_all(path.join("outside/deep")).unwrap();
    fs::write(path.join("root/real/sub/in.txt"), "in").unwrap();
    fs::write(path.join("outside/deep/out.txt"), "out").unwrap();
    Fixture(path)
}
#[cfg(unix)]
fn directory_link(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}
#[cfg(windows)]
fn directory_link(target: &Path, link: &Path) {
    let output = std::process::Command::new("cmd.exe")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .expect("create junction fixture");
    assert!(
        output.status.success(),
        "junction fixture: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn collect(
    root: &Path,
    workers: usize,
    follow: bool,
    depth: MaxDepth,
    files: bool,
    dirs: bool,
) -> (HashSet<PathBuf>, AdaptiveWalkerMetrics) {
    let mut paths = HashSet::new();
    let metrics = walk_adaptive_with_options(
        root,
        workers,
        workers,
        files,
        dirs,
        WalkOptions {
            max_depth: depth,
            follow_links: follow,
        },
        |entry| {
            assert!(paths.len() < 100, "cycle did not terminate");
            if super::super::classify_walker_entry(&entry.path, entry.file_type, files, dirs)
                .is_some()
            {
                assert!(paths.insert(entry.path), "duplicate lexical path");
            }
            true
        },
        || false,
    );
    (paths, metrics)
}
#[test]
fn follow_links_preserves_aliases_prunes_ancestor_cycles_and_respects_depth() {
    let fixture = fixture();
    let root = fixture.0.join("root");
    directory_link(&root.join("real"), &root.join("alias-a"));
    directory_link(&root.join("real"), &root.join("alias-b"));
    directory_link(&fixture.0.join("outside"), &root.join("external"));
    // Native separators keep cmd's mklink from parsing a path suffix as a switch.
    directory_link(&root, &root.join("real").join("back"));
    directory_link(&root.join("real"), &root.join("real").join("self"));
    let linked_root = fixture.0.join("linked-root");
    directory_link(&root, &linked_root);
    for workers in [1, 4] {
        let (from_linked_root, _) = collect(
            &linked_root,
            workers,
            true,
            MaxDepth::unlimited(),
            true,
            true,
        );
        assert!(from_linked_root.contains(&linked_root.join("external/deep/out.txt")));
        let (default, _) = collect(&root, workers, false, MaxDepth::unlimited(), true, true);
        assert!(default.contains(&root.join("alias-a")));
        assert!(!default.contains(&root.join("alias-a/sub/in.txt")));
        let (followed, metrics) = collect(&root, workers, true, MaxDepth::unlimited(), true, true);
        for relative in [
            "real/sub/in.txt",
            "alias-a/sub/in.txt",
            "alias-b/sub/in.txt",
            "external/deep/out.txt",
        ] {
            assert!(
                followed.contains(&root.join(relative)),
                "missing {relative}, workers={workers}"
            );
        }
        assert!(!followed.contains(&root.join("real/back/real")));
        assert!(!followed.contains(&root.join("real/self/sub")));
        assert_eq!(metrics.read_dir_errors, 0);
        let (limited, _) = collect(
            &root,
            workers,
            true,
            MaxDepth::limited(2).unwrap(),
            true,
            true,
        );
        assert!(limited.contains(&root.join("alias-a/sub")));
        assert!(!limited.contains(&root.join("alias-a/sub/in.txt")));
        let (files, _) = collect(&root, workers, true, MaxDepth::unlimited(), true, false);
        assert_eq!(files.len(), 4);
        let (dirs, _) = collect(&root, workers, true, MaxDepth::unlimited(), false, true);
        assert!(dirs.contains(&root.join("external/deep")));
        assert!(!dirs.contains(&root.join("external/deep/out.txt")));
    }
}
#[cfg(unix)]
#[test]
fn follow_links_handles_broken_file_and_relative_links_and_linked_root() {
    let fixture = fixture();
    let root = fixture.0.join("root");
    directory_link(Path::new("../outside"), &root.join("relative"));
    directory_link(Path::new("missing"), &root.join("broken"));
    directory_link(Path::new("cycle-b"), &root.join("cycle-a"));
    directory_link(Path::new("cycle-a"), &root.join("cycle-b"));
    std::os::unix::fs::symlink("real/sub/in.txt", root.join("file-link")).unwrap();
    let linked_root = fixture.0.join("linked-root");
    directory_link(&root, &linked_root);
    for workers in [1, 4] {
        let (paths, _) = collect(
            &linked_root,
            workers,
            true,
            MaxDepth::unlimited(),
            true,
            true,
        );
        assert!(paths.contains(&linked_root.join("relative/deep/out.txt")));
        assert!(paths.contains(&linked_root.join("broken")));
        assert!(paths.contains(&linked_root.join("file-link")));
    }
}
#[test]
fn follow_links_cancellation_and_callback_limit_settle() {
    let fixture = fixture();
    let root = fixture.0.join("root");
    directory_link(&root, &root.join("back"));
    for workers in [1, 4] {
        let count = std::cell::Cell::new(0);
        walk_adaptive_with_options(
            &root,
            workers,
            workers,
            true,
            true,
            WalkOptions {
                follow_links: true,
                ..Default::default()
            },
            |_| {
                count.set(count.get() + 1);
                count.get() < 2
            },
            || false,
        );
        assert_eq!(count.get(), 2);
        let metrics = walk_adaptive_with_options(
            &root,
            workers,
            workers,
            true,
            true,
            WalkOptions {
                follow_links: true,
                ..Default::default()
            },
            |_| panic!("canceled result"),
            || true,
        );
        assert_eq!(metrics.dirs_read, 0);
    }
}

#[test]
fn follow_links_ancestry_drop_is_iterative_and_keeps_shared_parents_alive() {
    let mut ancestry = None;
    for _ in 0..50_000 {
        ancestry = Some(Arc::new(DirectoryAncestor {
            resolved: PathBuf::new(),
            parent: ancestry,
        }));
    }
    let alias = ancestry.clone();
    drop(ancestry);
    assert!(alias.is_some());
    drop(alias);
}
