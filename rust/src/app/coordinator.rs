use super::*;
use memory_stats::memory_stats;

#[derive(Debug, Clone)]
pub(super) struct StatusLineContext<'a> {
    pub(super) active_tab: usize,
    pub(super) tab_count: usize,
    pub(super) indexed_count: usize,
    pub(super) results_len: usize,
    pub(super) limit: usize,
    pub(super) pinned_paths_len: usize,
    pub(super) search_in_progress: bool,
    pub(super) indexing_in_progress: bool,
    pub(super) action_in_progress: bool,
    pub(super) filelist_in_progress: bool,
    pub(super) filelist_cancel_requested: bool,
    pub(super) update_in_progress: bool,
    pub(super) sort_in_progress: bool,
    pub(super) history_search_active: bool,
    pub(super) history_search_results_len: usize,
    pub(super) query_history_len: usize,
    pub(super) notice: &'a str,
    pub(super) memory_text: Option<String>,
}

pub(super) fn build_status_line(ctx: StatusLineContext<'_>) -> String {
    let tab_label = if ctx.tab_count == 0 {
        "Tab: 1/1".to_string()
    } else {
        format!("Tab: {}/{}", ctx.active_tab + 1, ctx.tab_count)
    };
    let clip_text = if ctx.results_len >= ctx.limit {
        format!(" (limit {} reached)", ctx.limit)
    } else {
        String::new()
    };
    let pinned = if ctx.pinned_paths_len == 0 {
        String::new()
    } else {
        format!(" | Pinned: {}", ctx.pinned_paths_len)
    };
    let searching = if ctx.search_in_progress {
        " | Searching..."
    } else {
        ""
    };
    let indexing = if ctx.indexing_in_progress {
        " | Indexing..."
    } else {
        ""
    };
    let executing = if ctx.action_in_progress {
        " | Executing..."
    } else {
        ""
    };
    let creating_filelist = if ctx.filelist_in_progress {
        if ctx.filelist_cancel_requested {
            " | Canceling FileList..."
        } else {
            " | Creating FileList..."
        }
    } else {
        ""
    };
    let updating = if ctx.update_in_progress {
        " | Updating..."
    } else {
        ""
    };
    let sorting = if ctx.sort_in_progress {
        " | Sorting..."
    } else {
        ""
    };
    let history_search = if ctx.history_search_active {
        format!(
            " | History search: {}/{}",
            ctx.history_search_results_len, ctx.query_history_len
        )
    } else {
        String::new()
    };
    let notice = if ctx.notice.is_empty() {
        String::new()
    } else {
        format!(" | {}", ctx.notice)
    };
    let memory = match ctx.memory_text {
        Some(mem) => format!(" | Mem: {mem}"),
        None => String::new(),
    };

    format!(
        "{} | Entries: {} | Results: {}{}{}{}{}{}{}{}{}{}{}{}",
        tab_label,
        ctx.indexed_count,
        ctx.results_len,
        clip_text,
        pinned,
        searching,
        indexing,
        executing,
        creating_filelist,
        updating,
        sorting,
        history_search,
        memory,
        notice
    )
}

