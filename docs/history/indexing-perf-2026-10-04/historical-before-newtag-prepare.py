from pathlib import Path
import subprocess,re,shutil
REPO=Path('/private/tmp/flistwalker-indexing-perf-20261003'); BASE=REPO/'rust/target/indexing-perf-study/newtag-expanded'; CUR=REPO/'rust/target/indexing-perf-study/stale-full-final-source/src'
def orig(tag,file):return subprocess.check_output(['git','show',f'{tag}:rust/src/app/{file}'],cwd=REPO,text=True)
def current(file):return (CUR/'app'/file).read_text()
def replace(s,a,b):
 assert s.count(a)==1,(a[:100],s.count(a));return s.replace(a,b,1)
def span(s,start):
 # Brace scanner ignores quoted strings and line/block comments.
 p=s.index('{',start);depth=0;i=p;quoted=False;escape=False
 while i<len(s):
  c=s[i]
  if quoted:
   if escape:escape=False
   elif c=='\\':escape=True
   elif c=='"':quoted=False
  elif c=='"':quoted=True
  elif s.startswith('//',i):
   i=s.find('\n',i)
   if i<0:raise RuntimeError('unclosed line comment')
   continue
  elif s.startswith('/*',i):
   i=s.index('*/',i+2)+2;continue
  elif c=='{':depth+=1
  elif c=='}':
   depth-=1
   if not depth:return i+1
  i+=1
 raise RuntimeError('unclosed braces')
def item(s,kind,name,attrs=True):
 m=re.search(r'^([ \t]*)(?:pub(?:\([^\n]*?\))? )?'+kind+r' '+re.escape(name)+r'(?:\b|<)',s,re.M);assert m,(kind,name)
 start=m.start();end=span(s,start)
 if attrs:
  while start>0:
   prev=s.rfind('\n',0,start-1)+1;line=s[prev:start].strip()
   if line.startswith('#['):start=prev
   else:break
 return s[start:end]+'\n'
def fn(s,name,attrs=True):return item(s,'fn',name,attrs)
def cfgblock(s,needle):
 p=s.index(needle);start=s.rfind('#[cfg(test)]',0,p);assert start>=0
 start=s.rfind('\n',0,start)+1;i=s.index('\n',s.index('#[cfg(test)]',start))+1
 body=s[i:].lstrip();need_semi=not(body.startswith('if ') or body.startswith('{'))
 levels=[0,0,0];quoted=False;escape=False
 while i<len(s):
  c=s[i]
  if quoted:
   if escape:escape=False
   elif c=='\\':escape=True
   elif c=='"':quoted=False
  elif c=='"':quoted=True
  elif s.startswith('//',i):i=s.index('\n',i);continue
  elif s.startswith('/*',i):i=s.index('*/',i+2)+2;continue
  elif c in '({[':levels['({['.index(c)]+=1
  elif c in ')}]':
   levels[')}]'.index(c)]-=1
   if c=='}' and levels==[0,0,0] and not need_semi:
    rest=s[i+1:].lstrip()
    if not rest.startswith('else'):return s[start:i+1]+'\n'
  elif c==';' and levels==[0,0,0]:return s[start:i+1]+'\n'
  i+=1
 raise RuntimeError('unterminated cfg statement '+needle)
