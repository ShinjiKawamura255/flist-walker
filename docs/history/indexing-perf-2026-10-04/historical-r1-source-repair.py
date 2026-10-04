from pathlib import Path
B=Path('/private/tmp/flistwalker-indexing-perf-20261003/rust/target/indexing-perf-study/newtag-repair-r1')
def one(s,a,b):
 assert s.count(a)==1,(a[:100],s.count(a));return s.replace(a,b,1)
for tag in ['v0.27.0','v0.28.0','v0.29.0','v0.30.0']:
 root=B/tag/'rust/src';app=root/'app'
 # New common observer retains actual helper-thread results, never native invented fields.
 s=(B/'historical-perf-prefix.rs.txt').read_text()+(B/'historical-perf-cleanup.rs.txt').read_text()+(B/'historical-perf-parser.rs.txt').read_text()+(B/'historical-perf-tests.rs.txt').read_text()+(B/'guard-helper-extra.rs.txt').read_text()
 s=s.replace('NATIVE_PANICS', 'Some(summary.panicked.as_slice())' if tag=='v0.30.0' else 'None')
 (app/'historical_perf.rs').write_text(s)
 p=app/'mod.rs';s=p.read_text();s=one(s,'mod historical_perf;','pub(crate) mod historical_perf;');p.write_text(s)
 p=app/'worker/runtime.rs';s=p.read_text()
 old='crate::app::historical_perf::runtime_joined(&perf_shutdown_token, &summary);'
 new='crate::app::historical_perf::runtime_joined(&perf_shutdown_token, summary.joined, summary.total, &summary.pending, '+('Some(&summary.panicked)' if tag=='v0.30.0' else 'None')+');'
 s=one(s,old,new)
 if tag!='v0.30.0':
  s=one(s,'            thread::spawn(move || {\n                let _ = handle.join();', '''            #[cfg(test)]
            let perf_join_observer = crate::app::historical_perf::runtime_join_observer(&self.shutdown);
            thread::spawn(move || {
                #[cfg(not(test))]
                let _ = handle.join();
                #[cfg(test)]
                if let Some(observer) = perf_join_observer {
                    let ok = handle.join().is_ok();
                    observer.returned(&name, ok);
                } else {
                    let _ = handle.join();
                }''')
 else:
  s=one(s,'            thread::spawn(move || {\n                let panicked = handle.join().is_err();','            #[cfg(test)]\n            let perf_join_observer = crate::app::historical_perf::runtime_join_observer(&self.shutdown);\n            thread::spawn(move || {\n                let panicked = handle.join().is_err();\n                #[cfg(test)]\n                if let Some(observer) = perf_join_observer { observer.returned(&name, !panicked); }')
 p.write_text(s)
 # Establish actual startup ownership before app constructor can unwind.
 p=app/'tests/indexing_perf/harness.rs';s=p.read_text()
 s=one(s,'        fs::create_dir_all(&startup_root).unwrap();\n        let mut app', '        fs::create_dir(&startup_root).unwrap();\n        let owned_startup_root = OwnedEmptyRoot(startup_root.clone());\n        let mut app')
 s=one(s,'            _startup_root: OwnedEmptyRoot(startup_root),','            _startup_root: owned_startup_root,')
 s=one(s,'impl Drop for OwnedEmptyRoot {\n    fn drop(&mut self) {\n        let _ = fs::remove_dir_all(&self.0);','''impl Drop for OwnedEmptyRoot {
    fn drop(&mut self) {
        if crate::app::historical_perf::cleanup_unproven() {
            eprintln!("INDEX_PERF_STARTUP_ROOT_RETAINED {}",serde_json::json!({"root":self.0,"root_exists":self.0.exists(),"reason":"physical cleanup unproven"}));
            return;
        }
        let _ = fs::remove_dir_all(&self.0);''')
 s+='\n'+(B/'startup-root-guards.rs.txt').read_text().replace('NATIVE_PANICS', 'Some(summary.panicked.as_slice())' if tag=='v0.30.0' else 'None');p.write_text(s)
 # Path ownership binding is enabled only by a historical cell.
 p=app/'tests/support.rs';s=p.read_text()
 needle='        Self { base }'
 s=one(s,needle,'        crate::app::historical_perf::bind_settings_writer(&base.join(".flistwalker_ui_state.json"));\n'+needle);s=s.replace('fs::create_dir_all(&base).expect("create test settings dir");','fs::create_dir(&base).expect("exclusive owned test settings dir");').replace('fs::create_dir_all(&base).expect("owned settings dir");','fs::create_dir(&base).expect("exclusive owned settings dir");');p.write_text(s)
 p=app/'tests/support.rs';p.write_text(p.read_text()+'\n'+(B/'settings-scope-guards.rs.txt').read_text())
 # Actual registry spawn/cleanup seam, no general persistence-body backport.
 p=app/'session.rs' if tag in ['v0.27.0','v0.28.0'] else root/'persistence/worker.rs';s=p.read_text()
 if tag in ['v0.27.0','v0.28.0']:
  needle='''    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        run_ui_state_persistence_worker(
            rx,
            path,
            history_persist_disabled,
            UI_STATE_PERSISTENCE_LOCK_TIMEOUT,
        )
    });
    tx
}'''
  replacement='''    let (tx, rx) = mpsc::channel();
    #[cfg(test)]
    if let Some(observer) = crate::app::historical_perf::owned_writer(&path) {
        let handle = thread::spawn(move || {
            run_ui_state_persistence_worker(rx,path,history_persist_disabled,UI_STATE_PERSISTENCE_LOCK_TIMEOUT)
        });
        observer.spawned(handle);
        return tx;
    }
    thread::spawn(move || {
        run_ui_state_persistence_worker(
            rx,
            path,
            history_persist_disabled,
            UI_STATE_PERSISTENCE_LOCK_TIMEOUT,
        )
    });
    tx
}'''
  s=one(s,needle,replacement)
 else:
  needle='''        .or_insert_with(|| {
            spawn_ui_state_persistence_worker(
                path,
                history_persist_disabled,
                UI_STATE_PERSISTENCE_LOCK_TIMEOUT,
                startup_protected,
            )
            .0
        })'''
  replacement='''        .or_insert_with(|| {
            #[cfg(test)]
            if let Some(observer) = crate::app::historical_perf::owned_writer(&path) {
                let (sender,handle) = spawn_ui_state_persistence_worker(path,history_persist_disabled,UI_STATE_PERSISTENCE_LOCK_TIMEOUT,startup_protected);
                observer.spawned(handle);
                return sender;
            }
            spawn_ui_state_persistence_worker(
                path,
                history_persist_disabled,
                UI_STATE_PERSISTENCE_LOCK_TIMEOUT,
                startup_protected,
            )
            .0
        })'''
  s=one(s,needle,replacement)
 if tag=='v0.27.0':
  # This is untimed final cleanup, after original App persist/flush/runtime shutdown.
  block=(B/'writer-shutdown-v27.rs.txt').read_text();s+='\n'+block
  needle='        let _ = self.shutdown_workers_with_timeout(Self::WORKER_JOIN_TIMEOUT, phase);\n    }\n\n    pub(super) fn ui_state_file_path()'
  replacement='        let _ = self.shutdown_workers_with_timeout(Self::WORKER_JOIN_TIMEOUT, phase);\n        #[cfg(test)]\n        if let Some(path) = Self::ui_state_file_path() {\n            shutdown_owned_historical_writer(&path, Self::WORKER_JOIN_TIMEOUT);\n        }\n    }\n\n    pub(super) fn ui_state_file_path()'
  s=one(s,needle,replacement)
 else:
  marker='fn shutdown_ui_state_persistence_for_test(path: &Path, timeout: Duration) {'
  owned=(B/'writer-shutdown-modern.rs.txt').read_text().replace('SEND_SHUTDOWN','sender.send(UiStatePersistenceCommand::Shutdown(tx))' if tag=='v0.28.0' else 'sender.send_control(UiStatePersistenceCommand::Shutdown(tx))').replace('REMOVE_STARTUP','' if tag=='v0.28.0' else 'registry.startup_failures.remove(path);')
  s=one(s,marker,marker+'\n'+owned)
 s+='\n'+(B/'writer-integration-guards.rs.txt').read_text().replace('OWNED_SHUTDOWN','shutdown_owned_historical_writer' if tag=='v0.27.0' else 'shutdown_ui_state_persistence_for_test')
 p.write_text(s)
 # Unified one-cell truncated status, exact panic/cleanup/config truth and real failure exit.
 p=app/'tests/indexing_perf/extensions/runner.rs';s=p.read_text()
 marker='    let fixture = ExtendedFixture::new(500001, Shape::FlatFiles);';at=s.index(marker)
 s=s[:at]+(B/'truncated-body.rs.txt').read_text()
 p.write_text(s)
 # Actual global cap restoration: never mutate under uncertain worker/writer cleanup.
 p=app/'tests/indexing_perf/extensions/supplementary.rs';s=p.read_text()
 s=one(s,'        crate::runtime_config::set_process_runtime_config(self.0.clone());','''        if crate::app::historical_perf::cleanup_unproven() {
            eprintln!("INDEX_PERF_CONFIG_RETAINED: physical cleanup unproven; original global config restoration NOT_RUN");
            return;
        }
        crate::runtime_config::set_process_runtime_config(self.0.clone());''');p.write_text(s)
 print(tag,'four review findings prepared; execution NOT_RUN')
