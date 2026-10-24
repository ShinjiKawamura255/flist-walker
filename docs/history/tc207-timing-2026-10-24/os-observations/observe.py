#!/usr/bin/env python3
"""Owned-child macOS state samples, not a context-switch event trace.

No production instrumentation or timing waiver. All nonzero test exits survive.
Private enumeration flavor28 is diagnostic-only, runtime-validated against our
own native TID. Public flavor15 reads that exact TID; errors are never zero CPU.
"""
import argparse
import collections
import concurrent.futures
import ctypes
import gzip
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import signal
import subprocess
import time

spec = importlib.util.spec_from_file_location('previous', Path(__file__).parent.parent/'collect.py')
previous = importlib.util.module_from_spec(spec)
spec.loader.exec_module(previous)


def correlate(frame, rows):
    window = frame.get('monotonic_window_ns')
    margin = frame.get('measurement_overhead_ns', 0)
    eligible = []
    if window and frame.get('native_thread_id'):
        eligible = [r for r in rows if r['pid'] == frame['pid']
                    and r['tid'] == frame['native_thread_id']
                    and window[0]+margin <= r['start_ns'] <= r['end_ns'] <= window[1]-margin]
    counters = [r['cpu_ns'] for r in eligible]
    retrograde = any(b < a for a,b in zip(counters,counters[1:]))
    return dict(frame=frame, wall_under_100ms=frame['wall_under_100ms'],
                sample_count=len(eligible),
                state_sample_counts=dict(collections.Counter(str(r['state']) for r in eligible)),
                sampled_cpu_advance_ns=(counters[-1]-counters[0]) if len(counters)>1 and not retrograde else None,
                cpu_counter_retrograde=retrograde)


class ThreadInfo(ctypes.Structure):
    _fields_ = [('user',ctypes.c_uint64),('system',ctypes.c_uint64)] + [
        (n,ctypes.c_int32) for n in ('cpu','policy','state','flags','sleep','curpri','priority','maxpriority')
    ] + [('name',ctypes.c_char*64)]


