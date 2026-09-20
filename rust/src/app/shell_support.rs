use super::{
    egui, lexical_action_path_precheck, ActionPathPrecheck, Entry, EntryKind, FlistWalkerApp,
    IndexSource, PathBuf,
};
#[cfg(not(test))]
use crate::actions::open_text_file_with_default_or_editor;
use crate::path_utils::normalize_windows_path_buf;
use crate::runtime_config::{
    ensure_runtime_config_file_at, legacy_settings_base_dirs, migrate_file_if_needed,
    runtime_config_file_path, settings_base_dir,
};
use anyhow::{Context, Result};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

static PROCESS_SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);
static WINDOW_TRACE_SINK: OnceLock<WindowTraceSink> = OnceLock::new();

enum WindowTraceCommand {
    Append { event: String, details: String },
    Shutdown(mpsc::Sender<()>),
}

struct WindowTraceSink {
    tx: SyncSender<WindowTraceCommand>,
    handle: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl WindowTraceSink {
    fn spawn_with_capacity(capacity: usize, writer: impl Fn(&str, &str) + Send + 'static) -> Self {
        let (tx, rx) = mpsc::sync_channel(capacity.max(1));
        let handle = std::thread::Builder::new()
            .name("flistwalker-window-trace".to_string())
            .spawn(move || {
                while let Ok(command) = rx.recv() {
                    match command {
                        WindowTraceCommand::Append { event, details } => writer(&event, &details),
                        WindowTraceCommand::Shutdown(reply) => {
                            let _ = reply.send(());
                            break;
                        }
                    }
                }
            })
            .expect("spawn window trace worker");
        Self {
            tx,
            handle: Mutex::new(Some(handle)),
        }
    }

    fn try_append(&self, event: &str, details: &str) {
        match self.tx.try_send(WindowTraceCommand::Append {
            event: event.to_string(),
            details: details.to_string(),
        }) {
            Ok(()) | Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
        }
    }

    fn send_control(
        &self,
        mut command: WindowTraceCommand,
        deadline: Instant,
    ) -> Result<(), String> {
        loop {
            match self.tx.try_send(command) {
                Ok(()) => return Ok(()),
                Err(TrySendError::Full(returned)) if Instant::now() < deadline => {
                    command = returned;
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(TrySendError::Full(_)) => {
                    return Err("window trace control enqueue timed out".to_string())
                }
                Err(TrySendError::Disconnected(_)) => {
                    return Err("window trace worker is unavailable".to_string())
                }
            }
        }
    }

    fn shutdown(&self, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        let (tx, rx) = mpsc::channel();
        self.send_control(WindowTraceCommand::Shutdown(tx), deadline)?;
        rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|_| "window trace shutdown timed out".to_string())?;
        let Some(handle) = self
            .handle
            .lock()
            .map_err(|_| "window trace worker handle is unavailable".to_string())?
            .take()
        else {
            return Ok(());
        };
        let remaining = deadline.saturating_duration_since(Instant::now());
        let (joined_tx, joined_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = joined_tx.send(handle.join());
        });
        joined_rx
            .recv_timeout(remaining)
            .map_err(|_| "window trace worker join timed out".to_string())?
            .map_err(|_| "window trace worker panicked during shutdown".to_string())
    }
}

fn window_trace_sink() -> &'static WindowTraceSink {
    WINDOW_TRACE_SINK.get_or_init(|| {
        WindowTraceSink::spawn_with_capacity(256, FlistWalkerApp::write_window_trace_event)
    })
}

pub fn request_process_shutdown() {
    PROCESS_SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
}

pub(crate) fn process_shutdown_requested() -> bool {
    PROCESS_SHUTDOWN_REQUESTED.load(Ordering::SeqCst)
}

#[cfg(test)]
pub(crate) fn clear_process_shutdown_request() {
    PROCESS_SHUTDOWN_REQUESTED.store(false, Ordering::SeqCst);
}

pub fn configure_egui_fonts(ctx: &egui::Context) {
    ctx.set_fonts(egui::FontDefinitions::default());
    begin_async_cjk_font_load(ctx);
}

fn font_definitions_with_cjk(font_bytes: Vec<u8>) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    let font_name = "cjk_ui".to_string();
    fonts.font_data.insert(
        font_name.clone(),
        egui::FontData::from_owned(font_bytes).into(),
    );
    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        family.insert(0, font_name.clone());
    }
    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        family.push(font_name);
    }
    fonts
}

enum CjkFontLoadState {
    Idle,
    Loading,
    Ready {
        bytes: Vec<u8>,
        load_elapsed_ms: f64,
    },
    Unavailable,
}

