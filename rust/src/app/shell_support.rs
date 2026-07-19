use super::{
    egui, lexical_action_path_precheck, ActionPathPrecheck, Entry, EntryKind, FlistWalkerApp,
    IndexSource, PathBuf, ResultSortMode,
};
use crate::actions::open_text_file_with_default_or_editor;
use crate::path_utils::normalize_windows_path_buf;
use crate::runtime_config::{
    legacy_settings_base_dirs, migrate_file_if_needed, runtime_config_file_path, settings_base_dir,
    RuntimeConfig,
};
use anyhow::{Context, Result};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};
#[cfg(test)]
use std::time::Duration;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

static PROCESS_SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

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
        let Some(path) = Self::window_trace_path() else {
            return;
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
        std::env::var("FLISTWALKER_DISABLE_HISTORY_PERSIST")
            .ok()
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    }

    pub(super) fn open_runtime_config_file(&mut self) {
        match Self::open_runtime_config_file_with(open_text_file_with_default_or_editor) {
            Ok(path) => self.set_notice(format!(
                "Config file opened: {}",
                normalize_windows_path_buf(path).to_string_lossy()
            )),
            Err(err) => self.set_notice(format!("Config file open failed: {err}")),
        }
    }

    fn open_runtime_config_file_with(opener: impl FnOnce(&Path) -> Result<()>) -> Result<PathBuf> {
        let path = Self::ensure_runtime_config_file()?;
        opener(&path)?;
        Ok(path)
    }

    fn ensure_runtime_config_file() -> Result<PathBuf> {
        let config = RuntimeConfig::load_or_seed();
        let path = runtime_config_file_path().context("runtime config path is unavailable")?;
        if !path.exists() {
            config.save_to_path(&path)?;
        }
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

    pub(super) fn clear_root_scoped_entry_state(&mut self) {
        self.shell.runtime.index.entries.clear();
        self.shell.runtime.index.entries.shrink_to_fit();
        self.shell.runtime.index.source = IndexSource::None;
        self.shell.runtime.all_entries = Arc::new(Vec::new());
        self.shell.runtime.entries = Arc::new(Vec::new());
        self.shell.cache.entry_kind.clear();
        self.shell.runtime.base_results.clear();
        self.shell.runtime.base_results.shrink_to_fit();
        self.shell.runtime.results.clear();
        self.shell.runtime.results.shrink_to_fit();
        self.shell.runtime.total_match_count = 0;
        self.shell.indexing.incremental_filtered_entries.clear();
        self.shell
            .indexing
            .incremental_filtered_entries
            .shrink_to_fit();
        self.shell.worker_bus.sort.clear_request();
        self.shell.runtime.result_sort_mode = ResultSortMode::Score;
        self.clear_sort_metadata_cache();
        self.shell.indexing.last_search_snapshot_len = 0;
    }

    pub(super) fn prefer_relative_display(&self) -> bool {
        matches!(
            self.shell.runtime.index.source,
            IndexSource::Walker | IndexSource::FileList(_)
        )
    }

    pub(super) fn prefer_relative_display_for(source: &IndexSource) -> bool {
        matches!(source, IndexSource::Walker | IndexSource::FileList(_))
    }

    pub(super) fn use_filelist_requires_locked_filters(&self) -> bool {
        self.shell.runtime.use_filelist
            && !matches!(self.shell.runtime.index.source, IndexSource::Walker)
    }

    pub(super) fn is_entry_visible_for_flags(
        entry: &Entry,
        include_files: bool,
        include_dirs: bool,
    ) -> bool {
        entry.is_visible_for_flags(include_files, include_dirs)
    }

    pub(super) fn is_entry_visible_for_current_filter(&self, entry: &Entry) -> bool {
        if self.shell.ui.ignore_list_enabled
            && crate::query::path_matches_ignore_terms(
                entry.path(),
                &self.shell.runtime.root,
                self.shell.runtime.ignore_list_terms.as_slice(),
                self.prefer_relative_display(),
                self.shell.runtime.ignore_case,
            )
        {
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
    // lazily/reactively via `self.shell.cache.entry_kind` specifically to avoid UI freezes.
    pub(super) fn apply_entry_kind_updates(&mut self, updates: &[(PathBuf, EntryKind)]) {
        if updates.is_empty() {
            return;
        }
        for (path, kind) in updates {
            self.shell.cache.entry_kind.set(path.clone(), *kind);
        }
    }

    pub(super) fn find_entry_kind(&self, path: &Path) -> Option<EntryKind> {
        self.shell.cache.entry_kind.get(path)
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
}
