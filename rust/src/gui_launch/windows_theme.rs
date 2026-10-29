//! Keep an explicit app theme across winit 0.30's Windows settings messages.
//! Its Window::set_theme does not update preferred_theme; WM_SETTINGCHANGE can
//! otherwise restore the OS titlebar after the egui client area was forced.
use std::cell::Cell;

const WM_SETTINGCHANGE: u32 = 0x001a;
const WM_THEMECHANGED: u32 = 0x031a;
const WM_NCDESTROY: u32 = 0x0082;

thread_local! {
    static APPLYING_THEME: Cell<bool> = const { Cell::new(false) };
}

fn dispatch_message(
    message: u32,
    remove: impl FnOnce(),
    default: impl FnOnce() -> isize,
    apply: impl FnOnce(),
) -> isize {
    if message == WM_NCDESTROY {
        remove(); // HWND must still be valid when the subclass is removed.
    }
    let result = default();
    if matches!(message, WM_SETTINGCHANGE | WM_THEMECHANGED) {
        // Theme APIs can synchronously send another theme message. No heap
        // state/pointer is stored in the HWND; this guard is event-thread local.
        let _ = APPLYING_THEME.try_with(|busy| {
            if !busy.replace(true) {
                apply();
                busy.set(false);
            }
        });
    }
    result
}

#[cfg(target_os = "windows")]
mod native {
    use super::*;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};

    type Hwnd = *mut c_void;
    type SubclassProc = unsafe extern "system" fn(Hwnd, u32, usize, isize, usize, usize) -> isize;
    const SUBCLASS_ID: usize = 0x46575448;

    #[link(name = "comctl32")]
    extern "system" {
        fn SetWindowSubclass(hwnd: Hwnd, callback: SubclassProc, id: usize, data: usize) -> i32;
        fn RemoveWindowSubclass(hwnd: Hwnd, callback: SubclassProc, id: usize) -> i32;
        fn DefSubclassProc(hwnd: Hwnd, message: u32, wparam: usize, lparam: isize) -> isize;
    }
    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmSetWindowAttribute(
            hwnd: Hwnd,
            attribute: u32,
            value: *const c_void,
            size: u32,
        ) -> i32;
    }

    // No diagnostics or other Rust callbacks may unwind into a Win32 wndproc.
    fn report_error(operation: &str, code: i32) {
        static REPORTED: AtomicBool = AtomicBool::new(false);
        if !REPORTED.swap(true, Ordering::Relaxed) {
            let _ = std::panic::catch_unwind(|| {
                tracing::warn!(operation, code, "native titlebar theme update failed");
            });
        }
    }

    fn apply(hwnd: Hwnd, dark: bool) -> Result<(), i32> {
        let value = i32::from(dark);
        // SAFETY: called synchronously on the owner thread for its live HWND.
        // The BOOL remains valid for the duration of both API calls.
        let result = unsafe { DwmSetWindowAttribute(hwnd, 20, (&value as *const i32).cast(), 4) };
        if result >= 0 {
            return Ok(());
        }
        // Older Windows 10 versions used attribute 19 for immersive dark mode.
        let legacy = unsafe { DwmSetWindowAttribute(hwnd, 19, (&value as *const i32).cast(), 4) };
        if legacy >= 0 {
            Ok(())
        } else {
            Err(legacy)
        }
    }

    unsafe extern "system" fn callback(
        hwnd: Hwnd,
        message: u32,
        wparam: usize,
        lparam: isize,
        id: usize,
        dark: usize,
    ) -> isize {
        dispatch_message(
            message,
            || {
                // SAFETY: remove this exact callback before native destruction.
                if unsafe { RemoveWindowSubclass(hwnd, callback, id) } == 0 {
                    report_error("RemoveWindowSubclass", 0);
                }
            },
            || unsafe { DefSubclassProc(hwnd, message, wparam, lparam) },
            || {
                if let Err(code) = apply(hwnd, dark != 0) {
                    report_error("DwmSetWindowAttribute", code);
                }
            },
        )
    }

    pub(crate) fn install(
        cc: &eframe::CreationContext<'_>,
        theme: flist_walker::runtime_config::ThemeMode,
    ) -> anyhow::Result<()> {
        use flist_walker::runtime_config::ThemeMode;
        let dark = match theme {
            ThemeMode::System => return Ok(()), // native OS tracking stays untouched
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
        };
        let handle = cc
            .window_handle()
            .map_err(|error| anyhow::anyhow!("native window handle unavailable: {error}"))?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            anyhow::bail!("Windows titlebar theme requires a Win32 window");
        };
        let hwnd = handle.hwnd.get() as Hwnd;
        apply(hwnd, dark)
            .map_err(|code| anyhow::anyhow!("Windows titlebar theme failed: {code:#x}"))?;
        // SAFETY: app creation and all callbacks are on the native owner thread.
        // Refdata contains only a bool; no pointer, ownership or allocation leaks.
        if unsafe { SetWindowSubclass(hwnd, callback, SUBCLASS_ID, usize::from(dark)) } == 0 {
            anyhow::bail!("Could not preserve the selected Windows titlebar theme");
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
pub(super) use native::install;

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn os_theme_reset_is_corrected_after_default_without_changing_message_result() {
        for message in [WM_SETTINGCHANGE, WM_THEMECHANGED] {
            for desired in [false, true] {
                let dark = Cell::new(desired);
                let result = dispatch_message(
                    message,
                    || panic!("must not remove a live window hook"),
                    || {
                        dark.set(!desired);
                        73
                    },
                    || dark.set(desired),
                );
                assert_eq!(dark.get(), desired);
                assert_eq!(result, 73);
            }
        }
    }

    #[test]
    fn native_destroy_removes_hook_before_default_and_never_reapplies() {
        let order = RefCell::new(Vec::new());
        dispatch_message(
            WM_NCDESTROY,
            || order.borrow_mut().push("remove"),
            || {
                order.borrow_mut().push("default");
                0
            },
            || panic!("must not apply to a destroyed HWND"),
        );
        assert_eq!(*order.borrow(), ["remove", "default"]);
        dispatch_message(
            0,
            || panic!("unrelated message must not remove hook"),
            || 0,
            || panic!("unrelated message must not apply theme"),
        );
    }

    #[test]
    fn synchronous_theme_message_cannot_reenter_theme_update() {
        let applies = Cell::new(0);
        dispatch_message(
            WM_SETTINGCHANGE,
            || {},
            || 0,
            || {
                applies.set(applies.get() + 1);
                dispatch_message(
                    WM_THEMECHANGED,
                    || {},
                    || 0,
                    || applies.set(applies.get() + 1),
                );
            },
        );
        assert_eq!(applies.get(), 1);
        dispatch_message(
            WM_THEMECHANGED,
            || {},
            || 0,
            || applies.set(applies.get() + 1),
        );
        assert_eq!(
            applies.get(),
            2,
            "guard must be released after the outer update"
        );
    }
}
