//! OS reveal leaves. Invoked only from the action worker, after authorization.
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

/// The session file manager does not provide ShowItems. The lifecycle owns
/// fallback so it can recheck authorization, cancellation and freshness first.
#[derive(Debug)]
pub(super) struct RevealUnavailable;
impl std::fmt::Display for RevealUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("file manager selection is unavailable")
    }
}
impl std::error::Error for RevealUnavailable {}

pub fn reveal_items(parent: &Path, items: &[PathBuf]) -> Result<()> {
    if items.is_empty() || items.iter().any(|item| item.parent() != Some(parent)) {
        bail!("reveal requires children of one authorized folder");
    }
    platform_reveal(parent, items)
}

#[cfg(target_os = "macos")]
fn platform_reveal(_parent: &Path, items: &[PathBuf]) -> Result<()> {
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSArray, NSURL};
    use std::{ffi::CString, os::unix::ffi::OsStrExt, ptr::NonNull};
    autoreleasepool(|_| {
        let urls = items
            .iter()
            .map(|path| {
                let bytes = CString::new(path.as_os_str().as_bytes())?;
                // NSURL copies the NUL-terminated filesystem representation. This
                // preserves non-UTF8 filenames without interpreting URL delimiters.
                Ok(unsafe {
                    NSURL::fileURLWithFileSystemRepresentation_isDirectory_relativeToURL(
                        NonNull::new(bytes.as_ptr().cast_mut()).expect("CString is non-null"),
                        false,
                        None,
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?;
        NSWorkspace::sharedWorkspace()
            .activateFileViewerSelectingURLs(&NSArray::from_retained_slice(&urls));
        // The API submits an asynchronous Finder request; it does not attest
        // that a window is already visible or guarantee keyboard focus.
        Ok(())
    })
}

#[cfg(target_os = "windows")]
fn platform_reveal(parent: &Path, items: &[PathBuf]) -> Result<()> {
    use std::{ffi::c_void, ptr};
    #[link(name = "ole32")]
    unsafe extern "system" {
        fn CoInitializeEx(reserved: *mut c_void, flags: u32) -> i32;
        fn CoUninitialize();
        fn CoTaskMemFree(ptr: *mut c_void);
    }
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SHParseDisplayName(
            name: *const u16,
            bind: *mut c_void,
            pidl: *mut *mut c_void,
            attrs: u32,
            out_attrs: *mut u32,
        ) -> i32;
        fn ILFindLastID(pidl: *const c_void) -> *const c_void;
        fn SHOpenFolderAndSelectItems(
            folder: *const c_void,
            count: u32,
            items: *const *const c_void,
            flags: u32,
        ) -> i32;
    }
    struct Apartment(bool);
    impl Drop for Apartment {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() }
            }
        }
    }
    struct Pidl(*mut c_void);
    impl Drop for Pidl {
        fn drop(&mut self) {
            unsafe { CoTaskMemFree(self.0) }
        }
    }
    fn parse(path: &Path) -> Result<Pidl> {
        let wide = windows_shell_wide(path)?;
        let mut pidl = ptr::null_mut();
        let hr = unsafe {
            SHParseDisplayName(
                wide.as_ptr(),
                ptr::null_mut(),
                &mut pidl,
                0,
                ptr::null_mut(),
            )
        };
        // Own even an unexpected allocation on an error path.
        let pidl = Pidl(pidl);
        if hr < 0 || pidl.0.is_null() {
            bail!("Shell path parsing failed: {hr:#x}");
        }
        Ok(pidl)
    }
    // Apartment threaded. An existing MTA is also initialized COM;
    // do not uninitialize its owner.
    let hr = unsafe { CoInitializeEx(ptr::null_mut(), 2) };
    if hr < 0 && hr != 0x80010106u32 as i32 {
        bail!("COM initialization failed: {hr:#x}");
    }
    let _apartment = Apartment(hr >= 0);
    let folder = parse(parent)?;
    let children = items.iter().map(|p| parse(p)).collect::<Result<Vec<_>>>()?;
    let relative = children
        .iter()
        .map(|p| unsafe { ILFindLastID(p.0) })
        .collect::<Vec<_>>();
    let count = u32::try_from(relative.len())?;
    let hr = unsafe { SHOpenFolderAndSelectItems(folder.0, count, relative.as_ptr(), 0) };
    if hr < 0 {
        bail!("Shell selection failed: {hr:#x}");
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn windows_shell_wide(path: &Path) -> Result<Vec<u16>> {
    use std::os::windows::ffi::OsStrExt;
    let path = crate::path_utils::normalize_windows_shell_path(path);
    let mut wide: Vec<_> = path.as_os_str().encode_wide().collect();
    if wide.contains(&0) {
        bail!("NUL in Shell path");
    }
    wide.push(0);
    Ok(wide)
}

#[cfg(target_os = "linux")]
fn platform_reveal(_parent: &Path, items: &[PathBuf]) -> Result<()> {
    use std::process::{Command, Stdio};
    let uris = items
        .iter()
        .map(|p| file_uri(p))
        .collect::<Result<Vec<_>>>()?;
    let args = show_items_args(&uris);
    let result = Command::new("dbus-send")
        .args(args)
        .stdin(Stdio::null())
        .output();
    if result.is_ok_and(|output| output.status.success()) {
        Ok(())
    } else {
        Err(RevealUnavailable.into())
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn platform_reveal(_parent: &Path, _items: &[PathBuf]) -> Result<()> {
    Err(RevealUnavailable.into())
}

#[cfg(all(unix, any(target_os = "linux", test)))]
fn show_items_args(uris: &[String]) -> Vec<String> {
    vec![
        "--session".into(),
        "--print-reply".into(),
        "--type=method_call".into(),
        "--reply-timeout=3000".into(),
        "--dest=org.freedesktop.FileManager1".into(),
        "/org/freedesktop/FileManager1".into(),
        "org.freedesktop.FileManager1.ShowItems".into(),
        format!("array:string:{}", uris.join(",")),
        "string:".into(),
    ]
}

#[cfg(all(unix, any(target_os = "linux", test)))]
fn file_uri(path: &Path) -> Result<String> {
    use std::fmt::Write;
    use std::os::unix::ffi::OsStrExt;
    if !path.is_absolute() {
        bail!("file URI requires absolute path");
    }
    let mut uri = String::from("file://");
    for &b in path.as_os_str().as_bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
            uri.push(b as char);
        } else {
            write!(uri, "%{b:02X}")?;
        }
    }
    Ok(uri)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn tc_050_file_manager_uri_and_argv_preserve_literal_paths() {
        use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
        let uri = file_uri(Path::new("/tmp/日本 語/a,b#%?&'\"\\.txt")).unwrap();
        assert_eq!(
            uri,
            "file:///tmp/%E6%97%A5%E6%9C%AC%20%E8%AA%9E/a%2Cb%23%25%3F%26%27%22%5C.txt"
        );
        assert_eq!(
            file_uri(Path::new(OsStr::from_bytes(b"/tmp/\xff"))).unwrap(),
            "file:///tmp/%FF"
        );
        let args = show_items_args(&[uri.clone(), "file:///tmp/second".into()]);
        assert_eq!(args[7], format!("array:string:{uri},file:///tmp/second"));
        assert_eq!(args[8], "string:");
        assert!(file_uri(Path::new("relative")).is_err());
    }
    #[cfg(windows)]
    #[test]
    fn tc_051_windows_reveal_wide_paths_preserve_unc_and_special_characters() {
        use std::{ffi::OsString, os::windows::ffi::OsStringExt};
        for (input, expected) in [
            (
                r"\\?\UNC\server\share\日本 語 & [1],#.txt",
                r"\\server\share\日本 語 & [1],#.txt",
            ),
            (r"\\?\C:\folder\a,b.txt", r"C:\folder\a,b.txt"),
        ] {
            let wide = windows_shell_wide(Path::new(input)).unwrap();
            assert_eq!(wide.last(), Some(&0));
            assert_eq!(OsString::from_wide(&wide[..wide.len() - 1]), expected);
        }
        assert!(windows_shell_wide(Path::new("C:\\nul\0path")).is_err());
    }
}
