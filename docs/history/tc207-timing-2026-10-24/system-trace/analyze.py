#!/usr/bin/env python3
"""Decode actual Xcode27.1 TOC/XML, retaining target-only state/stack records."""
from collections import defaultdict
import json
from pathlib import Path
import re
import sys
import xml.etree.ElementTree as E


def resolve(element, ids):
    seen=set()
    while element.get('ref'):
        key=element.get('ref')
        if key in seen or key not in ids:raise ValueError('missing/cyclic XML reference '+key)
        seen.add(key);element=ids[key]
    return element


def calibrate(anchors,tick=1000):
    if len(anchors)!=2:raise ValueError('both process clock anchors required')
    intervals=[[a['mono_before_ns']-a['trace_ns']-tick,a['mono_after_ns']-a['trace_ns']+tick] for a in anchors]
    lo=max(x[0] for x in intervals);hi=min(x[1] for x in intervals)
    if hi<lo:raise ValueError('inconsistent clock offsets; no average/interpolation')
    return [lo,hi]


def mach_calibrate(anchors):
    rows=[]
    for a in anchors:
        n,d=a['timebase_numer'],a['timebase_denom']
        if n<=0 or d<=0 or a['mach_after']<a['mach_before']:raise ValueError('invalid Mach timebase/window')
        rows.append(dict(mono_before_ns=a['mach_before']*n//d,
                         mono_after_ns=(a['mach_after']*n+d-1)//d,trace_ns=a['trace_ns']))
    if len({(a['timebase_numer'],a['timebase_denom']) for a in anchors})!=1:raise ValueError('changed timebase')
    tick=max((a['timebase_numer']+a['timebase_denom']-1)//a['timebase_denom'] for a in anchors)
    return calibrate(rows,tick=tick)


def frame_window(frame,offset,clock_field='monotonic_window_ns'):
    lo,hi=frame[clock_field];margin=frame['measurement_overhead_ns']
    return [lo+margin-offset[0],hi-margin-offset[1]]


def clock_bridge(frames,anchors):
    if not frames:raise ValueError('empty frame collection')
    schemas={f.get('schema') for f in frames}
    if schemas=={3}:
        for f in frames:
            ticks,window,base=(f.get(k) for k in ['mach_absolute_window_ticks','mach_absolute_window_ns','mach_timebase'])
            if not ticks or not window or not base or len(ticks)!=2 or len(window)!=2 or len(base)!=2:
                raise ValueError('schema3 native clock missing; no MONOTONIC fallback')
            n,d=base
            if n<=0 or d<=0 or ticks[0]>ticks[1] or [t*n//d for t in ticks]!=window:
                raise ValueError('invalid raw Mach ticks/ns')
            if not anchors or any(base!=[a['timebase_numer'],a['timebase_denom']] for a in anchors):
                raise ValueError('frame/anchor timebase mismatch')
        return 'mach_absolute_window_ns',mach_calibrate(anchors)
    if not schemas or schemas=={2}:return 'monotonic_window_ns',calibrate(anchors)
    raise ValueError('unsupported/mixed frame schema')


def profile_sample_kind(attributes,state):
    return 'cpu' if attributes.get('context-switch-sampling')=='0' and state=='Running' else 'supplemental'


def expected_dumps(receipt):
    prefix='app::tests::tab_background_responses::'
    if receipt['argv']==['--nocapture']:
        names={
            'tc_207_background_finish_waits_for_reclaimer_without_dropping_old_snapshot':1,
            'tc_207_background_finished_commits_100k_default_snapshot_within_ui_budget':1,
            'tc_207_background_finished_mixed_owners_settles_incrementally':1,
            'tc_207_background_finished_file_filter_settles_incrementally':1,
            'tc_207_background_finalization_tracks_ignore_policy_changes_both_directions':2,
            'tc_207_late_filter_policy_change_reclaims_old_output_off_ui':1,
            'tc_207_late_filter_policy_change_rolls_back_while_reclaimer_is_full':2,
            'tc_207_background_finished_walker_kind_queue_settles_incrementally':1,
            'tc_207_terminal_waits_in_mailbox_and_activation_commits_complete_snapshot':1,
            'tc_207_promotion_before_terminal_uses_active_finalization_barrier':1,
        }
        return {prefix+k:v for k,v in names.items()}
    if '--exact' not in receipt['argv']:raise ValueError('undeclared capture case selection')
    names=[a for a in receipt['argv'] if a.startswith(prefix)]
    if not names or len(names)!=len(set(names)):raise ValueError('invalid exact case selection')
    return {name:1 for name in names}


def validate_frames(frames,pid,expected=None):
    if not frames:raise ValueError('empty frame collection')
    groups=defaultdict(list)
    required={'schema','pid','native_thread_id','test','request_id','frame','frames_observed','wall_ns','wall_under_100ms','measurement_overhead_ns'}
    for f in frames:
        if not required<=f.keys():raise ValueError('incomplete frame fields')
        if f['schema'] not in [2,3] or f['pid']!=pid or type(f['native_thread_id']) is not int:raise ValueError('foreign frame/schema')
        for key in ['wall_ns','measurement_overhead_ns','request_id','frame','frames_observed']:
            if type(f[key]) is not int or f[key]<0:raise ValueError('invalid nonnegative frame integer')
        if type(f['wall_under_100ms']) is not bool or f['wall_under_100ms']!=(f['wall_ns']<100_000_000):raise ValueError('invalid wall/gate')
        field='mach_absolute_window_ns' if f['schema']==3 else 'monotonic_window_ns'
        window=f.get(field);tick=1 if f['schema']==3 else 1000
        if not isinstance(window,list) or len(window)!=2 or any(type(t) is not int or t<0 for t in window):raise ValueError('invalid frame clock window')
        width=window[1]-window[0]
        if width<0 or width+tick<f['wall_ns'] or width>f['wall_ns']+f['measurement_overhead_ns']+tick:raise ValueError('inconsistent wall/bracket/window')
        if not isinstance(f['test'],str) or not f['test']:raise ValueError('missing test identity')
        groups[(f['test'],f['request_id'],f['native_thread_id'])].append(f)
    counts=defaultdict(int)
    for (test,_,_),rows in groups.items():
        current=[];last_end=None
        for f in rows:
            field='mach_absolute_window_ns' if f['schema']==3 else 'monotonic_window_ns'
            if last_end is not None and f[field][0]<last_end:raise ValueError('overlapping/duplicate measured frames')
            last_end=f[field][1]
            if f['frame']==0:
                if current:raise ValueError('new dump before previous dump completed')
                counts[test]+=1
            if f['frame']!=len(current):raise ValueError('missing/duplicate frame sequence')
            current.append(f)
            if any(r['frames_observed']!=f['frames_observed'] for r in current):raise ValueError('inconsistent dump count')
            if len(current)==f['frames_observed']:current=[]
        if current:raise ValueError('incomplete dump')
    if expected is not None and dict(counts)!=expected:raise ValueError('missing/unexpected case or dump count')
    return dict(counts)


def parse_frames(text,dedicated=False):
    if dedicated:
        rows=[]
        for line in text.splitlines():
            if not line.startswith('TC207_FRAME '):raise ValueError('unexpected dedicated log line')
            row=json.loads(line[len('TC207_FRAME '):])
            if not isinstance(row,dict):raise ValueError('non-object frame record')
            rows.append(row)
        if not rows:raise ValueError('empty dedicated frame log')
        return rows
    rows=[json.loads(m.group(1)) for m in re.finditer(r'TC207_FRAME (\{[^\n]*\})',text)]
    if len(rows)!=text.count('TC207_FRAME '):raise ValueError('malformed frame marker/log')
    return rows


def frame_text(root,receipt):
    if receipt.get('dedicated_frames'):
        path=root/'frame-logs'/('frames-'+str(receipt['pid'])+'.jsonl')
        if not path.is_file() or path.is_symlink():raise ValueError('missing owned dedicated frame log')
        return path.read_text()
    return (root/'test.stderr').read_text() if (root/'test.stderr').exists() else (root/'test.log').read_text()


def integrate(rows,lo,hi):
    if hi<lo:raise ValueError('uncertainty exceeds frame window')
    totals=defaultdict(int);last=None
    for r in sorted(rows,key=lambda r:r['start']):
        if r['end']<r['start'] or last is not None and r['start']<last:raise ValueError('overlapping/retrograde state rows')
        last=r['end'];n=max(0,min(hi,r['end'])-max(lo,r['start']))
        if n:totals[r['state']]+=n
    missing=hi-lo-sum(totals.values())
    if missing:totals['UNKNOWN']=missing
    return dict(totals)


def decode(root):
    receipt=json.loads((root/'receipt.json').read_text());pid=receipt['pid'];nonce=receipt['nonce']
    toc=E.parse(root/'toc.xml');tables=list(toc.find('.//data'))
    tree=E.parse(root/'events.xml');ids={}
    for e in tree.iter():
        if e.get('id'):
            if e.get('id') in ids:raise ValueError('duplicate XML identity')
            ids[e.get('id')]=e
    def res(e):return resolve(e,ids)
    def num(e):return int(res(e).text)
    def text(e):return res(e).text or res(e).get('fmt') or ''
    def process(e):return num(res(e).find('pid'))
    def thread(e):return num(res(e).find('tid'))
    def stack(e):
        if e.tag=='sentinel':return []
        return [res(f).get('name','UNKNOWN') for f in res(e).findall('frame')]
    states=[];switches=[];calls=[];samples=[];supplemental=[];transition_stacks=[];signposts={}
    for node in tree.getroot():
        match=re.search(r'table\[(\d+)\]$',node.get('xpath',''))
        if not match:raise ValueError('unmapped table identity')
        schema=tables[int(match.group(1))-1].get('schema')
        for row in node.findall('row'):
            c=list(row)
            if schema=='os-signpost-arg' and len(c)==10 and process(c[5])==pid and text(c[3])=='TC207Clock':
                key=(num(c[0]),thread(c[6]));g=signposts.setdefault(key,{})
                name=text(c[4])
                if name in g:raise ValueError('duplicate signpost argument')
                g[name]=text(c[9])
            elif schema=='thread-state' and len(c)>=5 and process(c[4])==pid:
                states.append(dict(pid=pid,tid=thread(c[1]),start=num(c[0]),end=num(c[0])+num(c[3]),state=text(c[2])))
            elif schema=='context-switch' and len(c)>=4 and process(c[3])==pid:
                switches.append(dict(pid=pid,tid=thread(c[1]),time=num(c[0]),event=text(c[2])))
            elif schema=='syscall' and len(c)>=14 and process(c[4])==pid:
                calls.append(dict(pid=pid,tid=thread(c[1]),start=num(c[0]),end=num(c[0])+num(c[3]),name=text(c[2]),stack=stack(c[13])))
            elif schema=='time-profile' and len(c)==7 and process(c[2])==pid:
                entry=dict(pid=pid,tid=thread(c[1]),time=num(c[0]),state=text(c[4]),stack=stack(c[6]),source_table=int(match.group(1)))
                (samples if profile_sample_kind(tables[int(match.group(1))-1].attrib,entry['state'])=='cpu' else supplemental).append(entry)
            elif schema=='context-switch-sample' and len(c)==6 and process(c[2])==pid:
                transition_stacks.append(dict(pid=pid,tid=thread(c[1]),time=num(c[0]),stack=stack(c[5])))
    raw=(root/'test.log').read_text()
    frame_log=frame_text(root,receipt)
    anchors=[json.loads(m.group(1)) for m in re.finditer(r'TC207_CLOCK (\{[^\n]*\})',raw)]
    if {a['label'] for a in anchors}!={'process-start','process-exit'} or len(anchors)!=2:raise ValueError('missing/extra process anchors')
    for a in anchors:
        if a['pid']!=pid or a['nonce']!=nonce or not a['signpost_enabled']:raise ValueError('foreign/inactive process anchor')
        matches=[(time,tid) for (time,tid),args in signposts.items() if args.get('arg0')==a['label'] and args.get('arg1')==nonce and args.get('arg2')==str(a['mono_before_ns'])]
        if len(matches)!=1 or matches[0][1]!=a['native_thread_id']:raise ValueError('missing/ambiguous native signpost anchor')
        a['trace_ns']=matches[0][0]
    frames=parse_frames(frame_log,dedicated=receipt.get('dedicated_frames',False))
    validate_frames(frames,pid,expected=expected_dumps(receipt))
    field=None;error=None
    try:field,offset=clock_bridge(frames,anchors)
    except ValueError as exc:offset=None;error=str(exc)
    tick=max((a['timebase_numer']+a['timebase_denom']-1)//a['timebase_denom'] for a in anchors) if field=='mach_absolute_window_ns' else 1000
    normalized=dict(receipt=receipt,anchors=anchors,clock_resolution_ns=tick,monotonic_clock_resolution_ns=1000,offset_interval_ns=offset,
                    calibration_clock=field,calibration_status='UNKNOWN' if error else 'ADMITTED',calibration_error=error,
                    states=states,switches=switches,syscalls=calls,cpu_samples=samples,supplemental_samples=supplemental,transition_stacks=transition_stacks,frames=frames,
                    limits=['constant offset admitted only if both bracket intervals agree; clock/sleep error beyond admission remains a limit',
                            'Interrupted is separate; no inference that every non-Running interval is scheduling wait',
                            'only owned PID state/call/sample records retained; other process narratives/device identifiers excluded'])
    frame_results=[]
    for f in frames:
        if f['pid']!=pid:raise ValueError('foreign measured frame')
        if offset is None:
            frame_results.append(dict(frame=f,reason=error,states_ns=None));continue
        lo,hi=frame_window(f,offset,field)
        if hi<lo:
            frame_results.append(dict(frame=f,reason='bracket/clock uncertainty exceeds short frame',states_ns=None));continue
        tid=f['native_thread_id'];rows=[r for r in states if r['tid']==tid]
        totals=integrate(rows,lo,hi)
        frame_results.append(dict(frame=f,conservative_trace_window_ns=[lo,hi],states_ns=totals,
                                  syscalls=[c for c in calls if c['tid']==tid and c['start']<hi and c['end']>lo],
                                  cpu_samples=[s for s in samples if s['tid']==tid and lo<=s['time']<=hi],
                                  supplemental_samples=[s for s in supplemental if s['tid']==tid and lo<=s['time']<=hi],
                                  transition_stacks=[s for s in transition_stacks if s['tid']==tid and lo<=s['time']<=hi]))
    normalized['frame_results']=frame_results
    return normalized


if __name__=='__main__':
    result=decode(Path(sys.argv[1]))
    print(json.dumps(result,indent=2))
    raise SystemExit(0 if result['calibration_status']=='ADMITTED' else 1)
