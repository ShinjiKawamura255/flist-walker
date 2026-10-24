#!/usr/bin/env python3
"""Read-only Linux/WSL perf prerequisites; no sudo/install/sysctl/mount/trace."""
import json
from pathlib import Path
import platform
import subprocess
import sys
import time


def classify_events(results):
    available=set()
    readable=False
    for row in results:
        if row.get('status')=='READ':
            readable=True
            available.update(row['content'].splitlines())
    required={'sched:sched_switch','sched:sched_wakeup','sched:sched_wakeup_new'}
    return dict(status='AVAILABLE' if required<=available else ('MISSING' if readable else 'UNKNOWN'),
                required=sorted(required),missing=sorted(required-available))


def read(path):
    try:
        return dict(path=path,status='READ',content=Path(path).read_text())
    except OSError as exc:
        return dict(path=path,status='UNAVAILABLE',errno=exc.errno,error=str(exc))


def command(argv):
    try:
        r=subprocess.run(argv,capture_output=True,text=True,timeout=10)
        return dict(argv=argv,exit=r.returncode,stdout=r.stdout,stderr=r.stderr)
    except (OSError,subprocess.TimeoutExpired) as exc:
        return dict(argv=argv,exit=None,error=repr(exc))


def main():
    if platform.system()!='Linux':
        print(json.dumps(dict(status='NOT RUN',reason='Run inside the intended Linux/WSL distribution')))
        return 2
    trace=[read(p) for p in ('/sys/kernel/tracing/available_events','/sys/kernel/debug/tracing/available_events')]
    settings=[read(p) for p in ('/proc/sys/kernel/perf_event_paranoid','/proc/sys/kernel/kptr_restrict','/proc/sys/kernel/sched_schedstats','/proc/pressure/cpu','/etc/os-release')]
    probes=[command(a) for a in (['uname','-r'],['uname','-m'],['id','-u'],['getconf','_NPROCESSORS_ONLN'],['perf','--version'],['perf','version','--build-options'],['perf','list','sched:sched_*'],['stat','-f','-c','%T','.'])]
    print(json.dumps(dict(observed_utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),
                         purpose='Prerequisites only; no perf_event_open/capture or privilege change',
                         kernel=platform.release(),wsl_marker_hint=('microsoft' in platform.release().lower() or 'wsl' in platform.release().lower()),
                         tracepoint_inventory=classify_events(trace),tracefs_reads=trace,settings=settings,commands=probes,
                         capture_readiness='NOT CONFIRMED; availability does not establish recording permission or complete events',
                         host_vcpu_wait='NOT OBSERVED; guest scheduling cannot exclude host pause'),indent=2))
    return 0


if __name__=='__main__':
    raise SystemExit(main())
