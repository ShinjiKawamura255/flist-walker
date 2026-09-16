use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static TMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct AtomicWriteAfterReplaceError {
    source: std::io::Error,
}

impl fmt::Display for AtomicWriteAfterReplaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "destination was replaced but durability sync failed: {}",
            self.source
        )
    }
}

impl std::error::Error for AtomicWriteAfterReplaceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

pub(crate) fn atomic_write_replaced_destination(error: &std::io::Error) -> bool {
    error
        .get_ref()
        .and_then(|source| source.downcast_ref::<AtomicWriteAfterReplaceError>())
        .is_some()
}

fn post_replace_sync_error(error: std::io::Error) -> std::io::Error {
    std::io::Error::new(error.kind(), AtomicWriteAfterReplaceError { source: error })
}

pub struct SidecarFileLock {
    // Keeping this handle alive keeps the OS advisory lock alive. The OS
    // releases it when a process exits, including after a crash.
    _file: File,
}

pub fn acquire_sidecar_lock(path: &Path, timeout: Duration) -> std::io::Result<SidecarFileLock> {
    let lock_path = sidecar_lock_path(path);
    let parent = lock_path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let deadline = Instant::now() + timeout;
    loop {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;
        match file.try_lock() {
            Ok(()) => return Ok(SidecarFileLock { _file: file }),
            Err(std::fs::TryLockError::WouldBlock) => {
                if Instant::now() >= deadline {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        format!("timed out waiting for sidecar lock {}", lock_path.display()),
                    ));
                }
                thread::sleep(Duration::from_millis(5));
            }
            Err(std::fs::TryLockError::Error(err)) => return Err(err),
        }
    }
}

pub fn sidecar_lock_path(path: &Path) -> PathBuf {
    let mut lock_path = path.as_os_str().to_os_string();
    lock_path.push(".lock");
    PathBuf::from(lock_path)
}

pub fn write_text_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    write_bytes_atomic(path, text.as_bytes())
}

pub fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    write_bytes_atomic_with_metadata(path, bytes, AtomicWriteMetadata::default())
}

#[derive(Clone, Default)]
pub(crate) struct AtomicWriteMetadata {
    pub permissions: Option<fs::Permissions>,
    pub accessed: Option<SystemTime>,
    pub modified: Option<SystemTime>,
}

pub(crate) fn write_bytes_atomic_with_metadata(
    path: &Path,
    bytes: &[u8],
    metadata: AtomicWriteMetadata,
) -> std::io::Result<()> {
    write_bytes_atomic_inner(path, bytes, metadata, |_| Ok(()), sync_directory)
}

#[cfg(test)]
pub(crate) fn write_bytes_atomic_with_sync_for_test<S>(
    path: &Path,
    bytes: &[u8],
    sync_directory: S,
) -> std::io::Result<()>
where
    S: FnMut(&Path) -> std::io::Result<()>,
{
    write_bytes_atomic_inner(
        path,
        bytes,
        AtomicWriteMetadata::default(),
        |_| Ok(()),
        sync_directory,
    )
}

fn write_bytes_atomic_inner<B, S>(
    path: &Path,
    bytes: &[u8],
    metadata: AtomicWriteMetadata,
    mut before_replace: B,
    mut sync_directory: S,
) -> std::io::Result<()>
where
    B: FnMut(&Path) -> std::io::Result<()>,
    S: FnMut(&Path) -> std::io::Result<()>,
{
    let parent = parent_directory(path);
    let directory_sync_targets = directory_sync_targets(parent)?;
    fs::create_dir_all(parent)?;
    let tmp = build_temp_path(path);
    let result = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(bytes)?;
        if let Some(permissions) = metadata.permissions {
            file.set_permissions(permissions)?;
        }
        if metadata.accessed.is_some() || metadata.modified.is_some() {
            let mut times = fs::FileTimes::new();
            if let Some(accessed) = metadata.accessed {
                times = times.set_accessed(accessed);
            }
            if let Some(modified) = metadata.modified {
                times = times.set_modified(modified);
            }
            file.set_times(times)?;
        }
        file.sync_all()?;
        before_replace(&tmp)?;
        drop(file);
        replace_file(&tmp, path)?;
        for directory in &directory_sync_targets {
            sync_directory(directory).map_err(post_replace_sync_error)?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn directory_sync_targets(parent: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut missing = Vec::new();
    let mut current = parent;
    while !current.try_exists()? {
        missing.push(current.to_path_buf());
        let next = parent_directory(current);
        if next == current {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("no existing ancestor for {}", parent.display()),
            ));
        }
        current = next;
    }
    let mut targets = vec![parent.to_path_buf()];
    for directory in missing {
        let ancestor = parent_directory(&directory);
        if targets.last().is_none_or(|last| last != ancestor) {
            targets.push(ancestor.to_path_buf());
        }
    }
    Ok(targets)
}

fn parent_directory(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn build_temp_path(path: &Path) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let seq = TMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("tmp");
    let parent = parent_directory(path);
    parent.join(format!(".{filename}.{pid}.{now}.{seq}.tmp"))
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> std::io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::rename(src, dst)
}

