use super::{IndexResponse, IndexResponseSink};
use std::path::{Component, Path, PathBuf};

/// Keep index internals on the resolved root while exposing the user's lexical
/// root to every GUI path consumer. Projection is lexical and runs in the worker;
/// resolving each result here would lose descendant link aliases and add I/O.
pub(super) struct RootProjectionSink<'a, S> {
    inner: &'a S,
    resolved_root: PathBuf,
    requested_root: &'a Path,
}

impl<'a, S> RootProjectionSink<'a, S> {
    pub(super) fn new(inner: &'a S, resolved_root: &Path, requested_root: &'a Path) -> Self {
        #[cfg(windows)]
        let resolved_root = crate::path_utils::windows_non_verbatim_path(resolved_root)
            .unwrap_or_else(|| resolved_root.to_path_buf());
        #[cfg(not(windows))]
        let resolved_root = resolved_root.to_path_buf();
        Self {
            inner,
            resolved_root,
            requested_root,
        }
    }

    fn project(&self, path: &Path) -> Option<PathBuf> {
        if self.requested_root == self.resolved_root {
            return None;
        }
        #[cfg(windows)]
        let normalized = crate::path_utils::windows_non_verbatim_path(path);
        #[cfg(windows)]
        let path = normalized.as_deref().unwrap_or(path);
        let relative = relative_to(path, &self.resolved_root)?;
        // Parent components can escape a linked root into a different parent.
        // Leave such absolute FileList paths intact rather than change targets.
        if relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
        {
            return None;
        }
        Some(self.requested_root.join(relative))
    }
}

impl<S: IndexResponseSink> IndexResponseSink for RootProjectionSink<'_, S> {
    fn send(&self, mut response: IndexResponse) -> Result<(), ()> {
        if let IndexResponse::Batch { entries, .. } | IndexResponse::ReplaceAll { entries, .. } =
            &mut response
        {
            for entry in entries {
                if let Some(projected) = self.project(&entry.path) {
                    entry.path = projected;
                }
            }
        }
        self.inner.send(response)
    }
}

#[cfg(not(windows))]
fn relative_to<'a>(path: &'a Path, root: &Path) -> Option<&'a Path> {
    path.strip_prefix(root).ok()
}

