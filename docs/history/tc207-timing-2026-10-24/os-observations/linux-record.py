#!/usr/bin/env python3
"""Owned-task perf SWITCH records. No root, tracefs, sysctl, retries or product edits."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import signal
import subprocess
import sys


def command(argv, prefix, timeout=30):
    child = None
    result = dict(argv=[str(x) for x in argv], exit=None, timeout=False)
    try:
        with prefix.with_suffix('.stdout').open('wb') as out, prefix.with_suffix('.stderr').open('wb') as err:
            child = subprocess.Popen(argv, stdout=out, stderr=err, start_new_session=True)
            try:
                result['exit'] = child.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                result['timeout'] = True
    except OSError as exc:
        result['error'] = repr(exc)
    finally:
        if child is not None:
            # The fresh process group contains only this owned perf/worker/test.
            # Clean descendants even if perf exited before its child.
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            result['exit'] = child.wait(timeout=5)
        prefix.with_suffix('.command.json').write_text(json.dumps(result, indent=2) + '\n')
    return result


def valid_result(perf, receipt):
    return perf.get('exit') == 0 and not perf.get('timeout') and bool(receipt and receipt.get('experiment_valid'))


def record_argv(perf, output, binary, label, control):
    return [str(perf), 'record', '--switch-events', '--clockid', 'mono',
            '-e', 'task-clock:u', '-F', '99', '-o', str(output / (label + '.data')),
            '--', sys.executable, str(Path(__file__).resolve()), '--worker',
            str(binary), str(output), label, control or 'natural']


def worker(argv):
    spec = importlib.util.spec_from_file_location('existing_collect', Path(__file__).resolve().parent.parent / 'collect.py')
    collect = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(collect)
    binary, output, label, control = argv
    receipt = collect.run(Path(binary), Path(output), label, None if control == 'natural' else control)
    return 0 if receipt['experiment_valid'] else 1


def run(perf, binary, output, label, control=None):
    result = command(record_argv(perf, output, binary, label, control), output / (label + '-record'))
    receipt_path = output / (label + '.receipt.json')
    receipt = json.loads(receipt_path.read_text()) if receipt_path.exists() else None
    exports = {}
    for kind, options in [('events', ['script', '--ns', '--show-switch-events']),
                          ('dump', ['script', '-D']), ('attributes', ['evlist', '-v'])]:
        exports[kind] = command([str(perf), *options, '-i', str(output / (label + '.data'))], output / (label + '-' + kind))
    return dict(label=label, perf=result, test_receipt=receipt,
                exports=exports, valid=valid_result(result, receipt) and all(x['exit'] == 0 and not x['timeout'] for x in exports.values()))


def main():
    def terminate(signum, _frame):
        signal.signal(signum, signal.SIG_IGN)
        raise SystemExit(128 + signum)
    signal.signal(signal.SIGTERM, terminate)
    if len(sys.argv) > 1 and sys.argv[1] == '--worker':
        return worker(sys.argv[2:])
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--perf', type=Path, required=True)
    p.add_argument('--binary', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    if platform.system() != 'Linux':
        p.error('Linux required')
    a.output.mkdir(parents=True, exist_ok=False)
    provenance = dict(source_base='f5e20628b4da774bc640ecc1f5351c7179849ed8', kernel=platform.release(),
                      binary=str(a.binary), binary_sha256=hashlib.sha256(a.binary.read_bytes()).hexdigest(),
                      collector_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                      wall_limit_ns=100_000_000, retries=0,
                      plan=dict(controls=['cpu', 'sleep'], natural_serial_processes=1, natural_parallel_processes=12,
                                cases_per_natural_process=4, test_threads=1, timeout_seconds=30),
                      scope='owned perf worker and test descendants only; no system-wide capture')
    (a.output / 'provenance.json').write_text(json.dumps(provenance, indent=2) + '\n')
    rows = [run(a.perf, a.binary, a.output, 'control-' + c, c) for c in ['cpu', 'sleep']]
    rows.append(run(a.perf, a.binary, a.output, 'natural-serial'))
    with ThreadPoolExecutor(max_workers=12) as pool:
        futures = [pool.submit(run, a.perf, a.binary, a.output, 'natural-parallel-' + str(n)) for n in range(12)]
        rows.extend(f.result() for f in futures)
    summary = dict(results=rows, all_experiments_valid=all(x['valid'] for x in rows),
                   binary_unchanged=hashlib.sha256(a.binary.read_bytes()).hexdigest() == provenance['binary_sha256'])
    (a.output / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    return 0 if summary['all_experiments_valid'] and summary['binary_unchanged'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
