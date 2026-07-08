use std::path::Path;
#[cfg(windows)]
use std::{ffi::c_void, os::windows::ffi::OsStrExt, ptr, sync::OnceLock};
pub fn should_skip_preview(path: &Path, is_dir: bool) -> bool {
    !is_dir && is_on_demand_file(path)
}

fn is_on_demand_file(path: &Path) -> bool {
    #[cfg(windows)]
    {
        if let Some(info) = read_file_attribute_tag_info(path) {
            return should_skip_preview_from_attr_tag(info.file_attributes, Some(info.reparse_tag));
        }

        std::fs::metadata(path)
            .map(|m| should_skip_preview_from_attr_tag(metadata_file_attributes(&m), None))
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug)]
struct FileAttributeTagInfoRecord {
    file_attributes: u32,
    reparse_tag: u32,
}

#[cfg(windows)]
fn metadata_file_attributes(metadata: &std::fs::Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;

    metadata.file_attributes()
}

#[cfg(any(test, windows))]
fn should_skip_preview_from_attr_tag(file_attributes: u32, reparse_tag: Option<u32>) -> bool {
    has_on_demand_attributes(file_attributes)
        || reparse_tag
            .map(|tag| is_cloud_placeholder(file_attributes, tag))
            .unwrap_or(false)
}

#[cfg(any(test, windows))]
fn has_on_demand_attributes(file_attributes: u32) -> bool {
    const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
    const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
    const FILE_ATTRIBUTE_UNPINNED: u32 = 0x0010_0000;
    const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;

    (file_attributes
        & (FILE_ATTRIBUTE_OFFLINE
            | FILE_ATTRIBUTE_RECALL_ON_OPEN
            | FILE_ATTRIBUTE_UNPINNED
            | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS))
        != 0
}

#[cfg(windows)]
fn is_cloud_placeholder(file_attributes: u32, reparse_tag: u32) -> bool {
    cf_get_placeholder_state_from_attribute_tag(file_attributes, reparse_tag) != 0
}

#[cfg(all(test, not(windows)))]
fn is_cloud_placeholder(_file_attributes: u32, _reparse_tag: u32) -> bool {
    false
}

