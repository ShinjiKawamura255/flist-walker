use crate::path_utils::normalize_path_for_display;
#[cfg(target_os = "windows")]
use crate::path_utils::normalize_windows_shell_path;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
#[cfg(target_os = "windows")]
use std::{ffi::OsStr, os::windows::ffi::OsStrExt, ptr};

/// The action adapters expose only these side-effect modes. Neither mode accepts a shell command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizedActionMode {
    ExecuteOrOpen,
    Reveal,
}

/// An immutable, root-bound snapshot submitted by a CLI or terminal adapter.
#[derive(Debug, Clone)]
pub struct AuthorizedActionRequest {
    pub request_id: u64,
    pub trusted_root: PathBuf,
    pub selected_targets: Vec<PathBuf>,
    pub mode: AuthorizedActionMode,
    cancellation: Arc<AtomicBool>,
}

impl AuthorizedActionRequest {
    pub fn new(
        request_id: u64,
        trusted_root: PathBuf,
        selected_targets: Vec<PathBuf>,
        mode: AuthorizedActionMode,
    ) -> Self {
        Self::new_with_cancellation(
            request_id,
            trusted_root,
            selected_targets,
            mode,
            Arc::new(AtomicBool::new(false)),
        )
    }

    pub fn new_with_cancellation(
        request_id: u64,
        trusted_root: PathBuf,
        selected_targets: Vec<PathBuf>,
        mode: AuthorizedActionMode,
        cancellation: Arc<AtomicBool>,
    ) -> Self {
        Self {
            request_id,
            trusted_root,
            selected_targets,
            mode,
            cancellation,
        }
    }
}

/// Adapter-owned freshness state. A root or request switch makes the request non-current.
pub trait AuthorizedActionGuard {
    fn is_current(&self, request_id: u64, trusted_root: &Path) -> bool;
}