class ProcObserver:
    def __init__(self):
        if platform.system() != 'Darwin':
            raise RuntimeError('macOS-only diagnostic')
        self.lib = ctypes.CDLL('/usr/lib/libproc.dylib',use_errno=True)
        self.lib.proc_pidinfo.argtypes = [ctypes.c_int,ctypes.c_int,ctypes.c_uint64,ctypes.c_void_p,ctypes.c_int]
        self.lib.proc_pidinfo.restype = ctypes.c_int
        assert ctypes.sizeof(ThreadInfo) == 112
        native = ctypes.c_uint64()
        libc = ctypes.CDLL(None)
        libc.pthread_threadid_np.argtypes = [ctypes.c_void_p,ctypes.POINTER(ctypes.c_uint64)]
        assert libc.pthread_threadid_np(None,ctypes.byref(native)) == 0
        own = self.snapshot(os.getpid())
        assert any(r.get('tid') == native.value for r in own), 'unsupported enumeration ABI'

    def snapshot(self, pid):
        ids = (ctypes.c_uint64*128)()
        n = self.lib.proc_pidinfo(pid,28,0,ids,ctypes.sizeof(ids))
        if n <= 0 or n%8 or n >= ctypes.sizeof(ids):
            return [dict(pid=pid,error='thread enumeration unavailable/truncated',ret=n,errno=ctypes.get_errno())]
        rows = []
        for tid in list(ids)[:n//8]:
            info = ThreadInfo()
            start = time.clock_gettime_ns(time.CLOCK_MONOTONIC)
            n = self.lib.proc_pidinfo(pid,15,tid,ctypes.byref(info),ctypes.sizeof(info))
            end = time.clock_gettime_ns(time.CLOCK_MONOTONIC)
            if n != ctypes.sizeof(info):
                rows.append(dict(pid=pid,tid=tid,error='thread exited or query unavailable',ret=n,errno=ctypes.get_errno()))
            else:
                rows.append(dict(pid=pid,tid=tid,start_ns=start,end_ns=end,state=info.state,
                                 cpu_ns=info.user+info.system,name=info.name.decode(errors='replace')))
        return rows


def cleanup(child):
    if child is not None and child.poll() is None:
        try:
            os.kill(child.pid,signal.SIGCONT)
        except ProcessLookupError:
            pass
        child.kill()
        child.wait(timeout=5)


def extend_samples(rows, new_rows, limit=100_000):
    if len(rows)+len(new_rows)>limit:
        raise RuntimeError('sample row cap reached; partial observation retained')
    rows.extend(new_rows)


def run(binary, output, label, observed=True, control=None, stacks=False):
    try:
        return _run(binary, output, label, observed, control, stacks)
    except BaseException as exc:
        (output/(label+'.failure.json')).write_text(json.dumps(dict(error=repr(exc)))+'\n')
        raise


def _run(binary, output, label, observed=True, control=None, stacks=False):
    env = os.environ.copy()
    env.pop('FLISTWALKER_TEST_TC207_CONTROL',None)
    env['FLISTWALKER_TEST_TC207_TRACE'] = '1'
    if control:
        env['FLISTWALKER_TEST_TC207_CONTROL'] = control
    cases = previous.CASES[-1:] if control else previous.CASES
    argv = [str(binary),'--exact',*cases,'--nocapture','--test-threads=1']
    observer = ProcObserver() if observed else None
    samples, stop, profiler, child = [], None, None, None
    log = output/(label+'.log')
    error, failure, cleanup_errors, processes = None, None, [], []
    with log.open('wb') as stream, (output/(label+'.sample-driver.log')).open('wb') as driver:
        try:
            child = subprocess.Popen(argv,env=env,stdout=stream,stderr=subprocess.STDOUT)
            started = time.monotonic()
            if stacks:
                profiler = subprocess.Popen(['/usr/bin/sample',str(child.pid),'1','1','-mayDie','-file',str(output/(label+'.sample.txt'))],stdout=driver,stderr=subprocess.STDOUT)
            while child.poll() is None:
                if time.monotonic()-started > 30:
                    raise TimeoutError('owned test exceeded30s')
                if observer:
                    extend_samples(samples,observer.snapshot(child.pid))
                if control == 'stop':
                    if stop is None:
                        pid,status = os.waitpid(child.pid,os.WNOHANG|os.WUNTRACED)
                        if pid:
                            if not os.WIFSTOPPED(status):
                                child.returncode = os.waitstatus_to_exitcode(status)
                                raise RuntimeError('exited before observed stop')
                            assert os.WSTOPSIG(status) == signal.SIGSTOP
                            stop = dict(signal=signal.SIGSTOP,observed_ns=time.monotonic_ns(),resumed_ns=None)
                    elif stop['resumed_ns'] is None and time.monotonic_ns()-stop['observed_ns'] >= 150_000_000:
                        os.kill(child.pid,signal.SIGCONT)
                        stop['resumed_ns'] = time.monotonic_ns()
                time.sleep(.001)
            code = child.wait(timeout=5)
        except BaseException as exc:
            error = repr(exc)
            failure = exc
        finally:
            # One owner's failure must not prevent the other owner from reaping.
            for name,process in [('child',child),('profiler',profiler)]:
                if process is None:
                    continue
                if name=='profiler':
                    try:
                        process.wait(timeout=3)
                    except BaseException as exc:
                        cleanup_errors.append(dict(stage='profiler-wait',error=repr(exc)))
                try:
                    cleanup(process)
                except BaseException as exc:
                    cleanup_errors.append(dict(stage=name+'-cleanup',error=repr(exc)))
                processes.append(dict(owner=name,pid=process.pid,exit=process.returncode))
            # Retain partial samples even on observer error or profiler timeout.
            # Always attempt lifecycle output even if the state file cannot save.
            try:
                with gzip.open(output/(label+'.states.json.gz'),'wt') as f:
                    json.dump(samples,f)
            finally:
                (output/(label+'.lifecycle.json')).write_text(json.dumps(dict(pid=child.pid if child else None,exit=child.returncode if child else None,error=error,cleanup_errors=cleanup_errors,processes=processes,stop=stop,profiler_exit=profiler.returncode if profiler else None))+'\n')
        if failure is not None:
            raise failure
        if cleanup_errors:
            raise RuntimeError('cleanup/profiler failure: '+repr(cleanup_errors))
    text = log.read_text()
    frames = [json.loads(m[1]) for m in re.finditer(r'TC207_FRAME (\{[^\n]*\})',text)]
    assert frames and {f['test'] for f in frames} == set(cases)
    for case in cases:
        rows = [f for f in frames if f['test']==case]
        assert [f['frame'] for f in rows] == list(range(len(rows)))
        assert all(f['frames_observed']==len(rows) and f['pid']==child.pid for f in rows)
        assert all(f['wall_under_100ms']==(f['wall_ns']<100_000_000) for f in rows)
    valid_samples = [r for r in samples if 'error' not in r]
    matched = [correlate(f,valid_samples) for f in frames]
    failing = [f for f in frames if not f['wall_under_100ms']]
    receipt = dict(label=label,argv=argv,pid=child.pid,observed=observed,control=control,
                   exit_code=code,frame_count=len(frames),failures=len(failing),
                   max_wall=max(matched,key=lambda r:r['frame']['wall_ns']),
                   matched_samples=sum(r['sample_count'] for r in matched),
                   state_counts=dict(sum((collections.Counter(r['state_sample_counts']) for r in matched),collections.Counter())),
                   query_errors=sum('error' in r for r in samples),stop=stop,
                   expected_exit=101 if control else 0,
                   experiment_valid=(code==(101 if control else 0) and (len(failing)==1 if control else not failing)))
    if observed:
        receipt['experiment_valid'] &= receipt['matched_samples']>0
    if control:
        receipt['experiment_valid'] &= 'background finalization frame exceeded 100ms' in text
        field = 'cpu_lower_ns' if control=='cpu' else 'off_cpu_lower_ns'
        receipt['experiment_valid'] &= bool(failing) and (failing[0].get(field) or 0)>=100_000_000
        if control=='stop':
            receipt['experiment_valid'] &= bool(stop and stop['resumed_ns'] and stop['resumed_ns']-stop['observed_ns']>=150_000_000)
    for name,data in [('frames',frames),('correlated',matched)]:
        with gzip.open(output/(label+'.'+name+'.json.gz'),'wt') as f:json.dump(data,f)
    with gzip.open(output/(label+'.log.gz'),'wb') as f:f.write(log.read_bytes())
    log.unlink()
    (output/(label+'.receipt.json')).write_text(json.dumps(receipt,indent=2)+'\n')
    print(json.dumps({k:receipt[k] for k in ('label','exit_code','failures','matched_samples','experiment_valid')}),flush=True)
    return receipt


def main():
    previous.install_termination_handler()
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True)
    args = p.parse_args()
    binary = args.binary.resolve(strict=True)
    args.output.mkdir(parents=True,exist_ok=False)
    sha = lambda f:hashlib.sha256(f.read_bytes()).hexdigest()
    provenance = dict(source=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
                      binary_sha256=sha(binary),collector_sha256=sha(Path(__file__)),
                      platform=platform.platform(),clock='CLOCK_MONOTONIC',wall_limit_ns=100_000_000,
                      retries=0,plan=dict(controls=['cpu','sleep','stop'],unobserved_serial=1,
                      observed_serial=1,observed_parallel_processes=12,stack_samples='cpu/sleep controls only'),
                      sample_cap_rows_per_child=100_000,
                      private_api='diagnostic-only enumeration28; self native-TID/sizeof/return validated; no product change')
    (args.output/'provenance.json').write_text(json.dumps(provenance,indent=2)+'\n')
    results = [run(binary,args.output,'control-'+mode,control=mode,stacks=mode!='stop') for mode in ('cpu','sleep','stop')]
    results.append(run(binary,args.output,'unobserved-serial',observed=False))
    results.append(run(binary,args.output,'observed-serial'))
    with concurrent.futures.ThreadPoolExecutor(max_workers=12) as pool:
        futures = [pool.submit(run,binary,args.output,f'observed-parallel12-{i}') for i in range(12)]
        results.extend(f.result() for f in futures)
    assert sha(binary)==provenance['binary_sha256']
    valid = all(r['experiment_valid'] for r in results)
    (args.output/'summary.json').write_text(json.dumps(dict(receipts=results,all_experiments_valid=valid),indent=2)+'\n')
    raise SystemExit(0 if valid else 1)


if __name__ == '__main__':
    main()