def save(out,file,s):(out/'rust/src/app'/file).write_text(s)
for tag in ['v0.27.0','v0.28.0','v0.29.0','v0.30.0']:
 out=BASE/tag
 shutil.copytree(CUR/'app/tests/indexing_perf',out/'rust/src/app/tests/indexing_perf',dirs_exist_ok=True)
 for p in (out/'rust/src/app/tests/indexing_perf').rglob('*.rs'):
  text=p.read_text()
  if tag in ['v0.27.0','v0.28.0']:text=re.sub(r'^.*\|\| i\.build\.active_filter\.is_some\(\).*\n','',text,flags=re.M)
  if tag!='v0.30.0':text=re.sub(r'^.*filelist_auto_check_enabled = false;\n','',text,flags=re.M)
  if tag=='v0.27.0':text=re.sub(r'^.*complete_walker_snapshot: false,\n','',text,flags=re.M)
  p.write_text(text)
 # Reset changed source files from immutable basic applied snapshot to make preparation reproducible.
 for file in ['index_coordinator.rs','index_mailbox.rs','index_worker.rs','pipeline.rs','pipeline_owner.rs','result_reducer.rs','tab_state.rs','tabs.rs','mod.rs','worker/runtime.rs','tests/support.rs']:
  src=BASE/(tag+'-basic-source')/'app'/file;shutil.copyfile(src,out/'rust/src/app'/file)
 # Rebuild mailbox from original production text + only explicit current cfg observation regions.
 s=orig(tag,'index_mailbox.rs');c=current('index_mailbox.rs')
 types=c[c.index('#[cfg(test)]\n#[derive(Clone, Debug)]'):c.index('#[derive(Default)]\nstruct MailboxState')]
 s=replace(s,'#[derive(Default)]\nstruct MailboxState',types+'#[derive(Default)]\nstruct MailboxState')
 s=replace(s,'    state: Mutex<MailboxState>,\n','    state: Mutex<MailboxState>,\n    #[cfg(test)]\n    perf_enabled: std::sync::atomic::AtomicBool,\n    #[cfg(test)]\n    perf_state: std::sync::OnceLock<IndexPerfHandle>,\n')
 s=replace(s,'            state: Mutex::new(MailboxState::default()),','            state: Mutex::new(MailboxState::default()),\n            #[cfg(test)]\n            perf_enabled: std::sync::atomic::AtomicBool::new(false),\n            #[cfg(test)]\n            perf_state: std::sync::OnceLock::new(),')
 methods=''.join(fn(c,n) for n in ['enable_perf_observation','perf_enabled','perf_handle','perf_observation','update_perf','record_stale_full_data_abort','record_data_publish_end','record_full_wait','record_terminal_offer','record_terminal_send_returned','record_nested_input_reused'])
 anchor='    pub(super) fn try_publish('
 s=replace(s,anchor,methods+'\n'+anchor)
 publish=cfgblock(c,'let mut perf = self')
 s=replace(s,'        let sequenced = SequencedResponse {',publish+'        let sequenced = SequencedResponse {')
 close=cfgblock(c,'p.mailbox_closed = true;')
 s=replace(s,'            state.closed = true;', '            state.closed = true;\n'+close.rstrip())
 if tag=='v0.30.0':s=s.replace('        if let Ok(mut state) = self.state.lock() {\n            state.snapshot = Some(snapshot);','        #[cfg(test)]\n        self.update_perf(|p| p.started_root = Some(snapshot.root.clone()));\n        if let Ok(mut state) = self.state.lock() {\n            state.snapshot = Some(snapshot);',1)
 registry=c[c.index('#[cfg(test)]\npub(super) type IndexPerfHandle'):]
 s+='\n'+registry;save(out,'index_mailbox.rs',s)
 # Coordinator cfg types/fields/helpers and exact operation-site hooks.
 s=(out/'rust/src/app/index_coordinator.rs').read_text();c=current('index_coordinator.rs')
 types=c[c.index('#[cfg(test)]\n#[derive(Clone, Debug)]\npub(super) struct AuxPerfObservation'):c.index('pub(super) struct IndexCoordinator')]
 s=replace(s,'pub(super) struct IndexCoordinator',types+'pub(super) struct IndexCoordinator')
 cfields=c[c.index('pub(super) struct IndexCoordinator'):c.index("#[cfg(test)]\npub(super) struct PerfSearchSortIdentity")]
 fields=re.findall(r'    #\[cfg\(test\)\]\n    pub\(super\) (perf_[\w]+):[^;]+?,\n',cfields)
 for name in fields:
  if re.search(r'pub\(super\) '+name+r':',s):continue
  m=re.search(r'    #\[cfg\(test\)\]\n    pub\(super\) '+name+r':.*?,\n',cfields,re.S);assert m
  s=replace(s,'    pub(super) perf_observe_requests: bool,\n','    pub(super) perf_observe_requests: bool,\n'+m.group())
 for name in fields:
  if re.search(r'            '+name+r':',s):continue
  m=re.search(r'            #\[cfg\(test\)\]\n            '+name+r':[^\n]+\n',c);assert m,name
  s=replace(s,'            perf_observe_requests: false,\n','            perf_observe_requests: false,\n'+m.group())
 identity=item(c,'struct','PerfSearchSortIdentity')
 helpers=''.join(fn(c,n) for n in ['perf_search_sort_owned','perf_aux_dispatch','perf_aux_delivered'])
 s=replace(s,'impl IndexCoordinator {',identity+'\nimpl IndexCoordinator {\n'+helpers)
 allocation=cfgblock(c,'"index allocation observer overflow"')
 s=replace(s,'            mailboxes.insert(request_id, mailbox);',allocation+'            mailboxes.insert(request_id, mailbox);')
 cleanup=fn(s,'cleanup_request',False);hook=cfgblock(c,'self.perf_released_requests\n                .entry(request_id)')
 newcleanup=cleanup.replace('        let tab_id = self.request_tabs.remove(&request_id);',hook+'        let tab_id = self.request_tabs.remove(&request_id);')
 assert newcleanup!=cleanup;s=replace(s,cleanup.rstrip(),newcleanup.rstrip())
 warm=fn(s,'replace_warm_tab',False)
 observation=cfgblock(c,'let observation = if self.perf_observe_history')
 appendevent=cfgblock(c,'drop(latest);\n                    if let Some(observation)')
 nw=warm.replace('                if let Some(request_id)',observation+'                if let Some(request_id)',1)
 nw=nw.replace('                    self.superseded_request_ids.insert(request_id);\n                }','                    self.superseded_request_ids.insert(request_id);\n                }\n'+appendevent.rstrip(),1)
 s=replace(s,warm.rstrip(),nw.rstrip());save(out,'index_coordinator.rs',s)
 print(tag,'mailbox/coordinator cfg preparation complete')
 # Worker observations: preserve tag body and existing basic Full retry instrumentation.
 s=(out/'rust/src/app/index_worker.rs').read_text();c=current('index_worker.rs')
 guards=c[c.index('#[cfg(test)]\nstruct RequestPerfReturnGuard'):c.index('struct MailboxResponseSink')]
 s=replace(s,'struct MailboxResponseSink',guards+'struct MailboxResponseSink')
 s=replace(s,'trait IndexResponseSink {\n    fn send(&self, response: IndexResponse) -> Result<(), ()>;','trait IndexResponseSink {\n    fn send(&self, response: IndexResponse) -> Result<(), ()>;\n    #[cfg(test)]\n    fn observe_nested_input(&self, _reused: bool) {}')
 helper=fn(c,'request_is_current_after_full')
 s=replace(s,'impl MailboxResponseSink {','impl MailboxResponseSink {\n'+helper)
 facade=fn(c,'mailbox_response_sink_for_test')
 if tag!='v0.30.0':facade=facade.replace('        root: req.root.clone(),\n','')
 s=replace(s,'impl IndexResponseSink for MailboxResponseSink {',facade+'\nimpl IndexResponseSink for MailboxResponseSink {\n'+fn(c[c.index('impl IndexResponseSink for MailboxResponseSink'):],'observe_nested_input'))
 prefix=cfgblock(c,'let _terminal_return = if self.mailbox.perf_enabled()')
 # Basic send has data_publish_end already; add terminal offer before the wait instrumentation.
 anchor='        #[cfg(test)]\n        let mut full_wait_started: Option<Instant> = None;'
 s=replace(s,anchor,prefix+anchor)
 s=replace(s,'                    if !self.request_is_current() {','                    #[cfg(test)]\n                    let current = self.request_is_current_after_full(&response);\n                    #[cfg(not(test))]\n                    let current = self.request_is_current();\n                    if !current {')
 if tag!='v0.30.0':
  mb=(out/'rust/src/app/index_mailbox.rs').read_text();mb=replace(mb,'    #[cfg(test)]\n    pub(super) fn record_terminal_send_returned', '    #[cfg(test)]\n    pub(super) fn record_started_root(&self, root: &std::path::Path) {\n        self.update_perf(|p| p.started_root = Some(root.to_path_buf()));\n    }\n    #[cfg(test)]\n    pub(super) fn record_terminal_send_returned');save(out,'index_mailbox.rs',mb)
  s=replace(s,'    fn observe_nested_input(&self, _reused: bool) {}','    fn observe_nested_input(&self, _reused: bool) {}\n    #[cfg(test)]\n    fn observe_started_root(&self, _root: &Path) {}')
  s=replace(s,'impl IndexResponseSink for MailboxResponseSink {','impl IndexResponseSink for MailboxResponseSink {\n    #[cfg(test)]\n    fn observe_started_root(&self, root: &Path) { self.mailbox.record_started_root(root); }')
  for stream in ['stream_filelist_index','stream_walker_index']:
   part=fn(s,stream,False);at=part.index('{')+1;changed=part[:at]+'\n    #[cfg(test)]\n    tx_res.observe_started_root(root);'+part[at:];s=replace(s,part.rstrip(),changed.rstrip())

 s=replace(s,'    let mut final_entries = if let Some(entries) = streamed_entries_for_nested {','    #[cfg(test)]\n    tx_res.observe_nested_input(streamed_entries_for_nested.is_some());\n    let mut final_entries = if let Some(entries) = streamed_entries_for_nested {')
 ret=cfgblock(c,'let mut _processing_return =')
 s=replace(s,'                let mailbox = mailbox_for_dequeued_request(',ret+'                let mailbox = mailbox_for_dequeued_request(')
 skip=cfgblock(c,'if let Some(handle) = &_processing_return.0')
 s=replace(s,'                let Some(mailbox) = mailbox else {\n                    continue;','                let Some(mailbox) = mailbox else {\n'+skip+'                    continue;')
 fallback=cfgblock(c,'if _processing_return.0.is_none()')
 s=replace(s,'                let tx_res_worker = MailboxResponseSink {',fallback+'                let tx_res_worker = MailboxResponseSink {')
 save(out,'index_worker.rs',s)
 print(tag,'worker operation-site cfg preparation complete')
 # Preemption/admission on the original scheduler.
 s=(out/'rust/src/app/pipeline.rs').read_text();c=current('pipeline.rs')
 s=replace(s,'let Some((_, tab_id, replacement_request_id)) = victim','let Some((_victim_request_id, tab_id, replacement_request_id)) = victim')
 event=cfgblock(c,'let perf_event = if self.shell.indexing.perf_observe_history')
 s=replace(s,'        latest.insert(tab_id, replacement_request_id);',event+'        latest.insert(tab_id, replacement_request_id);')
 hook=cfgblock(c,'if let Some(event) = perf_event')
 s=replace(s,'        drop(latest);\n        if replacement_request_id == 0','        drop(latest);\n'+hook+'        if replacement_request_id == 0')
 root=cfgblock(c,'let perf_root = if self.shell.indexing.perf_observe_history')
 s=replace(s,'            match self.shell.indexing.tx.try_send(req) {',root+'            match self.shell.indexing.tx.try_send(req) {')
 admitted=cfgblock(c,'if let Some(allocation) = self')
 s=replace(s,'                Ok(()) => {\n                    super::worker::channel::trace_worker_load(\n                        &self.shell.indexing.tx,','                Ok(()) => {\n'+admitted+'                    super::worker::channel::trace_worker_load(\n                        &self.shell.indexing.tx,')
 save(out,'pipeline.rs',s)
 # Search binding/RX: reuse only cfg blocks; exact original receive/route controls remain.
 s=(out/'rust/src/app/pipeline_owner.rs').read_text();c=current('pipeline_owner.rs')
 binding=cfgblock(c,'self.app.shell.indexing.perf_search_bindings.push')
 dispatch=cfgblock(c,'if req.sort_scope == super::ResultSortScope::AllMatches')
 oldline='        self.app.shell.search.observe_perf_dispatch'
 pos=s.index(oldline);end=s.index(';',pos)+1
 s=s[:end]+'\n'+binding+dispatch+s[end:]
 rx=cfgblock(c,'if let Some(binding) = self')
 route=cfgblock(c,'o.route = Some(match &route')
 anchor='            match self.app.shell.search.route_response(response.request_id) {'
 if anchor in s:s=replace(s,anchor,rx+'            let route = self.app.shell.search.route_response(response.request_id);\n'+route+'            match route {')
 else:
  anchor='            let route = self.app.shell.search.route_response(response.request_id);'
  s=replace(s,anchor,rx+anchor+'\n'+route.rstrip())
 save(out,'pipeline_owner.rs',s)
 # Existing kind enqueue and active accepted RX.
 s=(out/'rust/src/app/index_coordinator.rs').read_text();c=current('index_coordinator.rs')
 kd=cfgblock(c,'"kind",\n                        0,');s=replace(s,'                Ok(()) => {\n                    super::worker::channel::trace_worker_load(\n                        &self.shell.worker_bus.kind.tx,','                Ok(()) => {\n'+kd+'                    super::worker::channel::trace_worker_load(\n                        &self.shell.worker_bus.kind.tx,')
 kr=cfgblock(c,'"kind",\n                0,');s=replace(s,'            if response.epoch != self.shell.indexing.kind_resolution_epoch {\n                continue;\n            }','            if response.epoch != self.shell.indexing.kind_resolution_epoch {\n                continue;\n            }\n'+kr.rstrip())
 save(out,'index_coordinator.rs',s)
 # Existing sort/preview dispatch and response handling.
 s=(out/'rust/src/app/result_reducer.rs').read_text();c=current('result_reducer.rs')
 sd=cfgblock(c,'"sort",\n        request_id,');s=replace(s,'    app.bind_sort_request_to_current_tab(request_id);','    app.bind_sort_request_to_current_tab(request_id);\n'+sd.rstrip())
 sr=cfgblock(c,'"sort",\n        response.request_id,');s=replace(s,'    app.take_sort_request_tab(response.request_id);',sr+'    app.take_sort_request_tab(response.request_id);')
 pr=cfgblock(c,'"preview",\n        response.request_id,')
 if tag=='v0.27.0':pr=pr.replace('            && response.page_error.is_none()\n            && (response.document.is_some() || !response.preview.is_empty())','            && !response.preview.is_empty()')
 s=replace(s,'    app.take_preview_request_tab(response.request_id);',pr+'    app.take_preview_request_tab(response.request_id);')
 save(out,'result_reducer.rs',s)
 # preview_flow is unchanged by basic patch, keep current original handler.
 s=orig(tag,'preview_flow.rs');c=current('preview_flow.rs');pd=cfgblock(c,'"preview",\n                    request_id,')
 if tag=='v0.27.0':
  anchor='                if self.shell.worker_bus.preview.tx.send(req).is_err() {'
 else:anchor='                if !self.queue_preview_request(req) {'
 s=replace(s,anchor,pd+anchor);save(out,'preview_flow.rs',s)
 print(tag,'scheduler/search/kind/sort/preview cfg preparation complete')
 # Historical-only positive cleanup recording at original worker lifetime sites.
 shutil.copyfile(BASE/'newtag-historical-perf.rs',out/'rust/src/app/historical_perf.rs')
 s=(out/'rust/src/app/mod.rs').read_text();s+='\n#[cfg(test)]\nmod historical_perf;\n';save(out,'mod.rs',s)
 s=(out/'rust/src/app/worker/runtime.rs').read_text()
 s=replace(s,'    pub(in crate::app) fn new(shutdown: Arc<AtomicBool>) -> Self {','    pub(in crate::app) fn new(shutdown: Arc<AtomicBool>) -> Self {\n        #[cfg(test)]\n        crate::app::historical_perf::runtime_created(&shutdown);')
 s=replace(s,'            handle,\n        });','            handle,\n        });\n        #[cfg(test)]\n        crate::app::historical_perf::runtime_expected(&self.shutdown, self.handles.len());')
 s=replace(s,'        let summary = runtime.join_all_with_timeout(timeout);','        #[cfg(test)]\n        let perf_shutdown_token = Arc::clone(&runtime.shutdown);\n        let summary = runtime.join_all_with_timeout(timeout);\n        #[cfg(test)]\n        crate::app::historical_perf::runtime_joined(&perf_shutdown_token, &summary);')
 save(out,'worker/runtime.rs',s)
 # Oldtags: actual commit publication scalar/Weak traveling with original committed payload.
 if tag!='v0.30.0':
  coordinator=(out/'rust/src/app/index_coordinator.rs').read_text();coordinator=replace(coordinator,'    pub(super) perf_observe_requests: bool,','    pub(super) perf_observe_requests: bool,\n    #[cfg(test)]\n    pub(super) perf_observe_commits: bool,');coordinator=replace(coordinator,'            perf_observe_requests: false,','            perf_observe_requests: false,\n            #[cfg(test)]\n            perf_observe_commits: false,');save(out,'index_coordinator.rs',coordinator)
  s=(out/'rust/src/app/tab_state.rs').read_text()
  s=replace(s,'pub(super) struct TabCommittedPayload {','pub(super) struct TabCommittedPayload {\n    #[cfg(test)]\n    pub(super) perf_commit: Option<super::historical_perf::CommitWitness>,')
  s=replace(s,'impl Default for TabCommittedPayload {\n    fn default() -> Self {\n        Self {','impl Default for TabCommittedPayload {\n    fn default() -> Self {\n        Self {\n            #[cfg(test)]\n            perf_commit: None,')
  s=replace(s,'impl TabCommittedPayload {','impl TabCommittedPayload {\n    #[cfg(test)]\n    pub(super) fn perf_freshness(&self) -> Option<&super::historical_perf::CommitWitness> {\n        self.perf_commit.as_ref().filter(|w| w.matches(&self.all_entries))\n    }')
  # Existing production literal represents a saved real snapshot: Weak witness travels with it.
  s=replace(s,'                committed: TabCommittedPayload {','                committed: TabCommittedPayload {\n                    #[cfg(test)]\n                    perf_commit: shell.shell.runtime.perf_commit.clone(),')
  save(out,'tab_state.rs',s)
  s=(out/'rust/src/app/pipeline.rs').read_text();oldfn=fn(s,'finish_active_index_request',False)
  hook='        #[cfg(test)]\n        if self.shell.indexing.perf_observe_commits {\n            self.shell.runtime.perf_commit = Some(super::historical_perf::CommitWitness::capture(request_id, self.current_tab_id().expect("active commit tab"), &self.shell.runtime.root, &self.shell.indexing.build.index.source, &self.shell.runtime.all_entries));\n        }\n'
  marker='            .apply_resource_transition(TabResourceTransition::Success);';assert marker in oldfn
  newfn=oldfn.replace(marker,marker+'\n'+hook.rstrip(),1);s=replace(s,oldfn.rstrip(),newfn.rstrip());save(out,'pipeline.rs',s)
  s=(out/'rust/src/app/tabs.rs').read_text();marker='            .apply_resource_transition(TabResourceTransition::Success);'
  hook='        #[cfg(test)]\n        if shell.indexing.perf_observe_commits {\n            tab.result_state.committed.perf_commit = Some(super::historical_perf::CommitWitness::capture(request_id, tab.id, &tab.root, &tab.index_state.build.index.source, &tab.result_state.committed.all_entries));\n        }\n'
  s=replace(s,marker,marker+'\n'+hook.rstrip());save(out,'tabs.rs',s)
  p=out/'rust/src/app/tests/indexing_perf/extensions/driver.rs';text=p.read_text();text=re.sub(r'\.freshness\s*\.as_ref\(\)', '.perf_freshness()',text);text=text.replace('    let mut driver = Driver::new();\n    driver.settle_startup();','    let mut driver = Driver::new();\n    driver.app.shell.indexing.perf_observe_commits = true;\n    driver.settle_startup();',1);p.write_text(text)
 print(tag,'actual commit/positive runtime cleanup cfg preparation complete')
 # Legacy actual IgnoreCase/IgnoreList user-operation replay and absent plain-preview observations.
 if tag!='v0.30.0':
  p=out/'rust/src/app/historical_perf.rs';text=p.read_text();undo='' if tag=='v0.27.0' else '        self.manual_filter_changed(super::search_assist::FilterValue::Case(true));\n'
  text+='\nimpl super::FlistWalkerApp {\n    pub(super) fn set_ignore_case(&mut self, value: bool) {\n        if self.shell.runtime.ignore_case == value { return; }\n        self.shell.runtime.ignore_case = value;\n'+undo+'        self.shell.tabs.mark_active_tab_meaningfully_engaged();\n        self.invalidate_result_sort(true);\n        self.update_results();\n    }\n}\n';p.write_text(text)
 p=out/'rust/src/app/tests/indexing_perf/extensions/driver.rs';text=p.read_text()
 if tag in ['v0.27.0','v0.28.0']:
  needle='        if let Some(f) = &i.build.active_filter {'
  start=text.index(needle);end=span(text,start);text=text[:start]+text[end:]
 if tag=='v0.27.0':
  text=text.replace('                    && driver.app.shell.runtime.preview_document.is_none()','')
  start=text.index('                let shown = driver\n');end=text.index(';',start)+1
  text=text[:start]+'                let shown = &driver.app.shell.runtime.preview;'+text[end:]
  old='.maybe_reindex_from_filter_toggles(false, false, false, true);'
  start=text.rfind('                        driver',0,text.index(old));end=text.index(old)+len(old)
  text=text[:start]+'                        driver.app.shell.tabs.mark_active_tab_meaningfully_engaged();\n                        driver.app.apply_entry_filters(false);\n                        driver.app.mark_ui_state_dirty();\n                        driver.app.persist_ui_state_now();'+text[end:]
  debt=fn(text,'result_debt',False)
  begin=debt.index('        || app\n            .shell\n            .worker_bus\n            .preview\n            .worker_inflight_request_id')
  end=debt.index('        || app.shell.tabs.iter()',begin)
  text=replace(text,debt.rstrip(),(debt[:begin]+debt[end:]).rstrip())
 p.write_text(text)
 print(tag,'legacy operations and absent-field observation compatibility prepared')
 # Parser-only fixtures: retain first real join outcome, never detached or zero-handle cleanup as success.
 p=out/'rust/src/app/tests/indexing_perf/extensions/supplementary.rs';text=p.read_text()
 text=replace(text,'struct ParserWorkerCleanup {','struct ParserWorkerCleanup {\n    proof_id: u64,')
 text=replace(text,'impl ParserWorkerCleanup {','impl ParserWorkerCleanup {\n    fn new(shutdown: Arc<AtomicBool>, tx: BoundedSender<IndexRequest>, handles: Vec<thread::JoinHandle<()>>) -> Self {\n        let proof_id = crate::app::historical_perf::parser_started(handles.len());\n        Self { proof_id, shutdown, tx: Some(tx), handles }\n    }')
 text=text.replace('ParserWorkerCleanup {\n        shutdown: Arc::clone(&shutdown),\n        tx: Some(tx),\n        handles,\n    }','ParserWorkerCleanup::new(Arc::clone(&shutdown), tx, handles)')
 text=replace(text,'ParserWorkerCleanup {\n            shutdown: Arc::clone(&shutdown),\n            tx: Some(tx),\n            handles: vec![worker],\n        }','ParserWorkerCleanup::new(Arc::clone(&shutdown), tx, vec![worker])')
 part=fn(text,'stop',False)
 part=replace(part,'        self.shutdown.store(true, Ordering::Relaxed);','        let expected = self.handles.len();\n        self.shutdown.store(true, Ordering::Relaxed);')
 start=part.index('        for h in self.handles.drain(..) {');end=span(part,start)
 part=part[:start]+'''        let mut joined = 0;
        let mut panicked = 0;
        for h in self.handles.drain(..) {
            if h.is_finished() {
                joined += 1;
                panicked += usize::from(h.join().is_err());
            }
        }
        crate::app::historical_perf::parser_joined(self.proof_id, expected, joined, panicked, timed_out);
        if panicked > 0 && !thread::panicking() { panic!("parser worker failed"); }
'''+part[end:]
 text=replace(text,fn(text,'stop',False).rstrip(),part.rstrip());p.write_text(text)
 print(tag,'parser first-real physical join observer prepared')
 # Historical-only full-cell catch: assertions remain inside; continuation requires positive cleanup + actual restored roots.
 p=out/'rust/src/app/tests/indexing_perf/extensions/runner.rs';text=p.read_text()
 header='''
fn restored_fixture(f: &ExtendedFixture, source: Source) -> bool {
    f.root.is_dir()
        && f.records.iter().all(|r| std::fs::metadata(&r.path).is_ok_and(|m| m.is_dir() == r.is_dir))
        && match source {
            Source::FileList => std::fs::read(f.root.join("FileList.txt")).is_ok_and(|bytes| bytes == f.manifest),
            Source::Walker => !f.root.join("FileList.txt").exists() && !f.root.join("filelist.txt").exists(),
        }
}
'''
 text=replace(text,'use super::*;', 'use super::*;\n'+header)
 text=text.replace('    let mut rows = 0;','    let mut rows = 0;\n    let mut failures = 0;\n    let mut stopped = false;',1)
 start=text.index('    for (profile, source) in supported.iter().copied() {')
 text=text[:start]+text[start:].replace('    for (profile, source) in supported.iter().copied() {','''    for (profile, source) in supported.iter().copied() {
        if stopped {
            eprintln!("INDEX_PERF_CELL_STATUS {}",serde_json::json!({"case":profile.name(),"source":source.name(),"status":"NOT_RUN","reason":"previous cell cleanup or restoration unproven"}));
            continue;
        }
        let first_row = rows;
        let cleanup = crate::app::historical_perf::CellGuard::begin();
        eprintln!("INDEX_PERF_CELL_START {}",serde_json::json!({"case":profile.name(),"source":source.name(),"pairs":pairs,"entries":count,"expected_rows":pairs*2}));
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {''',1)
 # End catches before the original loop closes. Keep all original warmup/sample predicates and output.
 marker='''        }
    }
    assert_eq!(rows, supported.len() * pairs * 2, "missing raw rows");'''
 replacement='''        }
        }));
        let fixture = &fixtures.iter().find(|(shape,_)| *shape == profile.shape()).unwrap().1;
        let positive_cleanup = cleanup.safe();
        let restored = positive_cleanup && restored_fixture(fixture,source)
            && (!profile.tabs() || restored_fixture(&b,source))
            && (profile != Profile::TabChain || restored_fixture(&c,source));
        if !positive_cleanup {
            for (_,owned) in &fixtures { owned.retain_historical_root(); }
            b.retain_historical_root(); c.retain_historical_root();
        }
        let succeeded = outcome.is_ok() && positive_cleanup && restored && rows-first_row==pairs*2;
        if !succeeded { failures += 1; }
        eprintln!("INDEX_PERF_CELL_STATUS {}",serde_json::json!({"case":profile.name(),"source":source.name(),"status":if succeeded{"PASS"}else{"FAIL"},"actual_partial_rows":rows-first_row,"accepted_rows":if succeeded{rows-first_row}else{0},"positive_cleanup":positive_cleanup,"root_restored":restored,"cleanup_evidence":cleanup.evidence(),"panic":outcome.as_ref().err().map(|e| e.downcast_ref::<String>().cloned().or_else(||e.downcast_ref::<&str>().map(|s|s.to_string())).unwrap_or_else(||"non-string panic".into()))}));
        stopped = !positive_cleanup || !restored;
        drop(cleanup);
    }
    assert_eq!(failures,0,"historical observed cell failure(s), partial rows are not accepted timing");
    assert!(!stopped,"historical cleanup uncertainty prevented remaining cells");
    assert_eq!(rows, supported.len() * pairs * 2, "missing raw rows");'''
 text=replace(text,marker,replacement);p.write_text(text)
 # Historical filesystem guard: unknown physical cleanup retains every owned root.
 p=out/'rust/src/app/tests/indexing_perf/extensions/fixture.rs';fixture_text=p.read_text()
 fixture_text=replace(fixture_text,'    owns_root: bool,','    owns_root: bool,\n    historical_retained: std::cell::Cell<bool>,')
 fixture_text=fixture_text.replace('            owns_root: true,','            owns_root: true,\n            historical_retained: std::cell::Cell::new(false),').replace('            owns_root: false,','            owns_root: false,\n            historical_retained: std::cell::Cell::new(false),')
 fixture_text=replace(fixture_text,'impl ExtendedFixture {','impl ExtendedFixture {\n    pub(super) fn retain_historical_root(&self) {\n        self.historical_retained.set(true);\n        eprintln!("INDEX_PERF_ROOT_RETAINED {}", serde_json::json!({"root":self.root,"root_exists":self.root.exists(),"backup":self.publication_backup.borrow().as_ref(),"reason":"physical cleanup unproven; no restoration or deletion authorized"}));\n    }\n')
 fixture_text=replace(fixture_text,'        if self.owns_root && self.publication_backup.borrow().is_none() {','        if self.owns_root && crate::app::historical_perf::cleanup_unproven() { self.retain_historical_root(); }\n        if self.owns_root && !self.historical_retained.get() && self.publication_backup.borrow().is_none() {')
 fixture_text=replace(fixture_text,"impl Drop for EmptyRootPublication<'_> {\n    fn drop(&mut self) {","impl Drop for EmptyRootPublication<'_> {\n    fn drop(&mut self) {\n        if std::thread::panicking() && crate::app::historical_perf::cleanup_unproven() {\n            self.fixture.retain_historical_root();\n            return;\n        }")
 fixture_text=replace(fixture_text,"impl Drop for GenerationChange<'_> {\n    fn drop(&mut self) {","impl Drop for GenerationChange<'_> {\n    fn drop(&mut self) {\n        if crate::app::historical_perf::cleanup_unproven() {\n            self.original.retain_historical_root();\n            return;\n        }")
 p.write_text(fixture_text)
 # Single truncated cell uses the same join proof during fixture unwind.
 p=out/'rust/src/app/tests/indexing_perf/extensions/runner.rs';text=p.read_text()
 text=replace(text,'fn perf_indexing_truncated_serial() {','fn perf_indexing_truncated_serial() {\n    let cleanup = crate::app::historical_perf::CellGuard::begin();')
 text=replace(text,'    assert_eq!(rows, pairs * 2);','    if !cleanup.safe() { fixture.retain_historical_root(); }\n    assert!(cleanup.safe(), "truncated physical cleanup unproven; owned root retained");\n    assert_eq!(rows, pairs * 2);')
 p.write_text(text)
 # A separately selected truncated process has one cell: retain its original failure exit and no continuation.
 print(tag,'whole-cell failure/status/positive-cleanup gate prepared')
 # Remaining mechanical absent APIs, preserving old error surface and existing substantive checks.
 if tag in ['v0.27.0','v0.28.0']:
  p=out/'rust/src/app/tests/indexing_perf/harness.rs';text=p.read_text();needle='        if let Some(filter) = &i.build.active_filter {';start=text.index(needle);end=span(text,start);text=text[:start]+text[end:];p.write_text(text)
 if tag=='v0.27.0':
  for p in (out/'rust/src/app/tests/indexing_perf').rglob('*.rs'):
   text=p.read_text().replace('assert!(driver.app.shell.runtime.query_state.search_error.is_none());','assert!(!driver.app.status_line_text().contains("Search failed:"));');p.write_text(text)
 # Owned settings persist worker is part of actual runtime: retain on unproven joins.
 p=out/'rust/src/app/tests/support.rs';text=p.read_text()
 old=fn(text,'drop',False) if False else None
 start=text.index('impl Drop for TestSettingsScope {');end=span(text,start)
 part=text[start:end]
 part=replace(part,'        let _ = fs::remove_dir_all(&self.base);','        if crate::app::historical_perf::cleanup_unproven() {\n            eprintln!("INDEX_PERF_SETTINGS_RETAINED {}", serde_json::json!({"base":self.base,"base_exists":self.base.exists(),"reason":"physical cleanup unproven"}));\n            return;\n        }\n        let _ = fs::remove_dir_all(&self.base);')
 text=text[:start]+part+text[end:];p.write_text(text)
 print(tag,'preparation source complete; compilation/measurement NOT RUN')