#[cfg(windows)]
fn read_file_attribute_tag_info(path: &Path) -> Option<FileAttributeTagInfoRecord> {
    const FILE_READ_ATTRIBUTES: u32 = 0x0080;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const FILE_SHARE_DELETE: u32 = 0x0000_0004;
    const OPEN_EXISTING: u32 = 3;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_ATTRIBUTE_TAG_INFO_CLASS: i32 = 9;

    let mut wide_path: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide_path.push(0);

    let handle = unsafe {
        create_file_w(
            wide_path.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null_mut(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            ptr::null_mut(),
        )
    };

    if handle == invalid_handle_value() {
        return None;
    }

    let mut info = RawFileAttributeTagInfo {
        file_attributes: 0,
        reparse_tag: 0,
    };
    let ok = unsafe {
        get_file_information_by_handle_ex(
            handle,
            FILE_ATTRIBUTE_TAG_INFO_CLASS,
            (&mut info as *mut RawFileAttributeTagInfo).cast::<c_void>(),
            std::mem::size_of::<RawFileAttributeTagInfo>() as u32,
        ) != 0
    };
    unsafe {
        close_handle(handle);
    }

    ok.then_some(FileAttributeTagInfoRecord {
        file_attributes: info.file_attributes,
        reparse_tag: info.reparse_tag,
    })
}

#[cfg(windows)]
#[repr(C)]
struct RawFileAttributeTagInfo {
    file_attributes: u32,
    reparse_tag: u32,
}

#[cfg(windows)]
fn invalid_handle_value() -> *mut c_void {
    (-1isize) as *mut c_void
}

#[cfg(all(windows, target_env = "gnu"))]
#[link(name = "kernel32")]
extern "system" {
    fn CreateFileW(
        lpFileName: *const u16,
        dwDesiredAccess: u32,
        dwShareMode: u32,
        lpSecurityAttributes: *mut c_void,
        dwCreationDisposition: u32,
        dwFlagsAndAttributes: u32,
        hTemplateFile: *mut c_void,
    ) -> *mut c_void;
    fn GetFileInformationByHandleEx(
        hFile: *mut c_void,
        FileInformationClass: i32,
        lpFileInformation: *mut c_void,
        dwBufferSize: u32,
    ) -> i32;
    fn CloseHandle(hObject: *mut c_void) -> i32;
    fn LoadLibraryW(lpLibFileName: *const u16) -> *mut c_void;
    fn GetProcAddress(hModule: *mut c_void, lpProcName: *const u8) -> *mut c_void;
}

#[cfg(all(windows, not(target_env = "gnu")))]
#[link(name = "Kernel32")]
extern "system" {
    fn CreateFileW(
        lpFileName: *const u16,
        dwDesiredAccess: u32,
        dwShareMode: u32,
        lpSecurityAttributes: *mut c_void,
        dwCreationDisposition: u32,
        dwFlagsAndAttributes: u32,
        hTemplateFile: *mut c_void,
    ) -> *mut c_void;
    fn GetFileInformationByHandleEx(
        hFile: *mut c_void,
        FileInformationClass: i32,
        lpFileInformation: *mut c_void,
        dwBufferSize: u32,
    ) -> i32;
    fn CloseHandle(hObject: *mut c_void) -> i32;
    fn LoadLibraryW(lpLibFileName: *const u16) -> *mut c_void;
    fn GetProcAddress(hModule: *mut c_void, lpProcName: *const u8) -> *mut c_void;
}

#[cfg(windows)]
unsafe fn create_file_w(
    path: *const u16,
    desired_access: u32,
    share_mode: u32,
    security_attributes: *mut c_void,
    creation_disposition: u32,
    flags_and_attributes: u32,
    template_file: *mut c_void,
) -> *mut c_void {
    CreateFileW(
        path,
        desired_access,
        share_mode,
        security_attributes,
        creation_disposition,
        flags_and_attributes,
        template_file,
    )
}

#[cfg(windows)]
unsafe fn get_file_information_by_handle_ex(
    handle: *mut c_void,
    info_class: i32,
    file_information: *mut c_void,
    buffer_size: u32,
) -> i32 {
    GetFileInformationByHandleEx(handle, info_class, file_information, buffer_size)
}

#[cfg(windows)]
unsafe fn close_handle(handle: *mut c_void) -> i32 {
    CloseHandle(handle)
}

#[cfg(windows)]
fn cf_get_placeholder_state_from_attribute_tag(file_attributes: u32, reparse_tag: u32) -> u32 {
    type CfGetPlaceholderStateFromAttributeTagFn = unsafe extern "system" fn(u32, u32) -> u32;

    fn resolve() -> Option<CfGetPlaceholderStateFromAttributeTagFn> {
        static FN: OnceLock<Option<CfGetPlaceholderStateFromAttributeTagFn>> = OnceLock::new();

        *FN.get_or_init(|| {
            let mut dll_name: Vec<u16> = "cldapi.dll".encode_utf16().collect();
            dll_name.push(0);
            let module = unsafe { LoadLibraryW(dll_name.as_ptr()) };
            if module.is_null() {
                return None;
            }

            let proc = unsafe {
                GetProcAddress(
                    module,
                    c"CfGetPlaceholderStateFromAttributeTag".as_ptr().cast(),
                )
            };
            if proc.is_null() {
                None
            } else {
                Some(unsafe {
                    std::mem::transmute::<*mut c_void, CfGetPlaceholderStateFromAttributeTagFn>(
                        proc,
                    )
                })
            }
        })
    }

    resolve()
        .map(|func| unsafe { func(file_attributes, reparse_tag) })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn on_demand_attribute_bits_skip_preview_without_reparse_tag() {
        assert!(should_skip_preview_from_attr_tag(0x0000_1000, None));
        assert!(should_skip_preview_from_attr_tag(0x0004_0000, None));
        assert!(should_skip_preview_from_attr_tag(0x0010_0000, None));
        assert!(should_skip_preview_from_attr_tag(0x0040_0000, None));
    }

    #[test]
    fn plain_file_attributes_do_not_skip_preview() {
        assert!(!should_skip_preview_from_attr_tag(0, None));
    }
}
