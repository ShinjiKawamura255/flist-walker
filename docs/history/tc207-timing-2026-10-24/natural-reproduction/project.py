#!/usr/bin/env python3
"""Project complete native trace exports onto measured threads; retain failures."""
from collections import defaultdict
import importlib.util
import json
from pathlib import Path
import re
import sys
import xml.etree.ElementTree as E

_spec=importlib.util.spec_from_file_location('tc207_trace_legacy',Path(__file__).parent.parent/'system-trace/analyze.py')
legacy=importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(legacy)


def validate_argv(argv):
    if argv not in [['--nocapture'],['--nocapture','--test-threads=48']]:
        raise ValueError('not a declared complete-suite parallel condition')


def runner_outcomes(text,expected):
    outcomes={}
    for name in expected:
        matches=re.findall(r'^test '+re.escape(name)+r' \.\.\. (ok|FAILED)\s*$',text,re.M)
        if len(matches)!=1:raise ValueError('missing/duplicate runner case '+name)
        outcomes[name]=matches[0]
    rows=re.findall(r'^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; \d+ measured; (\d+) filtered out;',text,re.M)
    if not rows:raise ValueError('missing final test summary')
    state,passed,failed,ignored,filtered=rows[-1]
    summary=dict(state=state,passed=int(passed),failed=int(failed),ignored=int(ignored),filtered=int(filtered),failed_tests=re.findall(r'^test (.+?) \.\.\. FAILED\s*$',text,re.M))
    if len(summary['failed_tests'])!=summary['failed'] or len(set(summary['failed_tests']))!=len(summary['failed_tests']):raise ValueError('missing/duplicate full-suite failure names')
    if summary['filtered'] or (state=='ok')!=(summary['failed']==0):raise ValueError('inconsistent/filtered suite summary')
    if sum(v=='FAILED' for v in outcomes.values())>summary['failed']:raise ValueError('inconsistent runner failures')
    return outcomes,summary


def admit_cases(frames,pid,outcomes,expected):
    if set(outcomes)!=set(expected):raise ValueError('missing/unexpected runner cases')
    counts=legacy.validate_frames(frames,pid)
    if set(counts)-set(expected):raise ValueError('unexpected measured case')
    missing={}
    failed=[]
    for name,n in expected.items():
        actual=counts.get(name,0)
        if outcomes[name]=='ok':
            if actual!=n:raise ValueError('missing passed-case dump '+name)
        elif outcomes[name]=='FAILED':
            if actual>n:raise ValueError('extra failed-case dumps')
            if actual<n:missing[name]=n-actual
        else:raise ValueError('unknown runner outcome')
        rows=[f for f in frames if f['test']==name]
        bad=[f for f in rows if not f['wall_under_100ms']]
        if bad:
            if outcomes[name]!='FAILED' or len(bad)!=1 or rows[-1] is not bad[0] or bad[0]['frame']!=bad[0]['frames_observed']-1:
                raise ValueError('wall failure contradicts first-failure termination')
            failed.extend(bad)
    return dict(dump_counts=counts,missing_failed_dumps=missing,wall_failures=failed)


def assertion_receipt(stderr,frame):
    prefix='background finalization frame exceeded 100ms: request_id='+str(frame['request_id'])+', frame='+str(frame['frame'])+','
    contexts=re.findall(r"^thread '([^']+)'(?: \(\d+\))? panicked at [^\n]*\n([^\n]*)",stderr,re.M)
    matches=[message for name,message in contexts if name==frame['test'] and message.startswith(prefix)]
    return dict(test=frame['test'],request_id=frame['request_id'],frame=frame['frame'],
                status='CONFIRMED' if len(matches)==1 else 'UNKNOWN',
                reason=None if len(matches)==1 else 'missing/ambiguous case-owned panic line; no stderr reconstruction')


def unmeasured_tc207_failures(failed_tests,stderr,frames):
    measured_bad={f['test'] for f in frames if not f['wall_under_100ms']}
    contexts=re.findall(r"^thread '([^']+)'(?: \(\d+\))? panicked at [^\n]*\n([^\n]*)",stderr,re.M)
    promotion='app::tests::tab_background_responses::tc_207_background_finalization_survives_promotion_to_active'
    results=[]
    for name in failed_tests:
        if not name.startswith('app::tests::tab_background_responses::tc_207_') or name in measured_bad:continue
        messages=[message for case,message in contexts if case==name]
        strict=name==promotion and messages==['promotion finalization frame exceeded budget']
        results.append(dict(test=name,status='STRICT_WALL_FAILURE_NO_FRAME_WINDOW' if strict else 'FAILED_REASON_UNKNOWN',
                            threshold_ns=100_000_000 if strict else None,wall_lower_bound_ns=100_000_000 if strict else None,
                            wall_ns=None,native_thread_id=None,trace_correlation='UNKNOWN',
                            assertion='promotion finalization frame exceeded budget' if strict else None))
    return results


