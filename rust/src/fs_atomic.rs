use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn write_text_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let tmp = build_temp_path(path);
    fs::write(&tmp, text)?;
    if let Err(err) = replace_file(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }
    Ok(())
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
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!(".{filename}.{pid}.{now}.{seq}.tmp"))
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
}