#[cfg(windows)]
fn relative_to<'a>(path: &'a Path, root: &Path) -> Option<&'a Path> {
    use std::os::windows::ffi::OsStrExt;
    fn fold_ascii(unit: u16) -> u16 {
        if (u16::from(b'A')..=u16::from(b'Z')).contains(&unit) {
            unit + 32
        } else {
            unit
        }
    }
    let mut parts = path.components();
    for root_part in root.components() {
        let path_part = parts.next()?;
        if !root_part
            .as_os_str()
            .encode_wide()
            .map(fold_ascii)
            .eq(path_part.as_os_str().encode_wide().map(fold_ascii))
        {
            return None;
        }
    }
    Some(parts.as_path())
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::RootProjectionSink;
    use crate::path_utils::display_path_with_mode;

    #[test]
    fn root_projection_preserves_aliases_and_unrelated_absolute_paths_without_io() {
        let resolved = Path::new("physical/root");
        let requested = Path::new("chosen/link");
        let sink = RootProjectionSink::new(&(), resolved, requested);
        for child in ["資料/entry.txt", "alias-a/entry.txt", "alias-b/entry.txt"] {
            assert_eq!(
                sink.project(&resolved.join(child)),
                Some(requested.join(child))
            );
        }
        for unchanged in [
            "physical/root-other/entry.txt",
            "outside/entry.txt",
            "physical/root/../outside.txt",
        ] {
            assert_eq!(sink.project(Path::new(unchanged)), None);
        }
    }

    #[test]
    fn root_projection_rewrites_batches_and_replacements_but_preserves_identity_and_source() {
        let (tx, rx) = mpsc::channel();
        let sink = RootProjectionSink::new(&tx, Path::new("physical"), Path::new("linked"));
        for replace in [false, true] {
            let entries = vec![IndexEntry {
                path: PathBuf::from("physical/alias/entry.txt"),
                kind: EntryKind::link_unknown(),
                kind_known: false,
            }];
            sink.send(if replace {
                IndexResponse::ReplaceAll {
                    request_id: 23,
                    entries,
                }
            } else {
                IndexResponse::Batch {
                    request_id: 23,
                    entries,
                }
            })
            .unwrap();
            match rx.recv().unwrap() {
                IndexResponse::Batch {
                    request_id,
                    entries,
                }
                | IndexResponse::ReplaceAll {
                    request_id,
                    entries,
                } => {
                    assert_eq!(request_id, 23);
                    assert_eq!(entries[0].path, Path::new("linked/alias/entry.txt"));
                    assert_eq!(entries[0].kind, EntryKind::link_unknown());
                    assert!(!entries[0].kind_known);
                }
                _ => panic!("unexpected response"),
            }
        }
        let source = IndexSource::FileList(PathBuf::from("physical/FileList.txt"));
        sink.send(IndexResponse::Started {
            request_id: 23,
            source: source.clone(),
        })
        .unwrap();
        match rx.recv().unwrap() {
            IndexResponse::Started {
                request_id: 23,
                source: actual,
            } => assert_eq!(actual, source),
            _ => panic!("source identity changed"),
        }
    }

    #[test]
    #[cfg(windows)]
    fn root_projection_accepts_windows_drive_and_unc_namespaces_without_lossy_conversion() {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;
        for (resolved, path, requested) in [
            (
                r"\\?\C:\Physical",
                r"c:\physical\資料\entry.txt",
                r"C:\Linked",
            ),
            (
                r"C:\Physical",
                r"\\?\C:\PHYSICAL\資料\entry.txt",
                r"C:\Linked",
            ),
            (
                r"\\?\UNC\server\share\Physical",
                r"\\server\SHARE\physical\資料\entry.txt",
                r"C:\Linked",
            ),
        ] {
            let sink = RootProjectionSink::new(&(), Path::new(resolved), Path::new(requested));
            assert_eq!(
                sink.project(Path::new(path)),
                Some(Path::new(requested).join("資料").join("entry.txt"))
            );
        }
        let sink =
            RootProjectionSink::new(&(), Path::new(r"\\?\C:\Physical"), Path::new(r"C:\Linked"));
        let unpaired_surrogate = OsString::from_wide(&[0xd800]);
        assert_eq!(
            sink.project(&Path::new(r"C:\Physical").join(&unpaired_surrogate)),
            Some(Path::new(r"C:\Linked").join(unpaired_surrogate))
        );
        assert_eq!(sink.project(Path::new(r"C:\PhysicalOther\entry.txt")), None);
    }

    fn linked_root_results(use_filelist: bool) {
        let fixture = std::env::temp_dir().join(format!(
            "flistwalker-linked-root-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let physical = fixture.join("physical");
        let linked = fixture.join("linked");
        std::fs::create_dir_all(physical.join("資料")).unwrap();
        std::fs::write(physical.join("資料/entry.txt"), "test").unwrap();
        let outside = fixture.join("outside.txt");
        std::fs::write(&outside, "outside").unwrap();
        if use_filelist {
            std::fs::create_dir_all(physical.join("child")).unwrap();
            std::fs::write(physical.join("child/new.txt"), "new").unwrap();
            std::fs::write(physical.join("child/filelist.txt"), "new.txt\n").unwrap();
            std::fs::write(
                physical.join("FileList.txt"),
                format!(
                    "資料/entry.txt\n{}\nchild\nchild/old.txt\nchild/filelist.txt\n",
                    outside.display()
                ),
            )
            .unwrap();
            std::fs::File::open(physical.join("FileList.txt"))
                .unwrap()
                .set_modified(std::time::SystemTime::now() - Duration::from_secs(2))
                .unwrap();
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&physical, &linked).unwrap();
        #[cfg(windows)]
        assert!(std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&linked)
            .arg(&physical)
            .output()
            .expect("create native junction")
            .status
            .success());

        let shutdown = Arc::new(AtomicBool::new(false));
        let latest = Arc::new(Mutex::new(HashMap::from([(7, 1)])));
        let (tx, _rx, mailboxes, handles) = spawn_index_worker(Arc::clone(&shutdown), latest);
        let mailbox = Arc::new(IndexResponseMailbox::new());
        mailboxes.lock().unwrap().insert(1, Arc::clone(&mailbox));
        tx.send(IndexRequest {
            request_id: 1,
            tab_id: 7,
            root: linked.clone(),
            use_filelist,
            include_files: true,
            include_dirs: true,
            max_depth: MaxDepth::unlimited(),
            follow_links: false,
        })
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut paths = Vec::new();
        let mut error = None;
        let mut replaced = false;
        loop {
            match mailbox.try_recv() {
                Some(IndexResponse::Batch { entries, .. }) => {
                    paths.extend(entries.into_iter().map(|entry| entry.path));
                }
                Some(IndexResponse::ReplaceAll { entries, .. }) => {
                    replaced = true;
                    paths = entries.into_iter().map(|entry| entry.path).collect();
                }
                Some(IndexResponse::Finished { .. }) => break,
                Some(IndexResponse::Failed { error: message, .. }) => {
                    error = Some(message);
                    break;
                }
                Some(IndexResponse::Canceled { .. }) => {
                    error = Some("unexpected cancellation".into());
                    break;
                }
                _ => {}
            }
            if Instant::now() >= deadline {
                error = Some("index timed out".into());
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        shutdown.store(true, Ordering::Relaxed);
        drop(tx);
        for handle in handles {
            handle.join().unwrap();
        }
        #[cfg(windows)]
        std::fs::remove_dir(&linked).unwrap();
        std::fs::remove_dir_all(&fixture).unwrap();
        assert!(error.is_none(), "{error:?}");
        let expected = linked.join("資料/entry.txt");
        assert!(
            paths.contains(&expected),
            "lexical linked Root lost: {paths:?}"
        );
        assert_eq!(
            display_path_with_mode(&expected, &linked, true),
            std::path::Path::new("資料")
                .join("entry.txt")
                .to_string_lossy()
        );
        if use_filelist {
            assert!(
                replaced,
                "nested FileList must exercise ReplaceAll projection"
            );
            assert!(paths.contains(&linked.join("child/new.txt")));
            assert!(!paths.iter().any(|path| path.ends_with("old.txt")));
            assert!(
                paths.contains(&outside),
                "outside absolute FileList entry changed"
            );
        }
    }

    #[test]
    #[cfg(any(unix, windows))]
    fn linked_root_walker_results_use_relative_display() {
        linked_root_results(false);
    }

    #[test]
    #[cfg(any(unix, windows))]
    fn linked_root_filelist_results_use_relative_display() {
        linked_root_results(true);
    }
}
