use super::super::{FileListRequest, FlistWalkerApp, PendingFileListAfterIndex};
// FileList reducer command surface. FileListManager owns the workflow state,
// and this module bridges those commands back into FlistWalkerApp.
#[allow(dead_code)]
pub(super) enum FileListUiCommand {
    RefreshStatusLine,
    SetNotice(String),
}

#[allow(dead_code)]
pub(super) enum FileListWorkerCommand {
    Start(FileListRequest),
}

#[allow(dead_code)]
pub(super) enum FileListAppCommand {
    SetPendingAfterIndex(Option<PendingFileListAfterIndex>),
    SetIncludeFilesAndDirs {
        include_files: bool,
        include_dirs: bool,
    },
    RequestIndexRefresh,
    RequestCreateFileListWalkerRefresh,
    RequestBackgroundIndexRefreshForTab(usize),
    SetUseFileListForTab {
        tab_index: usize,
        use_filelist: bool,
    },
}

#[allow(dead_code)]
pub(super) enum FileListCommand {
    Ui(FileListUiCommand),
    Worker(FileListWorkerCommand),
    App(FileListAppCommand),
}
impl FlistWalkerApp {
    pub(in crate::app::filelist) fn dispatch_filelist_commands(
        &mut self,
        commands: Vec<FileListCommand>,
    ) {
        for command in commands {
            match command {
                FileListCommand::Ui(FileListUiCommand::RefreshStatusLine) => {
                    self.refresh_status_line();
                }
                FileListCommand::Ui(FileListUiCommand::SetNotice(notice)) => {
                    self.set_notice(notice);
                }
                FileListCommand::Worker(FileListWorkerCommand::Start(req)) => {
                    if self.shell.worker_bus.filelist.tx.send(req).is_err() {
                        let fallback = self.shell.features.filelist.send_failure_commands();
                        self.dispatch_filelist_commands(fallback);
                    }
                }
                FileListCommand::App(FileListAppCommand::SetPendingAfterIndex(pending)) => {
                    self.shell.features.filelist.workflow.pending_after_index = pending;
                }
                FileListCommand::App(FileListAppCommand::SetIncludeFilesAndDirs {
                    include_files,
                    include_dirs,
                }) => {
                    self.shell.runtime.include_files = include_files;
                    self.shell.runtime.include_dirs = include_dirs;
                }
                FileListCommand::App(FileListAppCommand::RequestIndexRefresh) => {
                    self.request_index_refresh();
                }
                FileListCommand::App(FileListAppCommand::RequestCreateFileListWalkerRefresh) => {
                    self.request_create_filelist_walker_refresh();
                }
                FileListCommand::App(FileListAppCommand::RequestBackgroundIndexRefreshForTab(
                    tab_index,
                )) => {
                    self.request_background_index_refresh_for_tab(tab_index);
                }
                FileListCommand::App(FileListAppCommand::SetUseFileListForTab {
                    tab_index,
                    use_filelist,
                }) => {
                    if let Some(tab) = self.shell.tabs.get_mut(tab_index) {
                        tab.use_filelist = use_filelist;
                    }
                    if tab_index == self.shell.tabs.active_tab_index() {
                        self.shell.runtime.use_filelist = use_filelist;
                    }
                }
            }
        }
    }
}
