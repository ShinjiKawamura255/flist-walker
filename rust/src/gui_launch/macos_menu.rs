use anyhow::{Context, Result};
use objc2::{sel, MainThreadMarker};
use objc2_app_kit::{NSApplication, NSMenu};
use objc2_foundation::ns_string;

pub(super) fn disable_native_quit_shortcut() -> Result<()> {
    let mtm = MainThreadMarker::new().context("macOS menu setup requires the main thread")?;
    if let Some(menu) = NSApplication::sharedApplication(mtm).mainMenu() {
        clear_quit_key_equivalents(&menu);
    }
    Ok(())
}

fn clear_quit_key_equivalents(menu: &NSMenu) {
    for item in menu.itemArray() {
        // Match the native action, independent of the process name or menu language.
        // Preserve the menu item so explicit Quit clicks still work normally.
        if item.action() == Some(sel!(terminate:)) {
            item.setKeyEquivalent(ns_string!(""));
        }
        if let Some(submenu) = item.submenu() {
            clear_quit_key_equivalents(&submenu);
        }
    }
}