/// OS-facing leaf. Implementations must use direct argv/path APIs rather than shell expansion.
pub trait AuthorizedActionBackend {
    fn execute_or_open(&self, path: &Path) -> Result<()>;
    fn reveal(&self, path: &Path) -> Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizedActionOutcome {
    Completed,
    Blocked,
    Canceled,
    Superseded,
    Failed,
    PartialFailure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedActionReport {
    pub request_id: u64,
    pub outcome: AuthorizedActionOutcome,
    pub completed: usize,
    pub total: usize,
    pub display_path: Option<PathBuf>,
    pub display_targets: Vec<PathBuf>,
    /// Safe, user-facing authorization or lifecycle reason.
    pub diagnostic: Option<String>,
    /// Raw executor error for a non-GUI adapter's diagnostic channel.
    pub backend_error: Option<String>,
}

impl AuthorizedActionReport {
    fn new(
        request: &AuthorizedActionRequest,
        outcome: AuthorizedActionOutcome,
        completed: usize,
        total: usize,
        display_path: Option<PathBuf>,
    ) -> Self {
        Self {
            request_id: request.request_id,
            outcome,
            completed,
            total,
            display_path,
            display_targets: Vec::new(),
            diagnostic: None,
            backend_error: None,
        }
    }

    fn with_diagnostic(mut self, diagnostic: impl Into<String>) -> Self {
        self.diagnostic = Some(diagnostic.into());
        self
    }

    fn with_backend_error(mut self, backend_error: impl Into<String>) -> Self {
        self.backend_error = Some(backend_error.into());
        self
    }
}

fn observed_request_outcome(
    request: &AuthorizedActionRequest,
    guard: &dyn AuthorizedActionGuard,
) -> Option<AuthorizedActionOutcome> {
    if request.cancellation.load(Ordering::Acquire) {
        Some(AuthorizedActionOutcome::Canceled)
    } else if !guard.is_current(request.request_id, &request.trusted_root) {
        Some(AuthorizedActionOutcome::Superseded)
    } else {
        None
    }
}

/// Preauthorizes every target, then reauthorizes immediately before each backend call.
///
/// Once cancellation or a root/request supersession is observed, no later backend call starts.
pub fn execute_authorized_action_request(
    request: &AuthorizedActionRequest,
    guard: &dyn AuthorizedActionGuard,
    backend: &dyn AuthorizedActionBackend,
) -> AuthorizedActionReport {
    let requested_total = request.selected_targets.len();
    if let Some(outcome) = observed_request_outcome(request, guard) {
        return AuthorizedActionReport::new(request, outcome, 0, requested_total, None);
    }

    let reveal = request.mode == AuthorizedActionMode::Reveal;
    let batch =
        match authorize_action_targets(&request.trusted_root, &request.selected_targets, reveal) {
            Ok(batch) => batch,
            Err(error) => {
                let diagnostic = error.to_string();
                return AuthorizedActionReport::new(
                    request,
                    AuthorizedActionOutcome::Blocked,
                    0,
                    requested_total,
                    error.display_path,
                )
                .with_diagnostic(diagnostic);
            }
        };
    let total = batch.targets.len();

    for (completed, target) in batch.targets.iter().enumerate() {
        if let Some(outcome) = observed_request_outcome(request, guard) {
            return AuthorizedActionReport::new(
                request,
                outcome,
                completed,
                total,
                Some(target.display_path.clone()),
            );
        }
        let execution_path = match reauthorize_action_target(&batch.canonical_root, target) {
            Ok(path) => path,
            Err(error) => {
                let diagnostic = error.to_string();
                let outcome = if completed == 0 {
                    AuthorizedActionOutcome::Blocked
                } else {
                    AuthorizedActionOutcome::PartialFailure
                };
                return AuthorizedActionReport::new(
                    request,
                    outcome,
                    completed,
                    total,
                    error
                        .display_path
                        .or_else(|| Some(target.display_path.clone())),
                )
                .with_diagnostic(diagnostic);
            }
        };
        if let Some(outcome) = observed_request_outcome(request, guard) {
            return AuthorizedActionReport::new(
                request,
                outcome,
                completed,
                total,
                Some(target.display_path.clone()),
            );
        }
        let action_result = match request.mode {
            AuthorizedActionMode::ExecuteOrOpen => backend.execute_or_open(&execution_path),
            AuthorizedActionMode::Reveal => backend.reveal(&execution_path),
        };
        if let Err(error) = action_result {
            let outcome = if completed == 0 {
                AuthorizedActionOutcome::Failed
            } else {
                AuthorizedActionOutcome::PartialFailure
            };
            return AuthorizedActionReport::new(
                request,
                outcome,
                completed,
                total,
                Some(target.display_path.clone()),
            )
            .with_diagnostic("executor failed")
            .with_backend_error(error.to_string());
        }
    }

    let mut report = AuthorizedActionReport::new(
        request,
        AuthorizedActionOutcome::Completed,
        total,
        total,
        None,
    );
    report.display_targets = batch
        .targets
        .iter()
        .map(|target| target.display_path.clone())
        .collect();
    report
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActionPathPrecheck {
    Reject,
    Defer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuthorizedActionTarget {
    pub(crate) display_path: PathBuf,
    pub(crate) execution_path: PathBuf,
    source_paths: Vec<PathBuf>,
    open_parent_for_files: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuthorizedActionBatch {
    pub(crate) canonical_root: PathBuf,
    pub(crate) targets: Vec<AuthorizedActionTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActionAuthorizationFailure {
    pub(crate) display_path: Option<PathBuf>,
    message: &'static str,
}

impl ActionAuthorizationFailure {
    fn new(display_path: Option<PathBuf>, message: &'static str) -> Self {
        Self {
            display_path,
            message,
        }
    }
}

impl fmt::Display for ActionAuthorizationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message)
    }
}

impl std::error::Error for ActionAuthorizationFailure {}

fn normalize_absolute_lexically(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    Some(normalized)
}

#[cfg(windows)]
fn lexically_within_root(root: &Path, path: &Path) -> Option<bool> {
    let mut root_key =
        crate::path_utils::strip_windows_extended_prefix(root.to_str()?).replace('\\', "/");
    let mut path_key =
        crate::path_utils::strip_windows_extended_prefix(path.to_str()?).replace('\\', "/");
    while root_key.len() > 1 && root_key.ends_with('/') {
        root_key.pop();
    }
    while path_key.len() > 1 && path_key.ends_with('/') {
        path_key.pop();
    }
    root_key.make_ascii_lowercase();
    path_key.make_ascii_lowercase();
    Some(
        path_key == root_key
            || path_key
                .strip_prefix(&root_key)
                .is_some_and(|suffix| suffix.starts_with('/')),
    )
}

#[cfg(not(windows))]
fn lexically_within_root(root: &Path, path: &Path) -> Option<bool> {
    Some(path.starts_with(root))
}

/// UI-only defense in depth. `Defer` is not authorization; the worker remains authoritative.
pub(crate) fn lexical_action_path_precheck(root: &Path, path: &Path) -> ActionPathPrecheck {
    let Some(root) = normalize_absolute_lexically(root) else {
        return ActionPathPrecheck::Defer;
    };
    let Some(path) = normalize_absolute_lexically(path) else {
        return ActionPathPrecheck::Defer;
    };

    match lexically_within_root(&root, &path) {
        Some(false) => ActionPathPrecheck::Reject,
        Some(true) | None => ActionPathPrecheck::Defer,
    }
}

pub(crate) fn action_target_path_for_open_in_folder(
    path: &Path,
) -> Result<PathBuf, ActionAuthorizationFailure> {
    let link_metadata = fs::symlink_metadata(path).map_err(|_| {
        ActionAuthorizationFailure::new(
            Some(path.to_path_buf()),
            "selected path type could not be determined",
        )
    })?;
    let metadata = if link_metadata.file_type().is_symlink() {
        fs::metadata(path).map_err(|_| {
            ActionAuthorizationFailure::new(
                Some(path.to_path_buf()),
                "selected link target could not be resolved",
            )
        })?
    } else {
        link_metadata
    };

    if metadata.is_dir() {
        return Ok(path.to_path_buf());
    }
    if !metadata.is_file() {
        return Err(ActionAuthorizationFailure::new(
            Some(path.to_path_buf()),
            "selected path type is unsupported",
        ));
    }
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            ActionAuthorizationFailure::new(
                Some(path.to_path_buf()),
                "containing folder could not be determined",
            )
        })
}

fn raw_action_target(
    path: &Path,
    open_parent_for_files: bool,
) -> Result<PathBuf, ActionAuthorizationFailure> {
    if !open_parent_for_files {
        return Ok(path.to_path_buf());
    }
    action_target_path_for_open_in_folder(path)
}

fn resolve_within_root(
    canonical_root: &Path,
    raw_path: &Path,
) -> Result<PathBuf, ActionAuthorizationFailure> {
    let resolved = raw_path.canonicalize().map_err(|_| {
        ActionAuthorizationFailure::new(
            Some(raw_path.to_path_buf()),
            "path could not be resolved within current root",
        )
    })?;
    if !resolved.starts_with(canonical_root) {
        return Err(ActionAuthorizationFailure::new(
            Some(raw_path.to_path_buf()),
            "path is outside current root",
        ));
    }
    Ok(resolved)
}

pub(crate) fn authorize_action_targets(
    root: &Path,
    paths: &[PathBuf],
    open_parent_for_files: bool,
) -> Result<AuthorizedActionBatch, ActionAuthorizationFailure> {
    let canonical_root = root.canonicalize().map_err(|_| {
        ActionAuthorizationFailure::new(
            paths.first().cloned(),
            "current root could not be resolved",
        )
    })?;
    let mut target_indices: HashMap<PathBuf, usize> = HashMap::with_capacity(paths.len());
    let mut targets: Vec<AuthorizedActionTarget> = Vec::with_capacity(paths.len());

    for source_path in paths {
        let raw_path = raw_action_target(source_path, open_parent_for_files)?;
        let execution_path = resolve_within_root(&canonical_root, &raw_path)?;
        if let Some(index) = target_indices.get(&execution_path).copied() {
            targets[index].source_paths.push(source_path.clone());
            continue;
        }
        target_indices.insert(execution_path.clone(), targets.len());
        targets.push(AuthorizedActionTarget {
            display_path: raw_path,
            execution_path,
            source_paths: vec![source_path.clone()],
            open_parent_for_files,
        });
    }

    Ok(AuthorizedActionBatch {
        canonical_root,
        targets,
    })
}

pub(crate) fn reauthorize_action_target(
    canonical_root: &Path,
    target: &AuthorizedActionTarget,
) -> Result<PathBuf, ActionAuthorizationFailure> {
    let mut final_execution_path = None;
    for source_path in &target.source_paths {
        let raw_path = raw_action_target(source_path, target.open_parent_for_files)?;
        let execution_path = resolve_within_root(canonical_root, &raw_path)?;
        if execution_path != target.execution_path {
            return Err(ActionAuthorizationFailure::new(
                Some(target.display_path.clone()),
                "authorization changed",
            ));
        }
        final_execution_path = Some(execution_path);
    }
    final_execution_path.ok_or_else(|| {
        ActionAuthorizationFailure::new(
            Some(target.display_path.clone()),
            "action target is unavailable",
        )
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Open,
    Execute,
}

fn normalize_action_path_for_display(path: &Path) -> String {
    normalize_path_for_display(path)
}

#[cfg(target_os = "windows")]
fn encode_wide_null(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(target_os = "windows")]
fn shell_execute_error(code: isize) -> std::io::Error {
    if (2..=32).contains(&code) {
        return std::io::Error::from_raw_os_error(code as i32);
    }
    std::io::Error::other(format!("ShellExecuteW failed with code {code}"))
}

#[cfg(target_os = "windows")]
fn shell_open(path: &Path) -> std::io::Result<()> {
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ShellExecuteW(
            hwnd: *mut std::ffi::c_void,
            lp_operation: *const u16,
            lp_file: *const u16,
            lp_parameters: *const u16,
            lp_directory: *const u16,
            n_show_cmd: i32,
        ) -> isize;
    }

    const SW_SHOWNORMAL: i32 = 1;
    let operation = encode_wide_null(OsStr::new("open"));
    let target = normalize_windows_shell_path(path);
    let target_wide = encode_wide_null(target.as_os_str());
    let result = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            operation.as_ptr(),
            target_wide.as_ptr(),
            ptr::null(),
            ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result <= 32 {
        return Err(shell_execute_error(result));
    }
    Ok(())
}

pub fn choose_action(path: &Path) -> Action {
    if path.is_dir() {
        Action::Open
    } else {
        #[cfg(target_os = "windows")]
        {
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                let ext = ext.to_ascii_lowercase();
                if ["exe", "com", "bat", "cmd"].contains(&ext.as_str()) {
                    return Action::Execute;
                }
            }
            Action::Open
        }

        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = path.metadata() {
                if metadata.permissions().mode() & 0o111 != 0 {
                    return Action::Execute;
                }
            }
            Action::Open
        }
    }
}

