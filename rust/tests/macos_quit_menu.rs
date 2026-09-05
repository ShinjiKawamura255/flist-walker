// AppKit menu objects require the process main thread, so this test uses no libtest harness.
#[cfg(target_os = "macos")]
#[path = "../src/gui_launch/macos_menu.rs"]
mod macos_menu;

#[cfg(target_os = "macos")]
fn main() {
    use objc2::{sel, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSMenu, NSMenuItem};
    use objc2_foundation::{ns_string, NSString};

    let mtm = MainThreadMarker::new().expect("test runs on the process main thread");
    let main_menu = NSMenu::new(mtm);
    let app_item = NSMenuItem::new(mtm);
    let app_menu = NSMenu::new(mtm);
    app_item.setSubmenu(Some(&app_menu));
    main_menu.addItem(&app_item);

    // SAFETY: Both selectors name standard NSApplication actions. No action is executed.
    let quit_item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc(),
            ns_string!("終了"),
            Some(sel!(terminate:)),
            ns_string!("q"),
        )
    };
    let hide_item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc(),
            ns_string!("Hide"),
            Some(sel!(hide:)),
            ns_string!("h"),
        )
    };
    app_menu.addItem(&hide_item);
    app_menu.addItem(&quit_item);
    NSApplication::sharedApplication(mtm).setMainMenu(Some(&main_menu));

    macos_menu::disable_native_quit_shortcut().expect("configure native menu");
    macos_menu::disable_native_quit_shortcut().expect("configuration is idempotent");
    assert_eq!(quit_item.keyEquivalent(), NSString::from_str(""));
    assert_eq!(quit_item.action(), Some(sel!(terminate:)));
    assert_eq!(quit_item.title(), NSString::from_str("終了"));
    assert_eq!(hide_item.keyEquivalent(), NSString::from_str("h"));
    assert_eq!(app_menu.numberOfItems(), 2);
    assert_eq!(main_menu.numberOfItems(), 1);
    NSApplication::sharedApplication(mtm).setMainMenu(None);
    macos_menu::disable_native_quit_shortcut().expect("no menu has no quit shortcut");
    println!("macOS quit menu shortcut: PASS (menu actions preserved)");
}

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("macOS quit menu shortcut: SKIPPED (requires macOS)");
}
