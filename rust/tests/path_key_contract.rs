use flist_walker::path_utils::path_key;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::path::Path;

// Keep allocation instrumentation in this integration-test binary, separate
// from application workers and the library test suite. Count only this thread
// and only while path_key runs; fixture construction/assertions are excluded.
struct CountingAllocator;

thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}

fn record_allocation() {
    let _ = ALLOCATIONS.try_with(|count| {
        if let Some(current) = count.get() {
            count.set(Some(current + 1));
        }
    });
}

// SAFETY: every allocation operation is forwarded to System with its original
// pointer/layout/size. Instrumentation neither allocates nor touches the memory.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record_allocation();
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
fn valid_path_key_needs_only_its_output_allocation() {
    for (text, _windows_key) in [
        (r"C:\Projects\README.TXT", r"c:\projects\readme.txt"),
        ("日本語/Ä/İ/ß/FILE.txt", "日本語/Ä/İ/ß/file.txt"),
        (r"\\?\C:\Folder\..\FILE", r"\\?\c:\folder\..\file"),
        (r"\\Server\Share\FILE", r"\\server\share\file"),
        ("", ""),
    ] {
        let path = Path::new(text);
        ALLOCATIONS.with(|count| count.set(Some(0)));
        let key = std::hint::black_box(path_key(std::hint::black_box(path)));
        let allocations = ALLOCATIONS.with(|count| count.replace(None).unwrap());

        assert!(allocations <= 1, "{text:?}: {allocations} allocations");
        #[cfg(windows)]
        assert_eq!(key, _windows_key);
        #[cfg(not(windows))]
        assert_eq!(key, text);
    }
}

#[test]
#[cfg(windows)]
fn windows_path_key_preserves_lossy_encoding_and_ascii_only_case_policy() {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    let path = OsString::from_wide(&[b'A' as u16, 0xd800, b'B' as u16, 0x00c4]);
    assert_eq!(path_key(Path::new(&path)), "a\u{fffd}bÄ");
}

#[test]
#[cfg(unix)]
fn unix_path_key_preserves_case_and_lossy_encoding() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let path = OsStr::from_bytes(b"A\xffB");
    assert_eq!(path_key(Path::new(path)), "A\u{fffd}B");
}