def index_ids(path,max_ids=4_000_000,max_numeric_id=128_000_000):
    """Index only referenced definitions; a bitset still rejects every duplicate."""
    def number(key):
        if not key.isascii() or not key.isdecimal() or len(key)>9:raise ValueError('unsupported XML numeric identity')
        value=int(key)
        if value<=0 or value>max_numeric_id or str(value)!=key:raise ValueError('XML numeric identity range exceeded')
        return value
    refs=set();seen=bytearray();parents=[]
    for event,e in E.iterparse(path,events=('start','end')):
        if event=='start':
            parents.append(e)
            key=e.get('id')
            if key:
                value=number(key);byte,bit=divmod(value,8)
                if byte>=len(seen):seen.extend(bytes(byte+1-len(seen)))
                if seen[byte]&(1<<bit):raise ValueError('duplicate XML identity')
                seen[byte]|=1<<bit
            ref=e.get('ref')
            if ref:
                number(ref);refs.add(ref)
                if len(refs)>max_ids:raise ValueError('referenced XML identity resource cap exceeded')
            continue
        if e.tag=='row' and len(parents)>1:parents[-2].remove(e)
        parents.pop()
    ids={};parents=[]
    for event,e in E.iterparse(path,events=('start','end')):
        if event=='start':parents.append(e);continue
        key=e.get('id')
        if key in refs:ids[key]=e
        if e.tag=='row' and len(parents)>1:parents[-2].remove(e)
        parents.pop()
    if refs-set(ids):raise ValueError('missing referenced XML definition')
    return ids


def iter_rows(path):
    parents=[];table=None
    for event,e in E.iterparse(path,events=('start','end')):
        if event=='start':
            parents.append(e)
            if e.tag=='node':
                match=re.search(r'table\[(\d+)\]$',e.get('xpath',''))
                if not match:raise ValueError('unmapped table identity')
                table=int(match.group(1))
            continue
        if e.tag=='row':
            if table is None:raise ValueError('row outside mapped table')
            yield table,e
            parents[-2].remove(e)
        if e.tag=='node':table=None
        parents.pop()


def project(root):
    receipt=json.loads((root/'receipt.json').read_text())
    if receipt['status']!='RECORDED' or receipt.get('tracer_exit')!=0 or receipt.get('test_exit') not in [0,101]:
        raise ValueError('incomplete/failed recording; retain receipt without numeric admission')
    if receipt.get('control') is not None or not receipt.get('dedicated_frames'):raise ValueError('not an uninjected dedicated natural capture')
    validate_argv(receipt['argv']);pid=receipt['pid'];nonce=receipt['nonce']
    frames=legacy.parse_frames(legacy.frame_text(root,receipt),dedicated=True)
    expected=legacy.expected_dumps({'argv':['--nocapture']})
    raw=(root/'test.log').read_text();outcomes,summary=runner_outcomes(raw,expected)
    if (summary['failed']==0)!=(receipt['test_exit']==0):raise ValueError('runner/exit disagreement')
    admission=admit_cases(frames,pid,outcomes,expected)
    stderr=(root/'test.stderr').read_text()
    admission['assertion_receipts']=[assertion_receipt(stderr,f) for f in admission['wall_failures']]
    admission['unmeasured_tc207_failures']=unmeasured_tc207_failures(summary['failed_tests'],stderr,frames)
    anchors=[json.loads(m.group(1)) for m in re.finditer(r'TC207_CLOCK (\{[^\n]*\})',raw)]
    if len(anchors)!=2 or {a['label'] for a in anchors}!={'process-start','process-exit'}:raise ValueError('missing/extra clock anchors')
    for a in anchors:
        if a['pid']!=pid or a['nonce']!=nonce or not a['signpost_enabled']:raise ValueError('foreign/inactive clock anchor')
    clock_tids={a['native_thread_id'] for a in anchors};measured_tids={f['native_thread_id'] for f in frames}
    tables=list(E.parse(root/'toc.xml').find('.//data'))
    path=root/'events.xml'
    if path.stat().st_size>8*1024**3:raise ValueError('XML byte resource cap exceeded')
    ids=index_ids(path)
    def res(e):return legacy.resolve(e,ids)
    def num(e):return int(res(e).text)
    def text(e):return res(e).text or res(e).get('fmt') or ''
    def process(e):return num(res(e).find('pid'))
    def thread(e):return num(res(e).find('tid'))
    def stack(e):
        if e.tag=='sentinel':return []
        return [res(f).get('name','UNKNOWN') for f in res(e).findall('frame')]
    states=[];switches=[];samples=[];supplemental=[];transition=[];signposts={}
    for index,row in iter_rows(path):
        if not 1<=index<=len(tables):raise ValueError('foreign table index')
        table=tables[index-1];schema=table.get('schema');c=list(row)
        if schema=='os-signpost-arg' and len(c)==10 and process(c[5])==pid and thread(c[6]) in clock_tids and text(c[3])=='TC207Clock':
            key=(num(c[0]),thread(c[6]));args=signposts.setdefault(key,{})
            if text(c[4]) in args:raise ValueError('duplicate signpost argument')
            args[text(c[4])]=text(c[9])
        elif schema in ['thread-state','context-switch','time-profile','context-switch-sample']:
            process_col={'thread-state':4,'context-switch':3,'time-profile':2,'context-switch-sample':2}[schema]
            if len(c)<=process_col or process(c[process_col])!=pid or thread(c[1]) not in measured_tids:continue
            tid=thread(c[1]);time=num(c[0])
            if schema=='thread-state' and len(c)>=5:states.append(dict(pid=pid,tid=tid,start=time,end=time+num(c[3]),state=text(c[2])))
            elif schema=='context-switch' and len(c)>=4:switches.append(dict(pid=pid,tid=tid,time=time,event=text(c[2])))
            elif schema=='time-profile' and len(c)==7:
                entry=dict(pid=pid,tid=tid,time=time,state=text(c[4]),stack=stack(c[6]),source_table=index)
                (samples if legacy.profile_sample_kind(table.attrib,entry['state'])=='cpu' else supplemental).append(entry)
            elif schema=='context-switch-sample' and len(c)==6:transition.append(dict(pid=pid,tid=tid,time=time,stack=stack(c[5])))
    for a in anchors:
        matches=[(time,tid) for (time,tid),args in signposts.items() if args.get('arg0')==a['label'] and args.get('arg1')==nonce and args.get('arg2')==str(a['mono_before_ns'])]
        if len(matches)!=1 or matches[0][1]!=a['native_thread_id']:raise ValueError('missing/ambiguous signpost anchor')
        a['trace_ns']=matches[0][0]
    field,offset=legacy.clock_bridge(frames,anchors)
    bounds={}
    for f in frames:
        lo,hi=f[field][0]-offset[1],f[field][1]-offset[0];tid=f['native_thread_id']
        a,b=bounds.get(tid,(lo,hi));bounds[tid]=(min(lo,a),max(hi,b))
    states=[r for r in states if r['start']<bounds[r['tid']][1] and r['end']>bounds[r['tid']][0]]
    def clip(rows):return [r for r in rows if bounds[r['tid']][0]<=r['time']<=bounds[r['tid']][1]]
    data=dict(receipt=receipt,anchors=anchors,frames=frames,states=states,switches=clip(switches),cpu_samples=clip(samples),supplemental_samples=clip(supplemental),transition_stacks=clip(transition),runner_outcomes=outcomes,runner_summary=summary,admission=admission,calibration_clock=field,offset_interval_ns=offset)
    return data