fn cjk_font_load_state() -> &'static Mutex<CjkFontLoadState> {
    static STATE: OnceLock<Mutex<CjkFontLoadState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(CjkFontLoadState::Idle))
}

fn begin_async_cjk_font_load(ctx: &egui::Context) {
    {
        let Ok(mut guard) = cjk_font_load_state().lock() else {
            return;
        };
        match *guard {
            CjkFontLoadState::Idle => {
                *guard = CjkFontLoadState::Loading;
            }
            CjkFontLoadState::Ready { ref bytes, .. } => {
                ctx.set_fonts(font_definitions_with_cjk(bytes.clone()));
                return;
            }
            CjkFontLoadState::Loading | CjkFontLoadState::Unavailable => return,
        }
    }

    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let load_start = Instant::now();
        let loaded = load_cjk_font_bytes();
        let load_elapsed_ms = load_start.elapsed().as_secs_f64() * 1000.0;
        if let Ok(mut guard) = cjk_font_load_state().lock() {
            *guard = match loaded {
                Some(bytes) => {
                    FlistWalkerApp::trace_window_event(
                        "startup_phase",
                        &format!(
                            "phase=cjk_font_loaded elapsed_ms={:.3} bytes={}",
                            load_elapsed_ms,
                            bytes.len()
                        ),
                    );
                    CjkFontLoadState::Ready {
                        bytes,
                        load_elapsed_ms,
                    }
                }
                None => CjkFontLoadState::Unavailable,
            };
        }
        ctx.request_repaint();
    });
}

impl FlistWalkerApp {
    pub(super) fn maybe_apply_pending_cjk_font(&mut self, ctx: &egui::Context) {
        if self.shell.ui.cjk_font_applied {
            return;
        }
        let Ok(guard) = cjk_font_load_state().lock() else {
            return;
        };
        let CjkFontLoadState::Ready {
            bytes,
            load_elapsed_ms,
        } = &*guard
        else {
            return;
        };
        ctx.set_fonts(font_definitions_with_cjk(bytes.clone()));
        self.shell.ui.cjk_font_applied = true;
        Self::trace_window_event(
            "startup_phase",
            &format!("phase=cjk_font_applied font_load_ms={load_elapsed_ms:.3}"),
        );
        ctx.request_repaint();
    }

    #[cfg(test)]
    pub(super) fn set_cjk_font_ready_for_test(bytes: Vec<u8>) {
        if let Ok(mut guard) = cjk_font_load_state().lock() {
            *guard = CjkFontLoadState::Ready {
                bytes,
                load_elapsed_ms: 0.0,
            };
        }
    }

    #[cfg(test)]
    pub(super) fn reset_cjk_font_state_for_test() {
        if let Ok(mut guard) = cjk_font_load_state().lock() {
            if !matches!(*guard, CjkFontLoadState::Loading) {
                *guard = CjkFontLoadState::Idle;
            }
        }
    }
}

pub(super) fn load_cjk_font_bytes() -> Option<Vec<u8>> {
    let mut candidates: Vec<&str> = Vec::new();

    #[cfg(windows)]
    {
        candidates.extend([
            r"C:\Windows\Fonts\YuGothR.ttc",
            r"C:\Windows\Fonts\YuGothM.ttc",
            r"C:\Windows\Fonts\meiryo.ttc",
            r"C:\Windows\Fonts\msgothic.ttc",
            r"C:\Windows\Fonts\MSYH.TTC",
        ]);
    }

    #[cfg(target_os = "macos")]
    {
        candidates.extend([
            "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
            "/System/Library/Fonts/ヒラギノ丸ゴ ProN W4.ttc",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
        ]);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        candidates.extend([
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansJP-Regular.otf",
            "/usr/share/fonts/truetype/noto/NotoSansJP-Regular.otf",
        ]);
    }

    candidates.into_iter().find_map(|path| fs::read(path).ok())
}

impl FlistWalkerApp {
    pub fn trace_window_event(event: &str, details: &str) {
        Self::append_window_trace(event, details);
    }

    pub(super) fn window_trace_enabled() -> bool {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        *ENABLED.get_or_init(|| {
            std::env::var("FLISTWALKER_WINDOW_TRACE")
                .map(|v| {
                    !(v == "0" || v.eq_ignore_ascii_case("false") || v.eq_ignore_ascii_case("off"))
                })
                .unwrap_or(false)
        })
    }

