//! Historical-only observers; this module is registered only under cfg(test).
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, Weak};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;
use crate::entry::Entry;
use crate::indexer::IndexSource;

#[derive(Debug, Clone)]
pub(super) struct CommitWitness {
    pub(super) request_id: u64,
    pub(super) tab_id: u64,
    pub(super) root: std::path::PathBuf,
    pub(super) source: IndexSource,
    pub(super) at: Instant,
    all_entries: Weak<Vec<Entry>>,
}
impl CommitWitness {
    pub(super) fn capture(request_id: u64, tab_id: u64, root: &std::path::Path, source: &IndexSource, all: &Arc<Vec<Entry>>) -> Self {
        Self {request_id,tab_id,root:root.to_path_buf(),source:source.clone(),at:Instant::now(),all_entries:Arc::downgrade(all)}
    }
    pub(super) fn matches(&self, all: &Arc<Vec<Entry>>) -> bool {
        self.all_entries.upgrade().is_some_and(|owned| Arc::ptr_eq(&owned, all))
    }
}
#[derive(Default, Debug, Clone)]
struct JoinProof {
    expected: usize,
    first: Option<(usize, usize, Vec<String>, Vec<String>, Instant)>,
    duplicate: bool,
}
impl JoinProof {
    fn safe(&self) -> bool {
        self.expected > 0 && !self.duplicate && self.first.as_ref().is_some_and(|(joined,total,pending,panicked,_)| *total==self.expected && joined==total && pending.is_empty() && panicked.is_empty())
    }
}
#[derive(Default, Debug)]
struct CellProof {
    runtimes: BTreeMap<usize, (Weak<AtomicBool>, JoinProof)>,
    parsers: BTreeMap<u64, JoinProof>,
    invalid: bool,
}
thread_local! { static CELL: RefCell<Option<Arc<Mutex<CellProof>>>> = const { RefCell::new(None) }; }
fn update(f: impl FnOnce(&mut CellProof)) {
    CELL.with(|cell| { if let Some(p)=cell.borrow().as_ref() { f(&mut p.lock().expect("historical cleanup observation")); } });
}
/// Default off. Only an active historical cell can require retained filesystem ownership.
pub(super) fn cleanup_unproven() -> bool {
    CELL.with(|cell| cell.borrow().as_ref().is_some_and(|proof| {
        let p = proof.lock().expect("historical cleanup observation");
        p.invalid || (p.runtimes.is_empty() && p.parsers.is_empty())
            || !p.runtimes.values().all(|(_,j)|j.safe())
            || !p.parsers.values().all(JoinProof::safe)
    }))
}
pub(super) struct CellGuard(Arc<Mutex<CellProof>>);
impl CellGuard {
    pub(super) fn begin() -> Self {
        let proof=Arc::new(Mutex::new(CellProof::default()));
        CELL.with(|cell| assert!(cell.replace(Some(Arc::clone(&proof))).is_none(),"historical cleanup cell overlap"));
        Self(proof)
    }
    pub(super) fn safe(&self) -> bool {
        let p=self.0.lock().expect("historical cleanup observation");
        !p.invalid && (!p.runtimes.is_empty() || !p.parsers.is_empty()) && p.runtimes.values().all(|(_,j)|j.safe()) && p.parsers.values().all(JoinProof::safe)
    }
    pub(super) fn evidence(&self) -> String { format!("{:?}",self.0.lock().expect("historical cleanup observation")) }
}
impl Drop for CellGuard { fn drop(&mut self) { CELL.with(|cell| { let old=cell.take();assert!(old.is_some_and(|p|Arc::ptr_eq(&p,&self.0)),"historical cleanup owner mismatch"); }); } }
pub(super) fn runtime_created(token:&Arc<AtomicBool>) {
    update(|p| {let key=Arc::as_ptr(token) as usize;if p.runtimes.len()>=32 || p.runtimes.contains_key(&key) {p.invalid=true;return;} p.runtimes.insert(key,(Arc::downgrade(token),JoinProof::default()));});
}
pub(super) fn runtime_expected(token:&Arc<AtomicBool>, count:usize) {
    update(|p| {let key=Arc::as_ptr(token) as usize;if let Some((w,j))=p.runtimes.get_mut(&key) {if w.upgrade().is_some_and(|original|Arc::ptr_eq(&original,token)) {j.expected=count;} else {p.invalid=true;}} else {p.invalid=true;}});
}
pub(super) fn runtime_joined(token:&Arc<AtomicBool>,summary:&super::worker::runtime::WorkerJoinSummary) {
    update(|p| {let key=Arc::as_ptr(token) as usize;let Some((w,j))=p.runtimes.get_mut(&key) else {p.invalid=true;return;};if !w.upgrade().is_some_and(|original|Arc::ptr_eq(&original,token)) {p.invalid=true;return;} if j.first.is_some() {j.duplicate=true;return;} j.first=Some((summary.joined,summary.total,summary.pending.clone(),summary.panicked.clone(),Instant::now()));});
}
pub(super) fn parser_started(count:usize)->u64 {
    static NEXT:AtomicU64=AtomicU64::new(1);let id=NEXT.fetch_add(1,Ordering::Relaxed);
    update(|p| {if count==0 || p.parsers.len()>=32 {p.invalid=true;return;}p.parsers.insert(id,JoinProof{expected:count,..Default::default()});});id
}
pub(super) fn parser_joined(id:u64,expected:usize,joined:usize,panicked:usize,timed_out:bool) {
    // Later Drop after an explicit stop has zero handles: retain the first real result.
    if expected==0 {return;}
    update(|p| {let Some(j)=p.parsers.get_mut(&id) else {p.invalid=true;return;};if j.first.is_some() {j.duplicate=true;return;}j.first=Some((joined,expected,if timed_out {vec!["parser-unjoined".into()]}else{vec![]},if panicked>0{vec![format!("{panicked} parser panic(s)")]}else{vec![]},Instant::now()));});
}
#[test]
fn historical_commit_rejects_different_or_dead_arc() {
    let first=Arc::new(Vec::<Entry>::new());let other=Arc::new(Vec::<Entry>::new());
    let witness=CommitWitness::capture(7,3,std::path::Path::new("owned"),&IndexSource::Walker,&first);
    assert!(witness.matches(&first));assert!(!witness.matches(&other));drop(first);assert!(!witness.matches(&other));
}
#[test]
fn historical_cleanup_requires_real_positive_join_and_retains_first() {
    let guard=CellGuard::begin();let token=Arc::new(AtomicBool::new(false));runtime_created(&token);runtime_expected(&token,2);
    assert!(!guard.safe());
    runtime_joined(&token,&super::worker::runtime::WorkerJoinSummary{joined:1,total:2,pending:vec!["worker".into()],panicked:vec![]});assert!(!guard.safe());
    runtime_joined(&token,&super::worker::runtime::WorkerJoinSummary{joined:2,total:2,pending:vec![],panicked:vec![]});assert!(!guard.safe());
}
#[test]
fn historical_parser_empty_second_stop_cannot_overwrite_timeout() {
    let guard=CellGuard::begin();let id=parser_started(2);parser_joined(id,2,1,0,true);parser_joined(id,0,0,0,false);assert!(!guard.safe());
}