def summarize(data):
    expected=legacy.expected_dumps({'argv':['--nocapture']})
    admission=admit_cases(data['frames'],data['receipt']['pid'],data['runner_outcomes'],expected)
    field,offset=legacy.clock_bridge(data['frames'],data['anchors'])
    by_tid=defaultdict(list)
    for row in data['states']:
        if row['pid']!=data['receipt']['pid']:raise ValueError('foreign state PID')
        by_tid[row['tid']].append(row)
    results=[]
    for f in data['frames']:
        lo,hi=legacy.frame_window(f,offset,field)
        totals=None if hi<lo else legacy.integrate(by_tid[f['native_thread_id']],lo,hi)
        results.append(dict(frame=f,conservative_trace_window_ns=[lo,hi],states_ns=totals))
    return dict(threshold_ns=100_000_000,frames=len(results),runner_summary=data['runner_summary'],dump_counts=admission['dump_counts'],missing_failed_dumps=admission['missing_failed_dumps'],calibration_clock=field,offset_interval_ns=offset,
                assertion_receipts=data['admission']['assertion_receipts'],unmeasured_tc207_failures=data['admission']['unmeasured_tc207_failures'],wall_failures=[r for r in results if not r['frame']['wall_under_100ms']],maximum_wall_frame=max(results,key=lambda r:r['frame']['wall_ns']),
                maximum_runnable_frame=max(results,key=lambda r:(r['states_ns'] or {}).get('Runnable',0)),
                uncertain_frames=sum(r['states_ns'] is None for r in results),coverage_gap_frames=sum(bool(r['states_ns'] and r['states_ns'].get('UNKNOWN')) for r in results))


if __name__=='__main__':
    root=Path(sys.argv[1]);inputs=project(root);summary=summarize(inputs)
    # Dedicated new names preserve raw and any earlier analysis. Never overwrite.
    with (root/'projected-inputs.json').open('x') as f:json.dump(inputs,f,separators=(',',':'));f.write('\n')
    with (root/'projected-summary.json').open('x') as f:json.dump(summary,f,indent=2);f.write('\n')
    print(json.dumps({k:summary[k] for k in ['frames','uncertain_frames','coverage_gap_frames','runner_summary']}))