    pub(super) fn window_trace_verbose_enabled() -> bool {
        static VERBOSE: OnceLock<bool> = OnceLock::new();
        *VERBOSE.get_or_init(|| {
            std::env::var("FLISTWALKER_WINDOW_TRACE_VERBOSE")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("on"))
                .unwrap_or(false)
        })
    }

    pub(super) fn query_trace_summary(query: &str) -> String {
        format!(
            "chars={} has_half_space={} has_full_space={}",
            query.chars().count(),
            query.contains(' '),
            query.contains('\u{3000}')
        )
    }

    pub(super) fn window_trace_path() -> Option<PathBuf> {
        if let Some(path) = std::env::var_os("FLISTWALKER_WINDOW_TRACE_PATH") {
            let path = PathBuf::from(path);
            if !path.as_os_str().is_empty() {
                return Some(path);
            }
        }
        let current = settings_base_dir().map(|base| Self::window_trace_path_in(&base))?;
        let legacy = legacy_settings_base_dirs()
            .into_iter()
            .map(|base| Self::window_trace_path_in(&base))
            .collect::<Vec<_>>();
        Some(Self::migrate_or_legacy_window_trace_path(current, &legacy))
    }

    fn window_trace_path_in(base: &Path) -> PathBuf {
        base.join(".flistwalker_window_trace.log")
    }

    fn migrate_or_legacy_window_trace_path(
        current_path: PathBuf,
        legacy_paths: &[PathBuf],
    ) -> PathBuf {
        if current_path.exists() {
            return current_path;
        }
        for legacy_path in legacy_paths {
            if migrate_file_if_needed(&current_path, legacy_path) {
                return current_path;
            }
        }
        for legacy_path in legacy_paths {
            if legacy_path.exists() {
                return legacy_path.to_path_buf();
            }
        }
        current_path
    }

    pub(super) fn append_window_trace(event: &str, details: &str) {
        if !Self::window_trace_enabled() {
            return;
        }
        window_trace_sink().try_append(event, details);
    }

    pub(super) fn shutdown_window_trace(timeout: Duration) {
        if let Some(sink) = WINDOW_TRACE_SINK.get() {
            let _ = sink.shutdown(timeout);
        }
    }

    fn write_window_trace_event(event: &str, details: &str) {
        let Some(path) = Self::window_trace_path() else {
            return;
        };
        let details = if event == "app_initialized" && details.is_empty() {
            format!("path={}", path.display())
        } else {
            details.to_string()
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default();
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "ts={} event={} {}", ts, event, details);
        }
    }

    pub(super) fn history_persist_disabled() -> bool {
        crate::persistence::history_persist_disabled()
    }

    pub(super) fn open_runtime_config_file(&mut self) {
        let tab_id = self.current_tab_id().unwrap_or_default();
        match self.shell.worker_bus.config_open.start(tab_id) {
            Ok(()) => self.set_notice("Opening config file..."),
            Err(error) => self.set_notice(error),
        }
    }

    pub(super) fn poll_config_open_response(&mut self) {
        let Some((tab_id, result)) = self.shell.worker_bus.config_open.poll() else {
            return;
        };
        self.apply_config_open_completion(tab_id, result);
    }

    fn apply_config_open_completion(
        &mut self,
        tab_id: u64,
        result: std::result::Result<PathBuf, String>,
    ) {
        let notice = match result {
            Ok(path) => format!(
                "Config file opened: {}",
                normalize_windows_path_buf(path).to_string_lossy()
            ),
            Err(error) => format!("Config file open failed: {error}"),
        };
        if Some(tab_id) == self.current_tab_id() {
            self.set_notice(notice);
        } else if let Some(index) = self.find_tab_index_by_id(tab_id) {
            if let Some(tab) = self.shell.tabs.get_mut(index) {
                tab.notice = notice;
            }
        }
    }

    pub(super) fn prepare_and_open_runtime_config() -> Result<PathBuf> {
        #[cfg(not(test))]
        {
            Self::open_runtime_config_file_with(open_text_file_with_default_or_editor)
        }
        #[cfg(test)]
        {
            Err(anyhow::anyhow!(
                "config opener uses a recording backend in tests"
            ))
        }
    }

    fn open_runtime_config_file_with(opener: impl FnOnce(&Path) -> Result<()>) -> Result<PathBuf> {
        let path = Self::ensure_runtime_config_file()?;
        opener(&path)?;
        Ok(path)
    }

    fn ensure_runtime_config_file() -> Result<PathBuf> {
        // Opening settings never reloads process-wide environment on a live GUI.
        let config = crate::runtime_config::current_runtime_config();
        let path = runtime_config_file_path().context("runtime config path is unavailable")?;
        ensure_runtime_config_file_at(&path, &config)?;
        Ok(path)
    }

    pub(super) fn first_action_path_outside_root(&self, paths: &[PathBuf]) -> Option<PathBuf> {
        paths
            .iter()
            .find(|path| {
                lexical_action_path_precheck(&self.shell.runtime.root, path)
                    == ActionPathPrecheck::Reject
            })
            .cloned()
    }

    pub(super) fn root_display_text(&self) -> String {
        normalize_windows_path_buf(self.shell.runtime.root.clone())
            .to_string_lossy()
            .to_string()
    }

    pub(super) fn prefer_relative_display(&self) -> bool {
        matches!(
            self.shell.indexing.build.index.source,
            IndexSource::Walker | IndexSource::FileList(_)
        )
    }

    pub(super) fn prefer_relative_display_for(source: &IndexSource) -> bool {
        matches!(source, IndexSource::Walker | IndexSource::FileList(_))
    }

    pub(super) fn use_filelist_requires_locked_filters(&self) -> bool {
        self.shell.runtime.use_filelist
            && !matches!(self.shell.indexing.build.index.source, IndexSource::Walker)
    }

    pub(super) fn compiled_ignore_terms(
        &mut self,
    ) -> Option<std::sync::Arc<crate::query::CompiledIgnoreTerms>> {
        if !self.shell.ui.ignore_list_enabled || self.shell.runtime.ignore_list_terms.is_empty() {
            return None;
        }
        Some(self.shell.cache.ignore_matcher.compiled(
            self.shell.runtime.ignore_list_terms.as_slice(),
            self.shell.runtime.ignore_case,
        ))
    }

    pub(super) fn is_entry_visible_for_current_filter(
        &self,
        entry: &Entry,
        ignore_terms: Option<&crate::query::CompiledIgnoreTerms>,
    ) -> bool {
        if ignore_terms.is_some_and(|compiled| {
            compiled.matches_path(
                entry.path(),
                crate::query::QueryScope {
                    root: Some(&self.shell.runtime.root),
                    prefer_relative: self.prefer_relative_display(),
                    ignore_case: self.shell.runtime.ignore_case,
                },
            )
        }) {
            return false;
        }
        let kind = self.find_entry_kind(entry.path()).or(entry.kind);
        match kind {
            Some(kind) => Entry::new(entry.path.clone(), Some(kind)).is_visible_for_flags(
                self.shell.runtime.include_files,
                self.shell.runtime.include_dirs,
            ),
            None => self.shell.runtime.include_files && self.shell.runtime.include_dirs,
        }
    }

    // Regression Guard (v0.16.0):
    // DO NOT invoke `set_entry_kind_in_arc_batch` or `Arc::make_mut` here.
    // Iterating and cloning all elements in the 500k+ `entries` arrays for every 512-item batch
    // from the background worker locks up the main frame loop entirely. All kinds are now fetched
    // lazily/reactively via `self.shell.indexing.build.entry_kind_cache` specifically to avoid UI freezes.
    pub(super) fn apply_entry_kind_updates(&mut self, updates: &[(PathBuf, EntryKind)]) {
        if updates.is_empty() {
            return;
        }
        for (path, kind) in updates {
            self.shell
                .indexing
                .build
                .entry_kind_cache
                .set(path.clone(), *kind);
        }
    }

    pub(super) fn find_entry_kind(&self, path: &Path) -> Option<EntryKind> {
        self.shell.indexing.build.entry_kind_cache.get(path)
    }

    #[cfg(test)]
    pub(super) fn set_entry_kind(&mut self, path: &Path, kind: EntryKind) {
        self.apply_entry_kind_updates(&[(path.to_path_buf(), kind)]);
    }

    #[cfg(test)]
    pub(super) fn worker_join_timeout() -> Duration {
        Self::WORKER_JOIN_TIMEOUT
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        env::temp_dir().join(format!("flistwalker-shell-{name}-{nonce}"))
    }

    #[test]
    fn window_trace_path_in_joins_base_directory() {
        let base = PathBuf::from("/tmp/flistwalker-settings");
        assert_eq!(
            FlistWalkerApp::window_trace_path_in(&base),
            base.join(".flistwalker_window_trace.log")
        );
    }

    #[test]
    fn query_trace_summary_reports_shape_without_query_contents() {
        let summary = FlistWalkerApp::query_trace_summary("alpha 日本");

        assert_eq!(summary, "chars=8 has_half_space=true has_full_space=false");
        assert!(!summary.contains("alpha"));
        assert!(!summary.contains('日'));
    }

    #[test]
    fn tc_120_trace_sink_enqueue_does_not_wait_for_blocked_writer() {
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let writes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let writer_writes = std::sync::Arc::clone(&writes);
        let observed = std::sync::Arc::new(Mutex::new(Vec::new()));
        let writer_observed = std::sync::Arc::clone(&observed);
        let sink = WindowTraceSink::spawn_with_capacity(1, move |event, details| {
            if writer_writes.fetch_add(1, Ordering::SeqCst) == 0 {
                let _ = entered_tx.send(());
                let _ = release_rx.recv();
            }
            writer_observed
                .lock()
                .expect("observed trace lock")
                .push((event.to_string(), details.to_string()));
        });

        sink.try_append("first", "one");
        entered_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("writer entered");
        sink.try_append("second", "two");
        let started = Instant::now();
        sink.try_append("dropped", "three");
        assert!(started.elapsed() < Duration::from_millis(50));

        release_tx.send(()).expect("release writer");
        sink.shutdown(Duration::from_secs(1))
            .expect("shutdown trace sink");
        assert_eq!(writes.load(Ordering::SeqCst), 2);
        assert_eq!(
            *observed.lock().expect("observed trace lock"),
            vec![
                ("first".to_string(), "one".to_string()),
                ("second".to_string(), "two".to_string())
            ]
        );
    }

    #[test]
    fn migrate_or_legacy_window_trace_path_moves_legacy_when_current_missing() {
        let base = temp_dir("trace");
        let legacy_base = base.join("legacy");
        let current_base = base.join("current");
        fs::create_dir_all(&legacy_base).expect("create legacy");
        fs::create_dir_all(&current_base).expect("create current");
        let current_path = FlistWalkerApp::window_trace_path_in(&current_base);
        let legacy_path = FlistWalkerApp::window_trace_path_in(&legacy_base);
        fs::write(&legacy_path, "legacy-trace").expect("write legacy");

        let resolved = FlistWalkerApp::migrate_or_legacy_window_trace_path(
            current_path.clone(),
            std::slice::from_ref(&legacy_path),
        );
        assert_eq!(resolved, current_path);
        assert!(current_path.exists());
        assert!(!legacy_path.exists());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn open_runtime_config_file_uses_resolved_config_path() {
        let settings_root = temp_dir("config-open");
        let previous_home = env::var_os("HOME");
        let previous_userprofile = env::var_os("USERPROFILE");
        let previous_localappdata = env::var_os("LOCALAPPDATA");
        let previous_appdata = env::var_os("APPDATA");
        env::set_var("HOME", &settings_root);
        env::set_var("USERPROFILE", &settings_root);
        env::set_var("LOCALAPPDATA", &settings_root);
        env::set_var("APPDATA", &settings_root);

        let mut opened_path = None;
        let result = FlistWalkerApp::open_runtime_config_file_with(|path| {
            opened_path = Some(path.to_path_buf());
            Ok(())
        });

        if let Some(value) = previous_home {
            env::set_var("HOME", value);
        } else {
            env::remove_var("HOME");
        }
        if let Some(value) = previous_userprofile {
            env::set_var("USERPROFILE", value);
        } else {
            env::remove_var("USERPROFILE");
        }
        if let Some(value) = previous_localappdata {
            env::set_var("LOCALAPPDATA", value);
        } else {
            env::remove_var("LOCALAPPDATA");
        }
        if let Some(value) = previous_appdata {
            env::set_var("APPDATA", value);
        } else {
            env::remove_var("APPDATA");
        }

        let opened_path = opened_path.expect("opened path");
        result.expect("open runtime config");
        assert!(opened_path.exists());
        assert_eq!(
            opened_path.file_name().and_then(|name| name.to_str()),
            Some(crate::runtime_config::RUNTIME_CONFIG_FILE_NAME)
        );

        let _ = fs::remove_dir_all(&settings_root);
    }
    #[test]
    fn config_completion_updates_inactive_owner_notice_without_changing_active_tab() {
        let root = temp_dir("config-owner");
        fs::create_dir_all(&root).unwrap();
        let mut app = FlistWalkerApp::new(root.clone(), 50, String::new());
        let owner = app.current_tab_id().unwrap();
        app.set_notice("Opening config file...");
        app.create_new_tab();
        app.set_notice("current tab notice");
        app.apply_config_open_completion(owner, Err("editor unavailable".into()));
        assert_eq!(app.shell.runtime.notice, "current tab notice");
        assert!(app
            .shell
            .tabs
            .get(0)
            .unwrap()
            .notice
            .contains("editor unavailable"));
        let _ = fs::remove_dir_all(root);
    }
}
