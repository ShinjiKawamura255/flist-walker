#!/usr/bin/env python3
"""Reconstruct case-thread evidence and source-order bounds, never exact frame times."""
import importlib.util
import json
from pathlib import Path
import sys
import xml.etree.ElementTree as E

_spec=importlib.util.spec_from_file_location('tc207_projection',Path(__file__).with_name('project.py'))
projection=importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(projection)
TARGET='tc_207_background_finalization_survives_promotion_to_active'
CASE='app::tests::tab_background_responses::'+TARGET


def prove(data):
    unknown=dict(status='UNKNOWN',wall_ns=None,cpu_total_ns=None,exact_frame_window_ns=None)
    if data['test']!=TARGET or len(data['matched_tids'])!=1 or not data['strict_promotion_failure_confirmed']:return unknown
    pid=data['pid'];tid=data['matched_tids'][0]
    for key in ['states','transition_stacks']:
        if any(r['pid']!=pid or r['tid']!=tid for r in data[key]):raise ValueError('foreign proof PID/TID')
    states=data['states']
    if states:projection.legacy.integrate(states,min(r['start'] for r in states),max(r['end'] for r in states))
    ends=[r['start'] for r in states if r['state']=='Terminated']
    if len(ends)!=1:return unknown
    end=ends[0]
    # Source: start -> poll -> immediate strict assertion -> uncaught test panic.
    # The failed timer read precedes this thread's termination. A previous poll
    # must return before the failed frame's start, which is <= end -100ms.
    # An in-poll off-core interval ending later cannot be in a previous call;
    # immediate panic also excludes a later call. 1ms is a sensitivity margin,
    # not a claimed kernel-event error bound or a changed acceptance threshold.
    latest_start=end-100_000_000
    admission_margin=1_000_000
    selected=[]
    for row in states:
        if row['state']!='Preempted' or row['end']<=latest_start+admission_margin or row['end']>=end-admission_margin:continue
        witnesses=[s for s in data['transition_stacks'] if s['time']==row['start'] and any(TARGET in f for f in s['stack']) and any('poll_index_response_with_budget_for_test' in f for f in s['stack'])]
        if witnesses:selected.append(dict(state=row,witnesses=witnesses))
    if not selected:return unknown
    observed=sum(x['state']['end']-x['state']['start'] for x in selected)
    sensitivity=sum(max(0,x['state']['end']-x['state']['start']-2_000_000) for x in selected)
    return dict(status='RECORDED_PREEMPTION_SUFFICIENT' if observed>100_000_000 else 'RECORDED_PREEMPTION_CONTRIBUTION',
                pid=pid,tid=tid,test=CASE,threshold_ns=100_000_000,thread_termination_trace_ns=end,
                latest_possible_failed_start_trace_ns=latest_start,inference_admission_margin_ns=admission_margin,
                selected_intervals=selected,observed_preempted_ns=observed,state_edge_1ms_sensitivity_ns=sensitivity,
                wall_ns=None,cpu_total_ns=None,exact_frame_window_ns=None,
                limits=['source-order containment inference; precise failed-frame start/end and total CPU unavailable',
                        'Preempted retained as OS label; not all asserted involuntary runqueue starvation',
                        '1ms inner-edge sensitivity is not a certified trace error bound',
                        'no independent loss-free trace certification; raw export/decoder limitations remain',
                        'old historical file-filter/Walker causes are not established by this new stress-condition failure'])


def extract(root):
    receipt=json.loads((root/'receipt.json').read_text())
    if receipt['status']!='RECORDED' or receipt['tracer_exit']!=0:raise ValueError('incomplete trace')
    expected=projection.legacy.expected_dumps({'argv':['--nocapture']})
    _,summary=projection.runner_outcomes((root/'test.log').read_text(),expected)
    failures=projection.unmeasured_tc207_failures(summary['failed_tests'],(root/'test.stderr').read_text(),[])
    confirmed=any(r['test']==CASE and r['status']=='STRICT_WALL_FAILURE_NO_FRAME_WINDOW' for r in failures)
    if not confirmed:raise ValueError('case-owned strict promotion failure not confirmed')
    pid=receipt['pid'];tables=list(E.parse(root/'toc.xml').find('.//data'))
    path=root/'events.xml'
    if path.stat().st_size>8*1024**3:raise ValueError('XML byte resource cap exceeded')
    ids=projection.index_ids(path)
    def res(e):return projection.legacy.resolve(e,ids)
    def num(e):return int(res(e).text)
    def proc(e):return num(res(e).find('pid'))
    def tid(e):return num(res(e).find('tid'))
    def stack(e):return [] if e.tag=='sentinel' else [res(f).get('name','UNKNOWN') for f in res(e).findall('frame')]
    matches=[]
    for i,row in projection.iter_rows(path):
        schema=tables[i-1].get('schema');c=list(row)
        if schema=='time-profile' and len(c)==7 and proc(c[2])==pid:names=stack(c[6])
        elif schema=='context-switch-sample' and len(c)==6 and proc(c[2])==pid:names=stack(c[5])
        else:continue
        if any(TARGET in name for name in names):matches.append(dict(pid=pid,tid=tid(c[1]),time=num(c[0]),source=schema,stack=names))
    tids={x['tid'] for x in matches}
    if len(tids)>8:raise ValueError('ambiguous thread candidate resource cap exceeded')
    states=[];switches=[];transitions=[]
    for i,row in projection.iter_rows(path):
        schema=tables[i-1].get('schema');c=list(row)
        if schema=='thread-state' and len(c)>=5 and proc(c[4])==pid and tid(c[1]) in tids:states.append(dict(pid=pid,tid=tid(c[1]),start=num(c[0]),end=num(c[0])+num(c[3]),state=res(c[2]).text or res(c[2]).get('fmt')))
        elif schema=='context-switch' and len(c)>=4 and proc(c[3])==pid and tid(c[1]) in tids:switches.append(dict(pid=pid,tid=tid(c[1]),time=num(c[0]),event=res(c[2]).text or res(c[2]).get('fmt')))
        elif schema=='context-switch-sample' and len(c)==6 and proc(c[2])==pid and tid(c[1]) in tids:transitions.append(dict(pid=pid,tid=tid(c[1]),time=num(c[0]),stack=stack(c[5])))
    return dict(pid=pid,test=TARGET,matched_tids=sorted(tids),strict_promotion_failure_confirmed=confirmed,
                matching_samples=matches,states=states,switches=switches,transition_stacks=transitions)


if __name__=='__main__':
    if sys.argv[1]=='--replay':print(json.dumps(prove(json.loads(Path(sys.argv[2]).read_text())),indent=2))
    else:
        root=Path(sys.argv[1]);data=extract(root)
        with (root/'promotion-inputs.json').open('x') as f:json.dump(data,f,separators=(',',':'));f.write('\n')
        with (root/'promotion-proof.json').open('x') as f:json.dump(prove(data),f,indent=2);f.write('\n')
