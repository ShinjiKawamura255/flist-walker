use crate::runtime_config::{
    read_editable_settings, save_editable_settings, EditableSettings, EditableSettingsSnapshot,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::Arc;
use std::thread;

#[cfg(test)]
use crate::runtime_config::settings_timing::{with_save_timing, Milestone, SaveTiming};

#[cfg(not(test))]
type RequestMessage = (u64, ConfigSettingsRequest);
#[cfg(test)]
type RequestMessage = (u64, ConfigSettingsRequest, Option<SaveTiming>);

pub(in crate::app) enum ConfigSettingsRequest {
    Load(PathBuf),
    Save {
        path: PathBuf,
        baseline: EditableSettingsSnapshot,
        draft: EditableSettings,
    },
}

pub(in crate::app) enum ConfigSettingsCompletion {
    Loaded(Result<EditableSettingsSnapshot, String>),
    Saved(Result<EditableSettingsSnapshot, String>),
}

pub(in crate::app) struct ConfigSettingsService {
    tx: Option<SyncSender<RequestMessage>>,
    rx: Receiver<(u64, ConfigSettingsCompletion)>,
    pending: Option<(u64, bool)>,
    #[cfg(test)]
    observe_save_timing: bool,
    #[cfg(test)]
    last_save_timing: Option<(u64, SaveTiming)>,
}

impl ConfigSettingsService {
    pub(in crate::app) fn spawn(shutdown: Arc<AtomicBool>) -> (Self, thread::JoinHandle<()>) {
        let (tx, requests) = mpsc::sync_channel::<RequestMessage>(1);
        let (responses, rx) = mpsc::sync_channel(1);
        let handle = thread::Builder::new()
            .name("flistwalker-config-settings".into())
            .spawn(move || {
                while let Ok(message) = requests.recv() {
                    #[cfg(not(test))]
                    let (generation, request) = message;
                    #[cfg(test)]
                    let (generation, request, timing) = message;
                    if shutdown.load(Ordering::Acquire) {
                        break;
                    }
                    #[cfg(test)]
                    if let Some(timing) = &timing {
                        timing.mark(Milestone::Pickup);
                    }
                    let completion = match request {
                        ConfigSettingsRequest::Load(path) => ConfigSettingsCompletion::Loaded(
                            read_editable_settings(&path).map_err(|error| format!("{error:#}")),
                        ),
                        ConfigSettingsRequest::Save {
                            path,
                            baseline,
                            draft,
                        } => {
                            let save = || {
                                save_editable_settings(&path, &baseline, &draft)
                                    .map_err(|error| format!("{error:#}"))
                            };
                            #[cfg(test)]
                            let result = with_save_timing(timing.as_ref(), save);
                            #[cfg(not(test))]
                            let result = save();
                            ConfigSettingsCompletion::Saved(result)
                        }
                    };
                    #[cfg(test)]
                    if let Some(timing) = &timing {
                        timing.mark(Milestone::NotifyAttempt);
                    }
                    if responses.try_send((generation, completion)).is_err() {
                        break;
                    }
                    #[cfg(test)]
                    if let Some(timing) = &timing {
                        // This records the return, not the exact instant the receiver could poll.
                        timing.mark(Milestone::NotifySucceeded);
                    }
                }
            })
            .expect("spawn settings worker");
        (
            Self {
                tx: Some(tx),
                rx,
                pending: None,
                #[cfg(test)]
                observe_save_timing: false,
                #[cfg(test)]
                last_save_timing: None,
            },
            handle,
        )
    }

    pub(in crate::app) fn start(
        &mut self,
        generation: u64,
        request: ConfigSettingsRequest,
    ) -> Result<(), &'static str> {
        if self.pending.is_some() {
            return Err("Settings operation is already running");
        }
        let is_save = matches!(request, ConfigSettingsRequest::Save { .. });
        #[cfg(not(test))]
        let message = (generation, request);
        #[cfg(test)]
        let timing = (self.observe_save_timing && is_save).then(SaveTiming::new);
        #[cfg(test)]
        let message = (generation, request, timing.clone());
        self.tx
            .as_ref()
            .ok_or("Settings worker is unavailable")?
            .try_send(message)
            .map_err(|_| "Settings worker is unavailable")?;
        self.pending = Some((generation, is_save));
        #[cfg(test)]
        {
            self.last_save_timing = timing.map(|timing| (generation, timing));
        }
        Ok(())
    }

    pub(in crate::app) fn in_progress(&self) -> bool {
        self.pending.is_some()
    }

    pub(in crate::app) fn poll(&mut self) -> Option<(u64, ConfigSettingsCompletion)> {
        let (generation, is_save) = self.pending?;
        let response = match self.rx.try_recv() {
            Ok(response) => response,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => {
                self.pending = None;
                let error = Err("Settings worker disconnected".into());
                return Some((
                    generation,
                    if is_save {
                        ConfigSettingsCompletion::Saved(error)
                    } else {
                        ConfigSettingsCompletion::Loaded(error)
                    },
                ));
            }
        };
        self.pending = None;
        Some(response)
    }

    #[cfg(test)]
    pub(in crate::app) fn pending_operation_for_test(&self) -> Option<(u64, bool)> {
        self.pending
    }

    #[cfg(test)]
    pub(in crate::app) fn observe_save_timing_for_test(&mut self) {
        self.observe_save_timing = true;
    }

    #[cfg(test)]
    pub(in crate::app) fn describe_save_timing_for_test(
        &self,
        generation: u64,
        deadline: std::time::Instant,
    ) -> Option<String> {
        let (owner, timing) = self.last_save_timing.as_ref()?;
        (*owner == generation).then(|| timing.describe(deadline))
    }

    pub(in crate::app) fn disconnect(&mut self) {
        self.tx = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_settings_requests_are_rejected_and_shutdown_settles() {
        let shutdown = Arc::new(AtomicBool::new(true));
        let (mut service, handle) = ConfigSettingsService::spawn(shutdown);
        service
            .start(1, ConfigSettingsRequest::Load("missing.json".into()))
            .expect("queue first request");
        assert!(service
            .start(2, ConfigSettingsRequest::Load("missing.json".into()))
            .is_err());
        handle.join().expect("shutdown worker");
        let (generation, result) = service.poll().expect("disconnected request settles");
        assert_eq!(generation, 1);
        assert!(matches!(result, ConfigSettingsCompletion::Loaded(Err(_))));
        assert!(!service.in_progress());
    }
}
