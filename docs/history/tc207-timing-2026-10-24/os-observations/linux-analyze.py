#!/usr/bin/env python3
"""Analyze this capture's exact little-endian perf TID|TIME metadata layout."""
from collections import Counter
import gzip
import json
from pathlib import Path
import re
import struct
import sys


def switch_record(data):
    if len(data) != 24:
        raise ValueError('unsupported SWITCH layout; expected header + PID/TID/TIME')
    kind, misc, size, pid, tid, ns = struct.unpack('<IHHIIQ', data)
    if kind != 14 or size != 24:
        raise ValueError('not a task-scoped SWITCH')
    return dict(pid=pid, tid=tid, ns=ns, out=bool(misc & 0x2000), preempt=bool(misc & 0x4000))


def records(path):
    data = path.read_bytes()
    if data[:8] != b'PERFILE2' or len(data) < 56:
        raise ValueError('unsupported perf header/endianness')
    offset, length = struct.unpack_from('<QQ', data, 40)
    end = offset + length
    if offset < 56 or end > len(data):
        raise ValueError('invalid data section')
    counts, events = Counter(), []
    while offset < end:
        if offset + 8 > end:
            raise ValueError('truncated event header')
        kind, _, size = struct.unpack_from('<IHH', data, offset)
        if size < 8 or offset + size > end:
            raise ValueError('truncated event body')
        counts[kind] += 1
        if kind == 14:
            events.append(switch_record(data[offset:offset + size]))
        offset += size
    return counts, events


def overlap(frame, events):
    unavailable = dict(switch_off_core_overlap_ns=None, preempt_out_overlap_ns=None,
                       reason='missing identity, boundary coverage, or alternating event sequence')
    # Raw per-CPU buffers can be delivered out of chronological order after migration.
    rows = sorted((x for x in events if x['pid'] == frame['pid'] and x['tid'] == frame['native_thread_id']), key=lambda x: x['ns'])
    lo, hi = frame['monotonic_window_ns']
    margin = frame['measurement_overhead_ns']
    lo, hi = lo + margin, hi - margin
    if not rows or hi < lo or rows[0]['ns'] > lo or rows[-1]['ns'] < hi:
        return unavailable
    intervals, pending, previous = [], None, None
    for row in rows:
        if previous is not None and (row['ns'] < previous['ns'] or row['out'] == previous['out']):
            return unavailable
        if row['out']:
            pending = row
        elif pending is not None:
            intervals.append((pending, row))
            pending = None
        previous = row
    clipped = [dict(start_ns=max(lo, start['ns']), end_ns=min(hi, end['ns']),
                    switch_out_preempt=start['preempt']) for start, end in intervals
               if max(lo, start['ns']) < min(hi, end['ns'])]
    return dict(switch_off_core_overlap_ns=sum(x['end_ns'] - x['start_ns'] for x in clipped),
                preempt_out_overlap_ns=sum(x['end_ns'] - x['start_ns'] for x in clipped if x['switch_out_preempt']),
                intervals=clipped, conservative_window_ns=[lo, hi],
                reason='guest switch-out to switch-in overlap; wakeup and host vCPU state not observed')


def clock_consistency(frame):
    return 'INCONSISTENT' if frame['cpu_lower_ns'] is not None and frame['cpu_lower_ns'] > frame['wall_ns'] else 'NOT DISPROVEN; not a calibration'


def analyze(root):
    summary = json.loads((root / 'summary.json').read_text())
    all_rows, captures = [], []
    for result in summary['results']:
        label = result['label']
        attr = (root / (label + '-attributes.stdout')).read_text()
        required = ['sample_type: IP|TID|TIME|PERIOD,', 'sample_id_all: 1,', 'use_clockid: 1,', 'context_switch: 1,']
        if not all(x in attr for x in required) or not re.search(r'\bclockid: 1\s*$', attr):
            raise ValueError(label + ': unsupported metadata or clock')
        counts, events = records(root / (label + '.data'))
        lost = counts[2] + counts[13]
        text = gzip.decompress((root / (label + '.log.gz')).read_bytes()).decode()
        frames = [json.loads(m.group(1)) for m in re.finditer(r'TC207_FRAME (\{[^\n]*\})', text)]
        if len(frames) != result['test_receipt']['frame_count'] or not frames:
            raise ValueError(label + ': frame count')
        for frame in frames:
            if frame['pid'] != result['test_receipt']['pid']:
                raise ValueError(label + ': PID mismatch')
            row = dict(label=label, frame=frame, clock_consistency=clock_consistency(frame),
                       os_overlap=overlap(frame, events) if not lost else dict(switch_off_core_overlap_ns=None, reason='lost records'))
            all_rows.append(row)
        captures.append(dict(label=label, execution_and_export_valid=result['valid'],
                             clockid=1, record_counts=dict(counts), lost_records=lost, frames=len(frames),
                             switch_records=len(events), cpu_fields='raw only; WSL clock inconsistency'))
    natural = [x for x in all_rows if x['label'].startswith('natural')]
    return dict(captures=captures, natural_cases=4 * 13, natural_frames=len(natural),
                natural_wall_failures=sum(not x['frame']['wall_under_100ms'] for x in natural),
                cpu_lower_exceeds_wall=sum(x['clock_consistency'] == 'INCONSISTENT' for x in all_rows),
                overlap_unavailable=sum(x['os_overlap']['switch_off_core_overlap_ns'] is None for x in all_rows),
                max_natural_wall=max(natural, key=lambda x: x['frame']['wall_ns']),
                max_natural_os_overlap=max(natural, key=lambda x: x['os_overlap']['switch_off_core_overlap_ns'] or 0),
                max_natural_preempt_overlap=max(natural, key=lambda x: x['os_overlap'].get('preempt_out_overlap_ns') or 0),
                controls=[x for x in all_rows if x['label'].startswith('control')],
                limits=['No sched_wakeup or blocked stacks: nonpreempt OUT cannot split blocked versus runnable wait',
                        'Preempt bit establishes TASK_RUNNING at switch-out; no continuous runnable-state proof',
                        'No host vCPU trace; no retroactive original-macOS causal attribution',
                        'Raw CPU fields cannot be treated as strict elapsed-time bounds on this WSL host'])


if __name__ == '__main__':
    print(json.dumps(analyze(Path(sys.argv[1])), indent=2))