fn spawn_executable(path: &Path) -> std::io::Result<()> {
    Command::new(path).spawn().map(|_| ())
}

fn execute_or_open_with(
    path: &Path,
    execute: impl FnOnce(&Path) -> std::io::Result<()>,
    open: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    match choose_action(path) {
        Action::Open => open(path),
        Action::Execute => {
            let result = execute(path);
            #[cfg(target_os = "windows")]
            {
                match result {
                    Ok(()) => Ok(()),
                    Err(err) if err.raw_os_error() == Some(193) => open(path),
                    Err(err) => Err(err).with_context(|| {
                        format!(
                            "failed to execute {}",
                            normalize_action_path_for_display(path)
                        )
                    }),
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                result.map(|_| ()).with_context(|| {
                    format!(
                        "failed to execute {}",
                        normalize_action_path_for_display(path)
                    )
                })
            }
        }
    }
}

pub fn execute_or_open(path: &Path) -> Result<()> {
    execute_or_open_with(path, spawn_executable, open_with_default)
}

fn open_text_editor(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        Command::new("notepad.exe").arg(path).spawn().map(|_| ())
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg("-t").arg(path).spawn().map(|_| ())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for editor_var in ["VISUAL", "EDITOR"] {
            if let Some(editor) = std::env::var_os(editor_var).filter(|value| !value.is_empty()) {
                return Command::new(editor).arg(path).spawn().map(|_| ());
            }
        }
        for editor in [
            "sensible-editor",
            "gedit",
            "kate",
            "mousepad",
            "xed",
            "leafpad",
        ] {
            if Command::new(editor).arg(path).spawn().is_ok() {
                return Ok(());
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no fallback text editor found",
        ))
    }
}