#[cfg(windows)]
fn replace_file(src: &Path, dst: &Path) -> std::io::Result<()> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(
            lp_existing_file_name: *const u16,
            lp_new_file_name: *const u16,
            dw_flags: u32,
        ) -> i32;
    }

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    fn wide_null(text: &OsStr) -> Vec<u16> {
        text.encode_wide().chain(std::iter::once(0)).collect()
    }

    let src_wide = wide_null(src.as_os_str());
    let dst_wide = wide_null(dst.as_os_str());
    let ok = unsafe {
        MoveFileExW(
            src_wide.as_ptr(),
            dst_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::process::Command;

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("fff-rs-fs-atomic-{name}-{nonce}"))
    }

    #[test]
    fn write_text_atomic_creates_parent_directories() {
        let root = test_root("create-parent");
        let path = root.join("nested/state.json");

        write_text_atomic(&path, "hello").expect("write");

        assert_eq!(fs::read_to_string(&path).expect("read"), "hello");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn write_text_atomic_overwrites_existing_file() {
        let root = test_root("overwrite");
        fs::create_dir_all(&root).expect("create dir");
        let path = root.join("state.json");
        fs::write(&path, "old").expect("write old");

        write_text_atomic(&path, "new").expect("write");

        assert_eq!(fs::read_to_string(&path).expect("read"), "new");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn atomic_write_prepares_metadata_and_syncs_before_replace() {
        let root = test_root("metadata-before-replace");
        fs::create_dir_all(&root).expect("create dir");
        let path = root.join("state.json");
        fs::write(&path, "old").expect("write old");
        let expected_modified = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let observed = RefCell::new(false);

        let err = write_bytes_atomic_inner(
            &path,
            b"new",
            AtomicWriteMetadata {
                permissions: None,
                accessed: None,
                modified: Some(expected_modified),
            },
            |tmp| {
                assert_eq!(fs::read(tmp).expect("read prepared temp"), b"new");
                assert_eq!(
                    fs::metadata(tmp)
                        .expect("temp metadata")
                        .modified()
                        .expect("mtime"),
                    expected_modified
                );
                *observed.borrow_mut() = true;
                Err(std::io::Error::other("stop before replace"))
            },
            |_| Ok(()),
        )
        .expect_err("injected pre-replace stop");

        assert_eq!(err.kind(), std::io::ErrorKind::Other);
        assert!(*observed.borrow());
        assert_eq!(fs::read_to_string(&path).expect("read destination"), "old");
        assert_eq!(fs::read_dir(&root).expect("read root").count(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn atomic_write_marks_directory_sync_failure_as_post_replace() {
        let root = test_root("post-replace-sync-failure");
        fs::create_dir_all(&root).expect("create dir");
        let path = root.join("state.json");
        fs::write(&path, "old").expect("write old");

        let err = write_bytes_atomic_inner(
            &path,
            b"new",
            AtomicWriteMetadata::default(),
            |_| Ok(()),
            |_| Err(std::io::Error::other("injected directory sync failure")),
        )
        .expect_err("directory sync must fail");

        assert!(atomic_write_replaced_destination(&err));
        assert_eq!(fs::read(&path).expect("read replaced destination"), b"new");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn atomic_write_bare_relative_child() {
        if std::env::var_os("FLISTWALKER_TEST_BARE_RELATIVE_ATOMIC_WRITE").is_none() {
            return;
        }
        write_text_atomic(Path::new("state.json"), "new").expect("write bare relative path");
    }

    #[test]
    fn atomic_write_supports_bare_relative_filename() {
        let root = test_root("bare-relative");
        fs::create_dir_all(&root).expect("create dir");
        let helper = "fs_atomic::tests::atomic_write_bare_relative_child";
        let status = Command::new(std::env::current_exe().expect("test executable"))
            .arg("--exact")
            .arg(helper)
            .current_dir(&root)
            .env("FLISTWALKER_TEST_BARE_RELATIVE_ATOMIC_WRITE", "1")
            .status()
            .expect("run bare relative child");
        assert!(status.success());
        assert_eq!(
            fs::read(root.join("state.json")).expect("read written file"),
            b"new"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_syncs_new_directory_chain_from_leaf_to_existing_ancestor() {
        let root = test_root("directory-chain-sync");
        fs::create_dir_all(&root).expect("create root");
        let path = root.join("one/two/state.json");
        let synced = RefCell::new(Vec::new());

        write_bytes_atomic_inner(
            &path,
            b"new",
            AtomicWriteMetadata::default(),
            |_| Ok(()),
            |directory| {
                synced.borrow_mut().push(directory.to_path_buf());
                Ok(())
            },
        )
        .expect("write");

        assert_eq!(
            *synced.borrow(),
            vec![root.join("one/two"), root.join("one"), root.clone()]
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn stale_sidecar_lock_file_is_reused_after_owner_exits() {
        let root = test_root("stale-lock");
        fs::create_dir_all(&root).expect("create dir");
        let path = root.join("state.json");
        let lock_path = sidecar_lock_path(&path);
        fs::write(&lock_path, format!("{}\n", u32::MAX)).expect("write stale lock");

        let lock = acquire_sidecar_lock(&path, Duration::from_millis(100))
            .expect("reuse stale sidecar lock file");
        assert!(lock_path.exists());
        let blocked = match acquire_sidecar_lock(&path, Duration::from_millis(20)) {
            Ok(_) => panic!("active OS lock must block a second writer"),
            Err(err) => err,
        };
        assert_eq!(blocked.kind(), std::io::ErrorKind::TimedOut);
        drop(lock);
        let next = acquire_sidecar_lock(&path, Duration::from_millis(100))
            .expect("reuse lock after owner exits");
        drop(next);
        assert!(lock_path.exists());
        let _ = fs::remove_dir_all(&root);
    }
}
