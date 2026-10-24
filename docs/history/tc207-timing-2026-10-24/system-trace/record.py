#!/usr/bin/env python3
"""Own/reap the stopped test child; resume only on xctrace's start notification."""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import time
import uuid


def wait_stopped(child, timeout=10):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        pid, status = os.waitpid(child.pid, os.WNOHANG | os.WUNTRACED)
        if pid:
            if os.WIFSTOPPED(status) and os.WSTOPSIG(status) == signal.SIGSTOP:
                return
            if os.WIFEXITED(status) or os.WIFSIGNALED(status):
                child.returncode = os.waitstatus_to_exitcode(status)
            raise RuntimeError('owned child exited or unexpectedly changed state before startup stop')
        time.sleep(.001)
    raise TimeoutError('no owned startup stop')


def cleanup(children):
    errors = []
    for child in children:
        if child is None:
            continue
        try:
            if child.poll() is None:
                child.send_signal(signal.SIGCONT)
                child.kill()
            child.wait(timeout=5)
        except BaseException as exc:
            errors.append(exc)
    if errors:
        raise errors[0]


def validate_budgets(child_timeout,trace_seconds,finalize_timeout):
    if child_timeout<=0 or finalize_timeout<=0 or trace_seconds<child_timeout+15:
        raise ValueError('trace must include child timeout plus15s startup/notification budget')


def spawn_target(argv,env,stdout,stderr):
    return subprocess.Popen(argv,env=env,stdout=stdout,stderr=stderr)


