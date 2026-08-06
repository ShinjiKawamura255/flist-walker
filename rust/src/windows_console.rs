/// Detach the GUI path from an inherited or newly allocated Windows console.
///
/// The executable intentionally uses the console PE subsystem so PowerShell and
/// cmd wait for CLI/TUI completion and preserve standard handles. Only the GUI
/// path calls this function; CLI, help, version, and update commands return
/// before reaching it.
pub(crate) fn detach_from_console_for_gui() {
    detach_from_console_for_gui_impl();
}

#[cfg(target_os = "windows")]
fn detach_from_console_for_gui_impl() {
    #[link(name = "kernel32")]
    extern "system" {
        #[link_name = "FreeConsole"]
        fn free_console() -> i32;
    }

    // A false result only means that this process had no console to detach
    // from, which is already the desired GUI state.
    unsafe {
        let _ = free_console();
    }
}

#[cfg(not(target_os = "windows"))]
fn detach_from_console_for_gui_impl() {}
