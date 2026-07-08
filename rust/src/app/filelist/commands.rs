use super::super::{FileListRequest, FlistWalkerApp};
// FileList reducer command surface. FileListManager owns the workflow state,
// and this module bridges those commands back into FlistWalkerApp.
pub(super) enum FileListUiCommand {
    RefreshStatusLine,
    SetNotice(String),
}

pub(super) enum FileListWorkerCommand {
    Start(FileListRequest),
}

pub(super) enum FileListAppCommand {
    RequestIndexRefresh,
    RequestBackgroundIndexRefreshForTab(usize),
    SetUseFileListForTab {
        tab_index: usize,
        use_filelist: bool,
    },
}

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
                FileListCommand::App(FileListAppCommand::RequestIndexRefresh) => {
                    self.request_index_refresh();
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