pub(super) fn normalized_compare_key(path: &Path) -> String {
    let mut key = normalize_windows_path_buf(path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/");
    while key.len() > 1 && key.ends_with('/') {
        key.pop();
    }
    #[cfg(windows)]
    {
        key.make_ascii_lowercase();
    }
    key
}

pub(super) fn path_is_within_root(root: &Path, path: &Path) -> bool {
    let root_key = normalized_compare_key(root);
    let path_key = normalized_compare_key(path);
    if path_key == root_key
        || path_key
            .strip_prefix(&root_key)
            .is_some_and(|suffix| suffix.starts_with('/'))
    {
        return true;
    }

    let canonical_root = root.canonicalize().ok();
    let canonical_path = path.canonicalize().ok();
    match (canonical_root, canonical_path) {
        (Some(canonical_root), Some(canonical_path)) => {
            let root_key = normalized_compare_key(&canonical_root);
            let path_key = normalized_compare_key(&canonical_path);
            path_key == root_key
                || path_key
                    .strip_prefix(&root_key)
                    .is_some_and(|suffix| suffix.starts_with('/'))
        }
        _ => false,
    }
}

impl FlistWalkerApp {
    /// 現在の進行状況と notice から status line を再構築する。
    pub(super) fn refresh_status_line(&mut self) {
        let indexed_count = if self.indexing.in_progress {
            if self.index.entries.is_empty() {
                self.all_entries.len()
            } else {
                self.index.entries.len()
            }
        } else {
            self.all_entries.len()
        };
        let memory = self.memory_usage_text();
        self.status_line = build_status_line(StatusLineContext {
            active_tab: self.tabs.active_tab,
            tab_count: self.tabs.len(),
            indexed_count,
            results_len: self.results.len(),
            limit: self.limit,
            pinned_paths_len: self.pinned_paths.len(),
            search_in_progress: self.search.in_progress(),
            indexing_in_progress: self.indexing.in_progress,
            action_in_progress: self.worker_bus.action.in_progress,
            filelist_in_progress: self.filelist_state.in_progress,
            filelist_cancel_requested: self.filelist_state.cancel_requested,
            update_in_progress: self.update_state.in_progress,
            sort_in_progress: self.worker_bus.sort.in_progress,
            history_search_active: self.query_state.history_search_active,
            history_search_results_len: self.query_state.history_search_results.len(),
            query_history_len: self.query_state.query_history.len(),
            notice: &self.notice,
            memory_text: memory,
        });
    }

    /// 定期的にメモリ使用量をサンプリングし表示文字列へ変換する。
    fn memory_usage_text(&mut self) -> Option<String> {
        if self.ui.memory_usage_bytes.is_none()
            || self.ui.last_memory_sample.elapsed() >= Self::MEMORY_SAMPLE_INTERVAL
        {
            self.ui.last_memory_sample = Instant::now();
            self.ui.memory_usage_bytes = memory_stats().map(|stats| stats.physical_mem as u64);
        }
        self.ui
            .memory_usage_bytes
            .map(|bytes| format!("{:.1} MiB", bytes as f64 / 1024.0 / 1024.0))
    }

    /// notice を更新し status line と同期する。
    pub(super) fn set_notice(&mut self, notice: impl Into<String>) {
        self.notice = notice.into();
        self.refresh_status_line();
    }

    /// notice を消去し status line を再計算する。
    pub(super) fn clear_notice(&mut self) {
        self.notice.clear();
        self.refresh_status_line();
    }

    /// action worker 実行中の進捗ラベルを返す。
    pub(super) fn action_progress_label(&self) -> Option<&'static str> {
        if self.worker_bus.action.in_progress {
            Some("Opening...")
        } else {
            None
        }
    }

    /// clipboard 向けの複数 path 文字列を構築する。
    pub(super) fn clipboard_paths_text(paths: &[PathBuf]) -> String {
        paths
            .iter()
            .map(|p| normalize_path_for_display(p))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 現在の index source を status 向け文言へ整形する。
    pub(super) fn source_text(&self) -> String {
        match &self.index.source {
            IndexSource::FileList(path) => format!(
                "Source: FileList ({})",
                path.file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("FileList.txt")
            ),
            IndexSource::Walker => "Source: Walker".to_string(),
            IndexSource::None => "Source: None".to_string(),
        }
    }

    pub(super) fn run_update_cycle(&mut self, ctx: &egui::Context) -> bool {
        self.poll_runtime_events();
        if self.request_viewport_close_if_needed(ctx) {
            return false;
        }
        self.commit_query_history_if_needed(false);
        self.schedule_frame_repaint(ctx);
        self.run_ui_frame(ctx);
        true
    }
}

impl eframe::App for FlistWalkerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.run_update_cycle(ctx) {
            return;
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.persist_state_and_shutdown("app exit");
    }
}

impl Drop for FlistWalkerApp {
    fn drop(&mut self) {
        self.persist_state_and_shutdown("drop fallback");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn build_status_line_includes_progress_and_notice() {
        let status = build_status_line(StatusLineContext {
            active_tab: 1,
            tab_count: 3,
            indexed_count: 42,
            results_len: 7,
            limit: 10,
            pinned_paths_len: 2,
            search_in_progress: true,
            indexing_in_progress: true,
            action_in_progress: false,
            filelist_in_progress: true,
            filelist_cancel_requested: true,
            update_in_progress: false,
            sort_in_progress: true,
            history_search_active: true,
            history_search_results_len: 4,
            query_history_len: 12,
            notice: "hello",
            memory_text: Some("123.4 MiB".to_string()),
        });

        assert!(status.contains("Tab: 2/3"));
        assert!(status.contains("Entries: 42"));
        assert!(status.contains("Results: 7"));
        assert!(status.contains("Pinned: 2"));
        assert!(status.contains("Searching..."));
        assert!(status.contains("Indexing..."));
        assert!(status.contains("Canceling FileList..."));
        assert!(status.contains("Sorting..."));
        assert!(status.contains("History search: 4/12"));
        assert!(status.contains("Mem: 123.4 MiB"));
        assert!(status.contains("hello"));
    }

    #[test]
    fn path_guard_accepts_descendants_and_rejects_outside_paths() {
        let root = PathBuf::from("/tmp/work/root");
        let inside = PathBuf::from("/tmp/work/root/sub/file.txt");
        let outside = PathBuf::from("/tmp/work/other/file.txt");

        assert!(path_is_within_root(&root, &inside));
        assert!(!path_is_within_root(&root, &outside));
    }
}
