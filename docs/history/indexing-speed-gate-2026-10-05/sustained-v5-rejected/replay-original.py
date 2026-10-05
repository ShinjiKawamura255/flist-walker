"""Verify original rejected v5 bytes and all three source CLI outcomes; never recalibrate."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile

p = argparse.ArgumentParser()
p.add_argument('archive', type=Path)
p.add_argument('manifest', type=Path)
p.add_argument('output', type=Path)
a = p.parse_args()
m = json.loads(a.manifest.read_text())
assert hashlib.sha256(a.archive.read_bytes()).hexdigest() == m['archive_sha256']
expected = {e['path']: e for e in m['files']}
records = []
with tempfile.TemporaryDirectory(prefix='flistwalker-rejected-v5-replay-') as tmp:
    root = Path(tmp)
    with tarfile.open(a.archive, 'r:gz') as archive:
        members = archive.getmembers()
        assert len(members) == len(expected)
        for member in members:
            relative = Path(member.name)
            assert member.isfile() and member.name in expected
            assert not relative.is_absolute() and '..' not in relative.parts
            data = archive.extractfile(member).read()
            entry = expected.pop(member.name)
            assert len(data) == entry['bytes'] and hashlib.sha256(data).hexdigest() == entry['sha256']
            dest = root / relative
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(data)
    assert not expected and not (root / 'complete.json').exists()
    assert not (root / 'frozen-before-held-out.json').exists()
    assert all(not (root / ('session-'+str(i))).exists() for i in (2,3,4,5))
    assert (root / 'stopped.json').exists()
    protocol = json.loads((root / 'protocol-before-outcomes.json').read_text())
    assert protocol['core_source'] == '83aa173deeb16308c838606313131843247f566c'
    original = root / 'source-contract/scripts/indexing_perf.py'
    total_rows = 0
    components = set()
    for index, run in enumerate(('37330978548',), 1):
        folder = root / f'session-{index}'
        state = json.loads((folder / 'state-final.json').read_text())
        assert state['headSha'] == protocol['core_source'] and state['attempt'] == 1
        packets = sorted((folder / 'artifacts').iterdir())
        assert len(packets) == 3
        groups = set()
        for packet in packets:
            receipt = json.loads((packet / 'receipt.json').read_text())
            group = receipt['group']
            assert group not in groups
            groups.add(group)
            assert receipt['source_before']['head'] == protocol['core_source']
            assert receipt['runner']['GITHUB_RUN_ID'] == run
            assert receipt['comparison']['enforced'] is False
            validate = subprocess.run([sys.executable, str(original), 'validate', '--group', group, str(packet)], capture_output=True, text=True, timeout=60)
            assert validate.returncode == 0, validate.stderr
            normal = json.loads(validate.stdout)
            assert normal['status'] == 'comparison-calibration' and normal['timing_gate'] is None
            dest = root / f'replayed-{index}-{group}.json'
            proposed = subprocess.run([sys.executable, str(original), 'compare-proposal', '--group', group, '--output', str(dest), str(packet)], capture_output=True, text=True, timeout=60)
            saved = json.loads((folder / (group+'-proposal.json')).read_text())
            expected_status = saved['status']
            assert proposed.returncode == (0 if expected_status == 'pass' else 1), proposed.stderr
            report = json.loads(dest.read_text())
            assert report == saved and report['null_rust_sources_equal']
            assert report['policy'] == {'id': 'rcr-median-v3', 'slowdown_ratio': 1.5, 'reference_drift_ratio': 1.25}
            assert report['enforced_count']==report['diagnostic_count']==report['decision_count']//2
            assert normal['enforced_count']==report['enforced_count'] and normal['diagnostic_count']==report['diagnostic_count']
            assert normal['proposal_status']==expected_status
            assert all(d['enforced'] is (d['statistic']=='median') and (d['status']=='diagnostic' if not d['enforced'] else d['status'] in ('pass','indeterminate','timing-fail')) for d in report['decisions'])
            for role, names in receipt['comparison']['legs'].items():
                leg = json.loads((packet / names['receipt']).read_text())
                assert leg['run_id'] not in components
                components.add(leg['run_id'])
                total_rows += sum('INDEX_PERF_SAMPLE ' in line for line in (packet / names['raw']).read_text().splitlines())
            records.append({'session': index, 'run': run, 'group': group, 'validate_exit': validate.returncode, 'validation_status': normal['status'], 'proposal_exit': proposed.returncode, 'proposal_status': report['status'], 'decision_count': report['decision_count'], 'enforced_count':report['enforced_count'], 'diagnostic_count':report['diagnostic_count']})
        assert groups == {'f1', 'matched', 'stable'}
assert len(records) == 3 and len(components) == 9 and total_rows == 1386
assert sum(r['decision_count'] for r in records) == 136
assert sum(r['enforced_count'] for r in records)==sum(r['diagnostic_count'] for r in records)==68
with a.output.open('x') as stream:
    json.dump({'original_contract_roundtrip_replay': 'PASS', 'source': protocol['core_source'], 'archive_sha256': m['archive_sha256'], 'results': records, 'raw_rows': total_rows, 'component_sessions': len(components), 'sessions2_5': 'NOT RUN', 'calibration_adopted': False, 'enforcement_active': False}, stream, indent=2)
    stream.write('\n')
assert {r['group']:r['proposal_status'] for r in records}=={'f1':'pass','matched':'indeterminate','stable':'pass'}
print('PASS: 3 original v5 group replays / 6 CLI invocations, 1386 rows, 136 observations/68 median/68 MAX diagnostics; original rejection retained')