def capture(binary, library, output, argv, control=None, child_timeout=45, trace_seconds=60, finalize_timeout=20,
            template='System Trace',instruments=(),recording_options=None,dedicated_frames=False):
    validate_budgets(child_timeout,trace_seconds,finalize_timeout)
    output.mkdir(parents=True, exist_ok=False,mode=0o700)
    child, tracer, token, lib = None, None, None, None
    receipt = dict(argv=argv, binary=str(binary), library=str(library),
                   binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                   library_sha256=hashlib.sha256(library.read_bytes()).hexdigest(),
                   nonce=uuid.uuid4().hex, control=control, status='INCOMPLETE',
                   child_timeout_seconds=child_timeout,trace_seconds=trace_seconds,finalize_timeout_seconds=finalize_timeout,
                   dedicated_frames=dedicated_frames)
    try:
        lib = ctypes.CDLL('/usr/lib/libSystem.B.dylib')
        lib.notify_register_file_descriptor.argtypes = [ctypes.c_char_p, ctypes.POINTER(ctypes.c_int), ctypes.c_int, ctypes.POINTER(ctypes.c_int)]
        lib.notify_register_file_descriptor.restype = ctypes.c_uint32
        lib.notify_cancel.argtypes = [ctypes.c_int]
        name = ('org.flistwalker.tc207.started.' + receipt['nonce']).encode()
        fd, pending = ctypes.c_int(), ctypes.c_int()
        if lib.notify_register_file_descriptor(name, ctypes.byref(fd), 0, ctypes.byref(pending)) != 0:
            raise RuntimeError('notify registration failed')
        token = pending.value
        env = os.environ.copy()
        env.pop('FLISTWALKER_TEST_TC207_CONTROL', None)
        env['FLISTWALKER_TEST_TC207_TRACE'] = '1'
        if control:
            env['FLISTWALKER_TEST_TC207_CONTROL'] = control
        env['DYLD_INSERT_LIBRARIES'] = str(library)
        env['TC207_DIAGNOSTIC_PARENT_PID'] = str(os.getpid())
        env['TC207_DIAGNOSTIC_NONCE'] = receipt['nonce']
        if dedicated_frames:
            (output/'frame-logs').mkdir(mode=0o700)
            env['FLISTWALKER_TEST_TC207_FRAME_LOG_DIR']=str(output/'frame-logs')
        with (output/'test.log').open('wb') as testlog, (output/'test.stderr').open('wb') as testerr, (output/'tracer.stdout').open('wb') as traceout, (output/'tracer.stderr').open('wb') as traceerr:
            child = spawn_target([str(binary), *argv],env,testlog,testerr)
            receipt['pid'] = child.pid
            wait_stopped(child)
            receipt['startup_stop_observed_ns'] = time.monotonic_ns()
            trace_env = os.environ.copy()
            trace_env['DEVELOPER_DIR'] = '/Applications/Xcode.app/Contents/Developer'
            trace_argv = ['xcrun','xctrace','record','--template',template,'--time-limit',str(trace_seconds)+'s',
                          '--output',str(output/'record.trace'),'--attach',str(child.pid),
                          '--notify-tracing-started',name.decode()]
            for instrument in instruments:trace_argv.extend(['--instrument',instrument])
            if recording_options:trace_argv.extend(['--recording-options',str(recording_options)])
            receipt['tracer_argv'] = trace_argv
            tracer = subprocess.Popen(trace_argv, env=trace_env, stdout=traceout, stderr=traceerr)
            if not select.select([fd.value], [], [], 15)[0]:
                raise TimeoutError('no actual tracing-start notification')
            delivery = os.read(fd.value, 4)
            if len(delivery) != 4 or int.from_bytes(delivery, 'big') != token:
                raise RuntimeError('wrong tracing-start notification token')
            receipt['tracing_start_notification_ns'] = time.monotonic_ns()
            child.send_signal(signal.SIGCONT)
            receipt['startup_resume_sent_ns'] = time.monotonic_ns()
            receipt['test_exit'] = child.wait(timeout=child_timeout)
            if tracer.poll() is None:
                tracer.send_signal(signal.SIGINT)
            receipt['tracer_exit'] = tracer.wait(timeout=finalize_timeout)
            receipt['status'] = 'RECORDED' if receipt['tracer_exit'] == 0 else 'TRACER_FAILED'
            if dedicated_frames:
                path=output/'frame-logs'/('frames-'+str(child.pid)+'.jsonl')
                receipt['frame_log_present']=path.is_file() and not path.is_symlink() and path.stat().st_size>0
                if not receipt['frame_log_present']:receipt['status']='FRAME_LOG_FAILED'
            if control and b'background finalization frame exceeded 100ms' not in (output/'test.stderr').read_bytes():
                receipt['status']='CONTROL_ASSERTION_MISSING'
    except BaseException as exc:
        receipt['error'] = repr(exc)
        raise
    finally:
        try:
            cleanup([child, tracer])
        except BaseException as exc:
            receipt['cleanup_error'] = repr(exc)
            raise
        finally:
            if lib is not None and token is not None:
                receipt['notify_cancel_status'] = lib.notify_cancel(token)
            receipt['test_exit'] = child.returncode if child else None
            receipt['tracer_exit'] = tracer.returncode if tracer else None
            receipt['finished_ns'] = time.monotonic_ns()
            (output/'receipt.json').write_text(json.dumps(receipt, indent=2)+'\n')
    return receipt


if __name__ == '__main__':
    def terminate(signum, _frame):
        signal.signal(signum, signal.SIG_IGN)
        raise SystemExit(128 + signum)
    signal.signal(signal.SIGTERM, terminate)
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=Path, required=True)
    p.add_argument('--library', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--control', choices=['cpu','sleep'])
    p.add_argument('--child-timeout',type=int,default=45)
    p.add_argument('--trace-seconds',type=int,default=60)
    p.add_argument('--finalize-timeout',type=int,default=20)
    p.add_argument('--template',default='System Trace')
    p.add_argument('--instrument',action='append',default=[])
    p.add_argument('--recording-options',type=Path)
    p.add_argument('--dedicated-frames',action='store_true')
    p.add_argument('args', nargs=argparse.REMAINDER)
    a=p.parse_args()
    result = capture(a.binary,a.library,a.output,a.args[1:] if a.args[:1] == ['--'] else a.args,a.control,a.child_timeout,a.trace_seconds,a.finalize_timeout,a.template,a.instrument,a.recording_options,a.dedicated_frames)
    print(json.dumps(result))
    raise SystemExit(0 if result['status'] == 'RECORDED' and result['test_exit'] == (101 if a.control else 0) else 1)
