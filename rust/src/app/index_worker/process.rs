//! Read-only indexing children with bounded, pollable supervision.
use super::{IndexEntry, IndexRequest, IndexResponse, IndexResponseSink, MailboxResponseSink};
use crate::app::freshness::{observe_file, FileObservation, SnapshotFreshness};
use crate::indexer::IndexSource;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
mod wire;
use wire::{Decoder, Event, Frame, Protocol, Request, Source};
const INTERNAL_FLAG: &str = "--internal-index-worker-v1";
const POLL: Duration = Duration::from_millis(5);
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
static PHYSICAL: AtomicUsize = AtomicUsize::new(0);
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
struct Lease(&'static AtomicUsize);
impl Lease {
    fn acquire(counter: &'static AtomicUsize, limit: usize) -> Result<Self, String> {
        counter
            .try_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < limit).then_some(n + 1)
            })
            .map(|_| Self(counter))
            .map_err(|_| {
                "index process capacity exhausted; termination is pending, retry later".into()
            })
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
struct ReapItem {
    child: Child,
    _lease: Lease,
}
impl ReapItem {
    fn terminate_and_poll(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(Some(_)) => true,
            _ => {
                let _ = self.child.kill();
                matches!(self.child.try_wait(), Ok(Some(_)))
            }
        }
    }
}
// OS-refused termination is retained with its physical lease, including after
// bounded shutdown. A later pool retries these same owners; it cannot overbook.
fn retained_children() -> &'static Mutex<Vec<ReapItem>> {
    static RETAINED: OnceLock<Mutex<Vec<ReapItem>>> = OnceLock::new();
    RETAINED.get_or_init(|| Mutex::new(Vec::with_capacity(4)))
}
struct ReaperState {
    pending: Mutex<Vec<ReapItem>>,
    stopping: AtomicBool,
    active: AtomicUsize,
    retired: AtomicBool,
}
impl ReaperState {
    fn retain_unconfirmed(&self, item: ReapItem) {
        let mut queue = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        if self.retired.load(Ordering::Acquire) {
            retained_children()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(item);
        } else {
            queue.push(item);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProcessOutcome {
    Completed,
    Failed,
}
impl ProcessOutcome {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

struct ChildOwner {
    item: Option<ReapItem>,
    reaper: Arc<ReaperState>,
}
impl ChildOwner {
    fn child(&mut self) -> &mut Child {
        &mut self.item.as_mut().expect("live child owner").child
    }
    fn mark_reaped(&mut self) {
        self.item.take();
    }
}
impl Drop for ChildOwner {
    fn drop(&mut self) {
        if let Some(mut item) = self.item.take() {
            if !item.terminate_and_poll() {
                // The same pending lock orders retirement and late admission.
                // Keep the physical lease even if shutdown already ended reaping.
                self.reaper.retain_unconfirmed(item);
            }
        }
    }
}
pub(super) struct ProcessPool {
    #[cfg(test)]
    executable: Option<PathBuf>,
    reaper: Arc<ReaperState>,
    shutdown: Arc<AtomicBool>,
}
impl ProcessPool {
    pub(super) fn new(shutdown: Arc<AtomicBool>) -> (Arc<Self>, JoinHandle<()>) {
        let retained = std::mem::take(
            &mut *retained_children()
                .lock()
                .unwrap_or_else(|e| e.into_inner()),
        );
        let reaper = Arc::new(ReaperState {
            pending: Mutex::new(retained),
            stopping: AtomicBool::new(false),
            active: AtomicUsize::new(0),
            retired: AtomicBool::new(false),
        });
        let pool = Arc::new(Self {
            #[cfg(test)]
            executable: None,
            reaper: reaper.clone(),
            shutdown: shutdown.clone(),
        });
        let handle = thread::spawn(move || {
            let mut stopping_at = None;
            loop {
                let mut queue = reaper.pending.lock().unwrap_or_else(|e| e.into_inner());
                queue.retain_mut(|item| !item.terminate_and_poll());
                if reaper.stopping.load(Ordering::Acquire) || shutdown.load(Ordering::Acquire) {
                    let start = stopping_at.get_or_insert_with(Instant::now);
                    if queue.is_empty() && reaper.active.load(Ordering::Acquire) == 0 {
                        reaper.retired.store(true, Ordering::Release);
                        break;
                    }
                    if start.elapsed() >= SHUTDOWN_GRACE {
                        reaper.retired.store(true, Ordering::Release);
                        retained_children()
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .append(&mut queue);
                        break;
                    }
                }
                drop(queue);
                thread::sleep(POLL);
            }
        });
        (pool, handle)
    }
    #[cfg(test)]
    pub(super) fn new_with_executable(
        shutdown: Arc<AtomicBool>,
        executable: PathBuf,
    ) -> (Arc<Self>, JoinHandle<()>) {
        let (mut pool, reaper) = Self::new(shutdown);
        Arc::get_mut(&mut pool)
            .expect("newly created exclusive pool")
            .executable = Some(executable);
        (pool, reaper)
    }
    pub(super) fn run(
        &self,
        req: &IndexRequest,
        sink: &MailboxResponseSink,
    ) -> Result<ProcessOutcome, String> {
        let snapshot = serde_json::to_string(&Request::capture(req)).map_err(|e| e.to_string())?;
        if snapshot.len() > 16_384 {
            return Err("index worker settings exceed argument limit".into());
        }
        #[cfg(test)]
        let executable = self
            .executable
            .clone()
            .map(Ok)
            .unwrap_or_else(std::env::current_exe)
            .map_err(|e| e.to_string())?;
        #[cfg(not(test))]
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut command = Command::new(executable);
        command.arg(INTERNAL_FLAG).arg(&req.root).arg(snapshot);
        self.run_command(req, sink, &mut command)
    }
    fn run_command(
        &self,
        req: &IndexRequest,
        sink: &MailboxResponseSink,
        command: &mut Command,
    ) -> Result<ProcessOutcome, String> {
        if self.shutdown.load(Ordering::Acquire) || !sink.request_is_current() {
            return Err("superseded".into());
        }
        let _active = Lease::acquire(&ACTIVE, 2)?;
        let lease = Lease::acquire(&PHYSICAL, 4)?;
        self.reaper.active.fetch_add(1, Ordering::AcqRel);
        struct Running<'a>(&'a ReaperState);
        impl Drop for Running<'_> {
            fn drop(&mut self) {
                self.0.active.fetch_sub(1, Ordering::AcqRel);
            }
        }
        let _running = Running(&self.reaper);
        sink.mailbox.record_worker_phase("Starting index process");
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let child = command
            .spawn()
            .map_err(|e| format!("cannot start index process: {e}"))?;
        // Register cleanup immediately, before any further fallible operation.
        let mut owner = ChildOwner {
            item: Some(ReapItem {
                child,
                _lease: lease,
            }),
            reaper: self.reaper.clone(),
        };
        let stdout = owner
            .child()
            .stdout
            .take()
            .ok_or("index process missing stdout")?;
        let mut output =
            PollableOutput::new(stdout).map_err(|e| format!("cannot poll index process: {e}"))?;
        let mut decoder = Decoder::default();
        let mut protocol = Protocol::new();
        let mut bytes = [0u8; 16_384];
        let mut eof = false;
        let mut exit = None;
        let mut closing_at: Option<Instant> = None;
        loop {
            if self.shutdown.load(Ordering::Acquire) || !sink.request_is_current() {
                return Err("superseded".into());
            }
            while let Some(frame) = decoder.next()? {
                protocol.accept(frame, req, sink)?;
            }
            if !eof {
                match output.read_available(&mut bytes) {
                    Ok(Some(0)) => eof = true,
                    Ok(Some(n)) => {
                        decoder.push(&bytes[..n])?;
                        continue;
                    }
                    Ok(None) => {}
                    Err(e) => return Err(format!("index process output failed: {e}")),
                }
            }
            if exit.is_none() {
                exit = owner
                    .child()
                    .try_wait()
                    .map_err(|e| format!("index process status failed: {e}"))?;
                if exit.is_some() {
                    owner.mark_reaped();
                }
            }
            if (protocol.terminal() || eof || exit.is_some()) && closing_at.is_none() {
                closing_at = Some(Instant::now());
            }
            if closing_at.is_some_and(|start| start.elapsed() >= Duration::from_secs(1)) {
                return Err("index process did not close output and exit after completion".into());
            }
            if let Some(status) = exit {
                if eof {
                    if !decoder.is_empty() {
                        return Err("index process exited with partial IPC frame".into());
                    }
                    if !status.success() {
                        return Err(format!("index process exited unexpectedly: {status}"));
                    }
                    if !protocol.terminal() {
                        return Err("index process exited without terminal response".into());
                    }
                    return protocol.publish_terminal(sink);
                }
            }
            thread::sleep(POLL);
        }
    }
}
impl Drop for ProcessPool {
    fn drop(&mut self) {
        self.reaper.stopping.store(true, Ordering::Release);
    }
}
struct PollableOutput(ChildStdout);
impl PollableOutput {
    fn new(stdout: ChildStdout) -> std::io::Result<Self> {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let fd = stdout.as_raw_fd();
            // SAFETY: fcntl operates on this owned, live pipe descriptor.
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
            {
                return Err(std::io::Error::last_os_error());
            }
        }
        Ok(Self(stdout))
    }
    fn read_available(&mut self, bytes: &mut [u8]) -> std::io::Result<Option<usize>> {
        #[cfg(windows)]
        {
            use std::ffi::c_void;
            use std::os::windows::io::AsRawHandle;
            #[link(name = "kernel32")]
            extern "system" {
                fn PeekNamedPipe(
                    pipe: *mut c_void,
                    buffer: *mut c_void,
                    size: u32,
                    read: *mut u32,
                    available: *mut u32,
                    left: *mut u32,
                ) -> i32;
            }
            let mut available = 0u32;
            // SAFETY: self owns the pipe, available is live, null optional
            // buffers ask Windows only for the number of readable bytes.
            let ok = unsafe {
                PeekNamedPipe(
                    self.0.as_raw_handle(),
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut available,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 {
                let e = std::io::Error::last_os_error();
                return if e.raw_os_error() == Some(109) {
                    Ok(Some(0))
                } else {
                    Err(e)
                };
            }
            if available == 0 {
                return Ok(None);
            }
            let len = bytes.len().min(available as usize);
            self.0.read(&mut bytes[..len]).map(Some)
        }
        #[cfg(unix)]
        match self.0.read(bytes) {
            Ok(n) => Ok(Some(n)),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
}
struct ChildWriter<W> {
    output: W,
    request_id: u64,
    tab_id: u64,
    sequence: u64,
    baseline: Option<(Source, FileObservation)>,
}
impl<W: Write> ChildWriter<W> {
    fn new(output: W, request_id: u64, tab_id: u64) -> Self {
        Self {
            output,
            request_id,
            tab_id,
            sequence: 0,
            baseline: None,
        }
    }

    fn serialize_event(&self, event: Event) -> Result<Vec<u8>, ()> {
        serde_json::to_vec(&Frame {
            version: 1,
            request_id: self.request_id,
            tab_id: self.tab_id,
            sequence: self.sequence,
            event,
        })
        .map_err(|_| ())
    }

    fn write_frame(&mut self, data: &[u8]) -> Result<(), ()> {
        if data.len() > wire::MAX_FRAME {
            return Err(());
        }
        self.sequence = self.sequence.checked_add(1).ok_or(())?;
        self.output
            .write_all(&(data.len() as u32).to_le_bytes())
            .map_err(|_| ())?;
        self.output.write_all(data).map_err(|_| ())?;
        self.output.flush().map_err(|_| ())
    }

    fn begin_data_frame(&self, replace: bool) -> Result<Vec<u8>, ()> {
        let entries = Vec::new();
        let event = if replace {
            Event::ReplaceAll { entries }
        } else {
            Event::Batch { entries }
        };
        let mut data = self.serialize_event(event)?;
        // Keep the canonical v1 envelope and open its trailing entries array.
        // Reject a schema change before publishing an incorrectly framed batch.
        if !data.ends_with(br#""entries":[]}}"#) {
            return Err(());
        }
        data.truncate(data.len() - DATA_FRAME_TRAILER.len());
        Ok(data)
    }

    fn write_entries(&mut self, entries: Vec<IndexEntry>, replace: bool) -> Result<(), ()> {
        if entries.is_empty() && !replace {
            return Ok(());
        }
        let mut data = self.begin_data_frame(replace)?;
        let mut encoded = Vec::new();
        let mut count = 0;
        let mut size = 0;
        for entry in entries {
            encoded.clear();
            serde_json::to_writer(&mut encoded, &wire::Entry::encode(entry).map_err(|_| ())?)
                .map_err(|_| ())?;
            // v1 counts encoded entry bytes only, excluding commas and envelope.
            if count > 0 && (size + encoded.len() > 1024 * 1024 || count == wire::MAX_ENTRIES) {
                data.extend_from_slice(DATA_FRAME_TRAILER);
                self.write_frame(&data)?;
                data = self.begin_data_frame(false)?;
                count = 0;
                size = 0;
            }
            if count > 0 {
                data.push(b',');
            }
            data.extend_from_slice(&encoded);
            count += 1;
            size += encoded.len();
        }
        data.extend_from_slice(DATA_FRAME_TRAILER);
        self.write_frame(&data)
    }
}
const DATA_FRAME_TRAILER: &[u8] = b"]}}";

struct ChildSink {
    writer: Mutex<ChildWriter<std::io::Stdout>>,
}
impl ChildSink {
    fn emit(&self, event: Event) -> Result<(), ()> {
        let mut writer = self.writer.lock().map_err(|_| ())?;
        let data = writer.serialize_event(event)?;
        writer.write_frame(&data)
    }
    fn phase(&self, phase: &str) -> Result<(), ()> {
        self.emit(Event::Phase {
            phase: phase.into(),
        })
    }
    fn snapshot(&self, source: &IndexSource, initial: bool) -> Result<(), ()> {
        let wire = Source::encode(source).map_err(|_| ())?;
        self.phase("Observing FileList freshness")?;
        let current = match source {
            IndexSource::FileList(path) => observe_file(path),
            _ => FileObservation::Unavailable,
        };
        let baseline = {
            let mut writer = self.writer.lock().map_err(|_| ())?;
            if initial {
                writer.baseline = Some((wire.clone(), current.clone()));
            }
            writer
                .baseline
                .as_ref()
                .filter(|(s, _)| s == &wire)
                .map(|(_, b)| b.clone())
                .ok_or(())?
        };
        self.emit(Event::Snapshot {
            source: wire,
            baseline,
            current,
        })
    }
    fn finish(&self, source: IndexSource) -> Result<(), ()> {
        self.snapshot(&source, false)?;
        self.emit(Event::Finished {
            source: Source::encode(&source).map_err(|_| ())?,
        })
    }
    fn entries(&self, entries: Vec<IndexEntry>, replace: bool) -> Result<(), ()> {
        self.writer
            .lock()
            .map_err(|_| ())?
            .write_entries(entries, replace)
    }
}
impl IndexResponseSink for ChildSink {
    fn send(&self, response: IndexResponse) -> Result<(), ()> {
        match response {
            IndexResponse::Started { source, .. } => {
                self.emit(Event::Started {
                    source: Source::encode(&source).map_err(|_| ())?,
                })?;
                self.snapshot(&source, true)?;
                self.phase(match source {
                    IndexSource::FileList(_) => "Reading FileList",
                    IndexSource::Walker => "Walking directory",
                    IndexSource::None => "Indexing",
                })
            }
            IndexResponse::Batch { entries, .. } => self.entries(entries, false),
            IndexResponse::ReplaceAll { entries, .. } => self.entries(entries, true),
            IndexResponse::Truncated { limit, .. } => self.emit(Event::Truncated { limit }),
            _ => Err(()),
        }
    }
}
/// Dispatch before tracing, updater, persisted settings or normal CLI parsing.
pub(crate) fn run_internal_if_requested() -> anyhow::Result<bool> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new(INTERNAL_FLAG)) {
        return Ok(false);
    }
    let root = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow::anyhow!("missing private indexing root"))?,
    );
    let raw = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("missing private indexing request"))?;
    let raw = raw
        .to_str()
        .filter(|s| s.len() <= 16_384)
        .ok_or_else(|| anyhow::anyhow!("invalid private indexing request"))?;
    anyhow::ensure!(
        args.next().is_none(),
        "unexpected private indexing argument"
    );
    let req = serde_json::from_str::<Request>(raw)?
        .into_request(root)
        .map_err(anyhow::Error::msg)?;
    let sink = ChildSink {
        writer: Mutex::new(ChildWriter::new(
            std::io::stdout(),
            req.request_id,
            req.tab_id,
        )),
    };
    let shutdown = AtomicBool::new(false);
    let latest = Mutex::new(HashMap::from([(req.tab_id, req.request_id)]));
    let _ = sink.phase("Resolving root and detecting source");
    match super::build_index_request(&sink, &req, &shutdown, &latest, &|root| {
        std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
    }) {
        Ok(source) => {
            sink.finish(source)
                .map_err(|()| anyhow::anyhow!("private index IPC closed"))?;
        }
        Err(error) => {
            sink.emit(Event::Failed {
                error: error.chars().take(1024).collect(),
            })
            .map_err(|()| anyhow::anyhow!("private index IPC closed"))?;
        }
    }
    Ok(true)
}
#[cfg(test)]
mod tests;
