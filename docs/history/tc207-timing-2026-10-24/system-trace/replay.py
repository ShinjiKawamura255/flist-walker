#!/usr/bin/env python3
"""Recompute frame-local attribution from sanitized native trace inputs."""
import json
from pathlib import Path
import sys

from analyze import clock_bridge, expected_dumps, frame_window, integrate, validate_frames


def replay(data):
    validate_frames(data['frames'], data['receipt']['pid'], expected=expected_dumps(data['receipt']))
    field, offset = clock_bridge(data['frames'], data['anchors'])
    results = []
    for frame in data['frames']:
        lo, hi = frame_window(frame, offset, field)
        totals = None if hi < lo else integrate(
            [r for r in data['states'] if r['tid'] == frame['native_thread_id']], lo, hi)
        results.append(dict(frame=frame, states_ns=totals))
    maximum = max(results, key=lambda r: r['frame']['wall_ns'])
    return dict(frames=len(results), threshold_ns=100_000_000,
                wall_failures=sum(not r['frame']['wall_under_100ms'] for r in results),
                uncertain_frames=sum(r['states_ns'] is None for r in results),
                coverage_gap_frames=sum(bool(r['states_ns'] and r['states_ns'].get('UNKNOWN')) for r in results),
                calibration_clock=field, offset_interval_ns=offset,
                maximum_wall_frame=maximum)


if __name__ == '__main__':
    print(json.dumps(replay(json.loads(Path(sys.argv[1]).read_text())), indent=2))