fn open_text_file_with_handlers(
    path: &Path,
    open_default: impl FnOnce(&Path) -> Result<()>,
    open_editor: impl FnOnce(&Path) -> std::io::Result<()>,
) -> Result<()> {
    match open_default(path) {
        Ok(()) => Ok(()),
        Err(default_err) => open_editor(path).with_context(|| {
            format!(
                "failed to open {} with the default app or fallback text editor; default app error: {default_err}",
                normalize_action_path_for_display(path)
            )
        }),
    }
}

pub fn open_text_file_with_default_or_editor(path: &Path) -> Result<()> {
    open_text_file_with_handlers(path, open_with_default, open_text_editor)
}

pub fn open_with_default(path: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        shell_open(path).with_context(|| {
            format!("failed to open {}", normalize_action_path_for_display(path))
        })?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open").arg(path).spawn().with_context(|| {
            format!("failed to open {}", normalize_action_path_for_display(path))
        })?;
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .with_context(|| {
                format!("failed to open {}", normalize_action_path_for_display(path))
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::fs;
    #[cfg(target_os = "windows")]
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    #[test]
    fn directory_is_open_action() {
        let dir = std::env::temp_dir();
        assert_eq!(choose_action(&dir), Action::Open);
    }

    #[test]
    fn non_exec_file_is_open_on_unix() {
        #[cfg(not(target_os = "windows"))]
        {
            let root = std::env::temp_dir().join("fff-rs-actions");
            let _ = fs::create_dir_all(&root);
            let file = root.join("note.txt");
            fs::write(&file, "x").expect("write file");
            assert_eq!(choose_action(&file), Action::Open);
        }
    }

    #[test]
    fn executable_file_is_execute_on_unix() {
        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;

            let root = std::env::temp_dir().join("fff-rs-actions-exec");
            let _ = fs::create_dir_all(&root);
            let file = root.join("run.sh");
            fs::write(&file, "#!/bin/sh\necho hi\n").expect("write file");
            let mut perms = fs::metadata(&file).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&file, perms).expect("set permissions");
            assert_eq!(choose_action(&file), Action::Execute);
        }
    }

    #[test]
    fn windows_executable_extension_is_execute() {
        #[cfg(target_os = "windows")]
        {
            let root = std::env::temp_dir().join("fff-rs-actions-winext");
            let _ = fs::create_dir_all(&root);
            let exe = root.join("tool.exe");
            fs::write(&exe, "bin").expect("write exe");
            assert_eq!(choose_action(&exe), Action::Execute);
        }
    }

    #[test]
    fn windows_powershell_script_is_open_action() {
        #[cfg(target_os = "windows")]
        {
            let root = std::env::temp_dir().join("fff-rs-actions-winps1");
            let _ = fs::create_dir_all(&root);
            let script = root.join("tool.ps1");
            fs::write(&script, "Write-Host test").expect("write script");
            assert_eq!(choose_action(&script), Action::Open);
        }
    }

    #[test]
    fn open_action_uses_open_handler_and_skips_execute_handler() {
        let path = std::env::temp_dir().join("fff-rs-actions-open");
        let execute_called = Cell::new(false);
        let open_called = Cell::new(false);

        execute_or_open_with(
            &path,
            |_| {
                execute_called.set(true);
                Ok(())
            },
            |_| {
                open_called.set(true);
                Ok(())
            },
        )
        .expect("open path");

        assert!(!execute_called.get());
        assert!(open_called.get());
    }

    #[test]
    fn execute_action_uses_execute_handler_and_skips_open_handler() {
        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;

            let root = std::env::temp_dir().join("fff-rs-actions-exec-handler");
            let _ = fs::create_dir_all(&root);
            let file = root.join("run.sh");
            fs::write(&file, "#!/bin/sh\necho hi\n").expect("write file");
            let mut perms = fs::metadata(&file).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&file, perms).expect("set permissions");

            let execute_called = Cell::new(false);
            let open_called = Cell::new(false);

            execute_or_open_with(
                &file,
                |_| {
                    execute_called.set(true);
                    Ok(())
                },
                |_| {
                    open_called.set(true);
                    Ok(())
                },
            )
            .expect("execute path");

            assert!(execute_called.get());
            assert!(!open_called.get());
        }
        #[cfg(target_os = "windows")]
        {
            let root = std::env::temp_dir().join("fff-rs-actions-exec-handler");
            let _ = fs::create_dir_all(&root);
            let exe = root.join("run.exe");
            fs::write(&exe, "bin").expect("write exe");

            let execute_called = Cell::new(false);
            let open_called = Cell::new(false);

            execute_or_open_with(
                &exe,
                |_| {
                    execute_called.set(true);
                    Ok(())
                },
                |_| {
                    open_called.set(true);
                    Ok(())
                },
            )
            .expect("execute path");

            assert!(execute_called.get());
            assert!(!open_called.get());
        }
    }

    #[test]
    fn execute_failure_returns_error_without_open_fallback_on_non_windows() {
        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;

            let root = std::env::temp_dir().join("fff-rs-actions-exec-failure");
            let _ = fs::create_dir_all(&root);
            let file = root.join("run.sh");
            fs::write(&file, "#!/bin/sh\necho hi\n").expect("write file");
            let mut perms = fs::metadata(&file).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&file, perms).expect("set permissions");

            let execute_called = Cell::new(false);
            let open_called = Cell::new(false);
            let err = std::io::Error::other("boom");

            let result = execute_or_open_with(
                &file,
                |_| {
                    execute_called.set(true);
                    Err(std::io::Error::other(err.to_string()))
                },
                |_| {
                    open_called.set(true);
                    Ok(())
                },
            );

            assert!(result.is_err());
            assert!(execute_called.get());
            assert!(!open_called.get());
        }
    }

    #[test]
    fn windows_execute_error_193_falls_back_to_open_handler() {
        #[cfg(target_os = "windows")]
        {
            let root = std::env::temp_dir().join("fff-rs-actions-exec-fallback");
            let _ = fs::create_dir_all(&root);
            let exe = root.join("run.exe");
            fs::write(&exe, "bin").expect("write exe");

            let open_called = Cell::new(false);
            let result = execute_or_open_with(
                &exe,
                |_| Err(std::io::Error::from_raw_os_error(193)),
                |_| {
                    open_called.set(true);
                    Ok(())
                },
            );

            assert!(result.is_ok());
            assert!(open_called.get());
        }
    }

    #[test]
    fn open_text_file_uses_default_app_first() {
        let path = std::env::temp_dir().join("fff-rs-actions-config-open");
        let default_called = Cell::new(false);
        let editor_called = Cell::new(false);

        open_text_file_with_handlers(
            &path,
            |_| {
                default_called.set(true);
                Ok(())
            },
            |_| {
                editor_called.set(true);
                Ok(())
            },
        )
        .expect("open config");

        assert!(default_called.get());
        assert!(!editor_called.get());
    }

    #[test]
    fn open_text_file_falls_back_to_text_editor() {
        let path = std::env::temp_dir().join("fff-rs-actions-config-open-fallback");
        let default_called = Cell::new(false);
        let editor_called = Cell::new(false);

        open_text_file_with_handlers(
            &path,
            |_| {
                default_called.set(true);
                Err(anyhow::anyhow!("no association"))
            },
            |_| {
                editor_called.set(true);
                Ok(())
            },
        )
        .expect("open config with fallback");

        assert!(default_called.get());
        assert!(editor_called.get());
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn normalize_action_path_for_display_strips_extended_prefix() {
        assert_eq!(
            normalize_action_path_for_display(Path::new(r"\\?\C:\Users\tester\file.txt")),
            r"C:\Users\tester\file.txt"
        );
        assert_eq!(
            normalize_action_path_for_display(Path::new(r"\\?\UNC\server\share\file.txt")),
            r"\\server\share\file.txt"
        );
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn normalize_windows_shell_path_strips_extended_prefix_and_keeps_special_chars() {
        let normalized =
            normalize_windows_shell_path(Path::new(r"\\?\C:\Users\tester\a&b [c];'!,()^$.txt"));
        assert_eq!(
            normalized,
            PathBuf::from(r"C:\Users\tester\a&b [c];'!,()^$.txt")
        );

        let unc =
            normalize_windows_shell_path(Path::new(r"\\?\UNC\server\share\dir&a\file[1].txt"));
        assert_eq!(unc, PathBuf::from(r"\\server\share\dir&a\file[1].txt"));
    }

    struct RecordingActionBackend {
        calls: Mutex<Vec<(AuthorizedActionMode, PathBuf)>>,
        on_call: Arc<dyn Fn(usize) + Send + Sync>,
    }

    impl RecordingActionBackend {
        fn new(on_call: impl Fn(usize) + Send + Sync + 'static) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                on_call: Arc::new(on_call),
            }
        }
    }

    impl AuthorizedActionBackend for RecordingActionBackend {
        fn execute_or_open(&self, path: &Path) -> Result<()> {
            let call_index = {
                let mut calls = self.calls.lock().expect("record action call");
                calls.push((AuthorizedActionMode::ExecuteOrOpen, path.to_path_buf()));
                calls.len()
            };
            (self.on_call)(call_index);
            Ok(())
        }

        fn reveal(&self, path: &Path) -> Result<()> {
            let call_index = {
                let mut calls = self.calls.lock().expect("record action call");
                calls.push((AuthorizedActionMode::Reveal, path.to_path_buf()));
                calls.len()
            };
            (self.on_call)(call_index);
            Ok(())
        }
    }

    struct FailingActionBackend;

    impl AuthorizedActionBackend for FailingActionBackend {
        fn execute_or_open(&self, _path: &Path) -> Result<()> {
            anyhow::bail!("recorded backend failure with actionable detail")
        }

        fn reveal(&self, _path: &Path) -> Result<()> {
            anyhow::bail!("recorded backend failure with actionable detail")
        }
    }

    struct TestActionGuard {
        current: AtomicBool,
    }

    impl TestActionGuard {
        fn current() -> Self {
            Self {
                current: AtomicBool::new(true),
            }
        }
    }

    impl AuthorizedActionGuard for TestActionGuard {
        fn is_current(&self, _request_id: u64, _trusted_root: &Path) -> bool {
            self.current.load(Ordering::SeqCst)
        }
    }

    #[test]
    fn tc_164_public_request_rejects_mixed_targets_before_any_backend_call() {
        let root = std::env::temp_dir().join("flistwalker-public-action-root");
        let outside = std::env::temp_dir().join("flistwalker-public-action-outside");
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
        fs::create_dir_all(&root).expect("create root");
        fs::create_dir_all(&outside).expect("create outside");
        let inside_file = root.join("inside.txt");
        let outside_file = outside.join("outside.txt");
        fs::write(&inside_file, "inside").expect("write inside");
        fs::write(&outside_file, "outside").expect("write outside");
        let request = AuthorizedActionRequest::new(
            701,
            root.clone(),
            vec![inside_file, outside_file],
            AuthorizedActionMode::ExecuteOrOpen,
        );
        let guard = TestActionGuard::current();
        let backend = RecordingActionBackend::new(|_| {});

        let report = execute_authorized_action_request(&request, &guard, &backend);

        assert_eq!(report.outcome, AuthorizedActionOutcome::Blocked);
        assert_eq!(report.completed, 0);
        assert_eq!(
            report.diagnostic.as_deref(),
            Some("path is outside current root")
        );
        assert!(report.backend_error.is_none());
        assert!(backend.calls.lock().expect("calls").is_empty());
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn tc_164_public_request_cancels_between_targets_before_next_backend_call() {
        let root = std::env::temp_dir().join("flistwalker-public-action-cancel");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create root");
        let first = root.join("first.txt");
        let second = root.join("second.txt");
        fs::write(&first, "first").expect("write first");
        fs::write(&second, "second").expect("write second");
        let cancellation = Arc::new(AtomicBool::new(false));
        let request = AuthorizedActionRequest::new_with_cancellation(
            702,
            root.clone(),
            vec![first, second],
            AuthorizedActionMode::ExecuteOrOpen,
            Arc::clone(&cancellation),
        );
        let guard = Arc::new(TestActionGuard::current());
        let backend = RecordingActionBackend::new(move |call_index| {
            if call_index == 1 {
                cancellation.store(true, Ordering::SeqCst);
            }
        });

        let report = execute_authorized_action_request(&request, guard.as_ref(), &backend);

        assert_eq!(report.outcome, AuthorizedActionOutcome::Canceled);
        assert_eq!(report.completed, 1);
        assert_eq!(backend.calls.lock().expect("calls").len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn tc_164_public_request_rejects_observed_root_or_request_supersession() {
        let root = std::env::temp_dir().join("flistwalker-public-action-superseded");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create root");
        let target = root.join("target.txt");
        fs::write(&target, "target").expect("write target");
        let request = AuthorizedActionRequest::new(
            703,
            root.clone(),
            vec![target],
            AuthorizedActionMode::Reveal,
        );
        let guard = TestActionGuard::current();
        guard.current.store(false, Ordering::SeqCst);
        let backend = RecordingActionBackend::new(|_| {});

        let report = execute_authorized_action_request(&request, &guard, &backend);

        assert_eq!(report.outcome, AuthorizedActionOutcome::Superseded);
        assert!(backend.calls.lock().expect("calls").is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn tc_164_public_request_cancels_before_the_first_backend_call() {
        let root = std::env::temp_dir().join("flistwalker-public-action-cancel-first");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create root");
        let target = root.join("target.txt");
        fs::write(&target, "target").expect("write target");
        let cancellation = Arc::new(AtomicBool::new(true));
        let request = AuthorizedActionRequest::new_with_cancellation(
            704,
            root.clone(),
            vec![target],
            AuthorizedActionMode::ExecuteOrOpen,
            cancellation,
        );
        let guard = TestActionGuard::current();
        let backend = RecordingActionBackend::new(|_| {});

        let report = execute_authorized_action_request(&request, &guard, &backend);

        assert_eq!(report.outcome, AuthorizedActionOutcome::Canceled);
        assert_eq!(report.completed, 0);
        assert!(backend.calls.lock().expect("calls").is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn tc_164_public_request_reports_partial_when_reauthorization_changes() {
        let root = std::env::temp_dir().join("flistwalker-public-action-partial");
        let _ = fs::remove_dir_all(&root);
        let first_parent = root.join("first");
        let second_parent = root.join("second");
        fs::create_dir_all(&first_parent).expect("create first parent");
        fs::create_dir_all(&second_parent).expect("create second parent");
        let first = first_parent.join("first.txt");
        let second = second_parent.join("second.txt");
        fs::write(&first, "first").expect("write first");
        fs::write(&second, "second").expect("write second");
        let request = AuthorizedActionRequest::new(
            705,
            root.clone(),
            vec![first, second.clone()],
            AuthorizedActionMode::Reveal,
        );
        let guard = TestActionGuard::current();
        let backend = RecordingActionBackend::new(move |call_index| {
            if call_index == 1 {
                fs::remove_file(&second).expect("remove second file");
                fs::create_dir(&second).expect("replace second file with directory");
            }
        });

        let report = execute_authorized_action_request(&request, &guard, &backend);

        assert_eq!(report.outcome, AuthorizedActionOutcome::PartialFailure);
        assert_eq!(report.completed, 1);
        assert_eq!(report.total, 2);
        assert_eq!(backend.calls.lock().expect("calls").len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn tc_164_public_report_keeps_safe_and_backend_diagnostics_separate() {
        let root = std::env::temp_dir().join("flistwalker-public-action-diagnostics");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create root");
        let target = root.join("target.txt");
        fs::write(&target, "target").expect("write target");
        let request = AuthorizedActionRequest::new(
            706,
            root.clone(),
            vec![target],
            AuthorizedActionMode::ExecuteOrOpen,
        );
        let report = execute_authorized_action_request(
            &request,
            &TestActionGuard::current(),
            &FailingActionBackend,
        );

        assert_eq!(report.outcome, AuthorizedActionOutcome::Failed);
        assert_eq!(report.diagnostic.as_deref(), Some("executor failed"));
        assert_eq!(
            report.backend_error.as_deref(),
            Some("recorded backend failure with actionable detail")
        );
        let _ = fs::remove_dir_all(&root);
    }
}
