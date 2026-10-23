//! Private, versioned IPC. Paths retain native units rather than lossy Unicode.
use super::*;
use crate::entry::EntryDisplayKind;
use crate::runtime_config::RuntimeConfig;
use serde::{Deserialize, Serialize};

pub(super) const MAX_FRAME: usize = 8 * 1024 * 1024;
const MAX_PATH_UNITS: usize = 65_536;
pub(super) const MAX_ENTRIES: usize = 1024;

#[cfg(unix)]
type NativeUnit = u8;
#[cfg(windows)]
type NativeUnit = u16;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(super) struct NativePath(Vec<NativeUnit>);
impl NativePath {
    pub(super) fn encode(path: &Path) -> Result<Self, String> {
        #[cfg(unix)]
        let units = {
            use std::os::unix::ffi::OsStrExt;
            path.as_os_str().as_bytes().to_vec()
        };
        #[cfg(windows)]
        let units = {
            use std::os::windows::ffi::OsStrExt;
            path.as_os_str().encode_wide().collect::<Vec<_>>()
        };
        if units.len() > MAX_PATH_UNITS {
            return Err("index IPC path exceeds native-unit limit".into());
        }
        Ok(Self(units))
    }
    pub(super) fn decode(&self) -> Result<PathBuf, String> {
        if self.0.len() > MAX_PATH_UNITS || self.0.contains(&0) {
            return Err("invalid native IPC path".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            Ok(std::ffi::OsString::from_vec(self.0.clone()).into())
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt;
            Ok(std::ffi::OsString::from_wide(&self.0).into())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "path", rename_all = "snake_case")]
pub(super) enum Source {
    FileList(NativePath),
    Walker,
    None,
}
impl Source {
    pub(super) fn encode(source: &IndexSource) -> Result<Self, String> {
        Ok(match source {
            IndexSource::FileList(p) => Self::FileList(NativePath::encode(p)?),
            IndexSource::Walker => Self::Walker,
            IndexSource::None => Self::None,
        })
    }
    pub(super) fn decode(&self) -> Result<IndexSource, String> {
        Ok(match self {
            Self::FileList(p) => IndexSource::FileList(p.decode()?),
            Self::Walker => IndexSource::Walker,
            Self::None => IndexSource::None,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Settings {
    walker_max_entries: usize,
    walker_metrics: bool,
    #[serde(default)]
    walker_metrics_log_path: String,
    walker_adaptive_initial_limit: Option<usize>,
    walker_adaptive_max_limit: Option<usize>,
}
impl Settings {
    fn capture() -> Self {
        let c = crate::runtime_config::current_runtime_config();
        Self {
            walker_max_entries: c.walker_max_entries,
            walker_metrics: c.developer.walker_metrics,
            walker_metrics_log_path: c.developer.walker_metrics_log_path,
            walker_adaptive_initial_limit: c.developer.walker_adaptive_initial_limit,
            walker_adaptive_max_limit: c.developer.walker_adaptive_max_limit,
        }
    }
    pub(super) fn install(self) -> Result<(), String> {
        if self.walker_max_entries == 0 || self.walker_metrics_log_path.len() > 4096 {
            return Err("invalid index settings snapshot".into());
        }
        let mut c = RuntimeConfig {
            walker_max_entries: self.walker_max_entries,
            ..RuntimeConfig::default()
        };
        c.developer.walker_metrics = self.walker_metrics;
        c.developer.walker_metrics_log_path = self.walker_metrics_log_path;
        c.developer.walker_adaptive_initial_limit = self.walker_adaptive_initial_limit;
        c.developer.walker_adaptive_max_limit = self.walker_adaptive_max_limit;
        crate::runtime_config::set_process_runtime_config(c);
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    request_id: u64,
    tab_id: u64,
    use_filelist: bool,
    include_files: bool,
    include_dirs: bool,
    max_depth: crate::indexer::MaxDepth,
    follow_links: bool,
    complete_walker_snapshot: bool,
    pub(super) settings: Settings,
}
impl Request {
    pub(super) fn capture(r: &IndexRequest) -> Self {
        Self {
            request_id: r.request_id,
            tab_id: r.tab_id,
            use_filelist: r.use_filelist,
            include_files: r.include_files,
            include_dirs: r.include_dirs,
            max_depth: r.max_depth,
            follow_links: r.follow_links,
            complete_walker_snapshot: r.complete_walker_snapshot,
            settings: Settings::capture(),
        }
    }
    pub(super) fn into_request(self, root: PathBuf) -> Result<IndexRequest, String> {
        self.settings.install()?;
        Ok(IndexRequest {
            request_id: self.request_id,
            tab_id: self.tab_id,
            root,
            use_filelist: self.use_filelist,
            include_files: self.include_files,
            include_dirs: self.include_dirs,
            max_depth: self.max_depth,
            follow_links: self.follow_links,
            complete_walker_snapshot: self.complete_walker_snapshot,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Entry {
    pub(super) path: NativePath,
    kind: u8,
    kind_known: bool,
}
impl Entry {
    pub(super) fn encode(e: IndexEntry) -> Result<Self, String> {
        let kind = match e.kind.display {
            EntryDisplayKind::File => 0,
            EntryDisplayKind::Dir => 1,
            EntryDisplayKind::Link => match e.kind.is_dir {
                Some(false) => 2,
                Some(true) => 3,
                None => 4,
            },
            EntryDisplayKind::Other => 5,
        };
        Ok(Self {
            path: NativePath::encode(&e.path)?,
            kind,
            kind_known: e.kind_known,
        })
    }
    fn decode(self) -> Result<IndexEntry, String> {
        let kind = match self.kind {
            0 => crate::entry::EntryKind::file(),
            1 => crate::entry::EntryKind::dir(),
            2 => crate::entry::EntryKind::link(false),
            3 => crate::entry::EntryKind::link(true),
            4 => crate::entry::EntryKind::link_unknown(),
            5 => crate::entry::EntryKind::other(),
            _ => return Err("invalid index IPC entry kind".into()),
        };
        Ok(IndexEntry {
            path: self.path.decode()?,
            kind,
            kind_known: self.kind_known,
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Event {
    Phase {
        phase: String,
    },
    Started {
        source: Source,
    },
    Batch {
        entries: Vec<Entry>,
    },
    ReplaceAll {
        entries: Vec<Entry>,
    },
    Snapshot {
        source: Source,
        baseline: FileObservation,
        current: FileObservation,
    },
    Finished {
        source: Source,
    },
    Failed {
        error: String,
    },
    Truncated {
        limit: usize,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Frame {
    pub(super) version: u32,
    pub(super) request_id: u64,
    pub(super) tab_id: u64,
    pub(super) sequence: u64,
    pub(super) event: Event,
}

#[derive(Default)]
pub(super) struct Decoder {
    bytes: Vec<u8>,
    expected: Option<usize>,
}
impl Decoder {
    pub(super) fn push(&mut self, chunk: &[u8]) -> Result<(), String> {
        if self.bytes.len().saturating_add(chunk.len()) > MAX_FRAME + 4 + 16_384 {
            return Err("index IPC receive buffer limit".into());
        }
        self.bytes.extend_from_slice(chunk);
        Ok(())
    }
    pub(super) fn next(&mut self) -> Result<Option<Frame>, String> {
        if self.expected.is_none() && self.bytes.len() >= 4 {
            let len =
                u32::from_le_bytes(self.bytes[..4].try_into().expect("length checked")) as usize;
            if len == 0 || len > MAX_FRAME {
                return Err("invalid index IPC frame length".into());
            }
            self.expected = Some(len);
        }
        let Some(len) = self.expected else {
            return Ok(None);
        };
        if self.bytes.len() < len + 4 {
            return Ok(None);
        }
        let frame = serde_json::from_slice(&self.bytes[4..len + 4])
            .map_err(|e| format!("invalid index IPC frame: {e}"))?;
        self.bytes.drain(..len + 4);
        self.expected = None;
        Ok(Some(frame))
    }
    pub(super) fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

pub(super) struct Protocol {
    next_sequence: u64,
    source: Option<Source>,
    terminal: bool,
    snapshot: bool,
    pending_terminal: Option<IndexResponse>,
}
impl Protocol {
    pub(super) fn new() -> Self {
        Self {
            next_sequence: 0,
            source: None,
            terminal: false,
            snapshot: false,
            pending_terminal: None,
        }
    }
    pub(super) fn accept(
        &mut self,
        frame: Frame,
        req: &IndexRequest,
        sink: &MailboxResponseSink,
    ) -> Result<(), String> {
        if frame.version != 1
            || frame.request_id != req.request_id
            || frame.tab_id != req.tab_id
            || frame.sequence != self.next_sequence
            || self.terminal
        {
            return Err("index IPC version, identity or ordering mismatch".into());
        }
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or("index IPC sequence overflow")?;
        sink.mailbox.record_worker_activity();
        let id = req.request_id;
        let response = match frame.event {
            Event::Phase { phase } => {
                if phase.len() > 96 || phase.chars().any(char::is_control) {
                    return Err("invalid index IPC phase".into());
                }
                sink.mailbox.record_worker_phase(&phase);
                return Ok(());
            }
            Event::Started { source } => {
                if self.source.is_some() {
                    return Err("duplicate index IPC start".into());
                }
                let decoded = source.decode()?;
                self.source = Some(source);
                tracing::info!(
                    flow = "index",
                    source_kind = super::super::index_source_kind(&decoded),
                    event = "started",
                    request_id = id,
                    tab_id = req.tab_id,
                    "isolated worker request started"
                );
                IndexResponse::Started {
                    request_id: id,
                    source: decoded,
                }
            }
            Event::Batch { entries } | Event::ReplaceAll { entries }
                if self.source.is_none() || entries.len() > MAX_ENTRIES =>
            {
                return Err("index IPC data before start or excess entries".into())
            }
            Event::Batch { entries } => IndexResponse::Batch {
                request_id: id,
                entries: entries
                    .into_iter()
                    .map(Entry::decode)
                    .collect::<Result<_, _>>()?,
            },
            Event::ReplaceAll { entries } => IndexResponse::ReplaceAll {
                request_id: id,
                entries: entries
                    .into_iter()
                    .map(Entry::decode)
                    .collect::<Result<_, _>>()?,
            },
            Event::Snapshot {
                source,
                baseline,
                current,
            } => {
                if self.source.as_ref() != Some(&source) {
                    return Err("index IPC snapshot source mismatch".into());
                }
                sink.mailbox.set_snapshot(SnapshotFreshness::acquired(
                    id,
                    req.root.clone(),
                    source.decode()?,
                    baseline,
                    current,
                ));
                self.snapshot = true;
                return Ok(());
            }
            Event::Finished { source } => {
                if self.source.as_ref() != Some(&source) || !self.snapshot {
                    return Err("index IPC completion without matching snapshot".into());
                }
                self.terminal = true;
                IndexResponse::Finished {
                    request_id: id,
                    source: source.decode()?,
                }
            }
            Event::Failed { error } => {
                if error.len() > 4096 {
                    return Err("index IPC error too long".into());
                }
                self.terminal = true;
                IndexResponse::Failed {
                    request_id: id,
                    error,
                }
            }
            Event::Truncated { limit } => {
                if self.source.is_none() || limit == 0 {
                    return Err("invalid index IPC truncation".into());
                }
                IndexResponse::Truncated {
                    request_id: id,
                    limit,
                }
            }
        };
        if self.terminal {
            self.pending_terminal = Some(response);
            Ok(())
        } else {
            sink.publish(response, false)
                .map_err(|()| "superseded".to_string())?;
            sink.mailbox.record_worker_activity();
            Ok(())
        }
    }
    pub(super) fn terminal(&self) -> bool {
        self.terminal
    }
    pub(super) fn publish_terminal(
        &mut self,
        sink: &MailboxResponseSink,
    ) -> Result<ProcessOutcome, String> {
        let terminal = self
            .pending_terminal
            .take()
            .ok_or("index process missing terminal response")?;
        let outcome = match &terminal {
            IndexResponse::Finished { .. } => ProcessOutcome::Completed,
            _ => ProcessOutcome::Failed,
        };
        sink.publish(terminal, false)
            .map_err(|()| "superseded".to_string())?;
        Ok(outcome)
    }
}
