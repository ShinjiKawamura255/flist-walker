"""Strict extended measurement contracts; no timing ceiling is inferred here.

Victim predicates are an explicit port of the frozen fbd9 collector. Historical
files remain immutable. Python validation checks reported witnesses; full member,
kind/order and omitted GUI debt oracles remain source-bound Rust assertions.
"""
from __future__ import annotations

import collections
import json
import math
import re
import statistics


class ValidationError(ValueError):
    """A run cannot supply accepted calibration evidence."""


def require(condition, message):
    if not condition:
        raise ValidationError(str(message))


CANCELED_ROLE = "declared-Warm-eviction-retained-last-good"
STALE_FULL_ROLE = "declared-Warm-eviction-stale-Full-retained-last-good"
RETAINED_ROLES = (CANCELED_ROLE, STALE_FULL_ROLE)

def validate_victim_contract(r):
    validate_numeric_fields(r)
    require(r['preemption_observer_overflow'] is False and r['preemption_observer_limit'] == 128, 'victim contract line 15')
    require(r['warm_removal_observer_overflow'] is False and r['warm_removal_observer_limit'] == 128, 'victim contract line 16')
    events = r['actual_preemption_events']
    edges = r['declared_eviction_edges']
    victims = r['declared_retained_victims']
    removals = r['actual_warm_removal_events']
    reqs = {q['request_id']: q for q in r['index_requests']}
    require(len(reqs) == len(r['index_requests']) and set(reqs) == set(r['allocated_request_ids']), 'victim contract line 19')
    require(len(events) <= 128 and len({e['victim_request_id'] for e in events}) == len(events), 'victim contract line 20')
    require(len(removals) <= 128 and len({m['removed_request_id'] for m in removals}) == len(removals), 'victim contract line 21')
    require(len({e['victim_request_id'] for e in edges}) == len(edges), 'victim contract line 22')
    require(len({v['request_id'] for v in victims}) == len(victims), 'victim contract line 23')
    retained = {v['request_id']: v for v in victims}
    declared = {e['victim_request_id']: e for e in edges}
    removed = {m['removed_request_id']: m for m in removals}
    if events or edges or victims or removals:
        require(r['comparison'] == 'T1-A-B-C-A' and r['case'] != 'B0', 'victim contract line 25')
    require(r['retained_seed_validation_policy'] == 'full independent oracle after tentative t3; no extra frame/input before output', 'victim contract line 26')

    def actors(edge):
        tabs = edge['trace_tabs']
        require(len(set(tabs)) == 3 and edge['stage'] in (1, 2), 'victim contract line 28')
        result = (tabs[0], tabs[2], tabs[1]) if edge['stage'] == 1 else (tabs[1], tabs[0], tabs[2])
        require((edge['victim_tab'], edge['expected_active_tab'], edge['expected_current_warm_tab']) == result, 'victim contract line 30')
        return result

    def permission(q, edge, mutation):
        require(edge['declared_ms'] <= mutation, 'victim contract line 33')
        if edge['permission_ms'] is None:
            require(q['permission_ms'] is None and q['permission_reason'] is None and (edge['permission_reason'] is None), 'victim contract line 35')
            require(q['completed_predecessor'] is True and q['terminal_kind'] == q['actual_terminal_offer_kind'] == 'finished', 'victim contract line 36')
            require(q['terminal_publish_ms'] <= q['completed_predecessor_observed_ms'] <= mutation, 'victim contract line 37')
            require(q['started_source'] == q['terminal_source'] == r['source'] and q['actual_started_root'] == q['expected_root'], 'victim contract line 38')
        else:
            require(q['permission_reason'] == edge['permission_reason'] == 'switch-evicts-previous-Warm', 'victim contract line 40')
            require(edge['declared_ms'] <= q['permission_ms'] == edge['permission_ms'] <= mutation, 'victim contract line 41')
        if edge['closed_ms'] is not None:
            require(mutation <= edge['closed_ms'], 'victim contract line 42')
        require(q['bookkeeping_released_ms'] <= r['index_ready_ms'] and q['request_processing_returned_ms'] <= r['index_ready_ms'], 'victim contract line 43')
        require(q['admitted_root_matches'] is True and q['actual_admitted_root'] == q['expected_root'], 'victim contract line 44')
        if q['actual_started_root'] is not None:
            require(q['actual_started_root'] == q['expected_root'] and q['started_source'] == r['source'], 'victim contract line 45')
    for m in removals:
        q = reqs[m['removed_request_id']]
        edge = declared[m['removed_request_id']]
        victim, active, warm = actors(edge)
        require(m['removed_request_id'] > 0 and m['previous_warm_tab'] == m['route_tab'] == q['tab_id'] == victim == edge['old_warm_tab'], 'victim contract line 48')
        require(m['replacement_warm_tab'] == warm, 'victim contract line 49')
        ack = edge['actual_switch_ack']
        require(ack['pending_activation_tab_id'] is None and ack['tab_id'] == active, 'victim contract line 50')
        require(ack['root'] == edge['expected_active_root'] and m['mutation_ms'] <= ack['at_ms'], 'victim contract line 51')
        require(edge['incoming_request_ids'] and all((reqs[i]['tab_id'] == active and reqs[i]['expected_root'] == ack['root'] for i in edge['incoming_request_ids'])), 'victim contract line 52')
        if edge['closed_ms'] is not None:
            require(ack['at_ms'] <= edge['closed_ms'], 'victim contract line 53')
        permission(q, edge, m['mutation_ms'])
    for e in events:
        edge = declared[e['victim_request_id']]
        q = reqs[e['victim_request_id']]
        victim, active, warm = actors(edge)
        require(e['victim_tab'] == q['tab_id'] == victim == edge['old_warm_tab'] and e['replacement_request_id'] == 0, 'victim contract line 57')
        require((e['actual_active_tab'], e['actual_warm_tab']) == (active, warm), 'victim contract line 58')
        incoming = e['pending_active_request_id']
        require(incoming == e['latest_active_request_id'], 'victim contract line 59')
        require(incoming in edge['incoming_request_ids'] and reqs[incoming]['tab_id'] == active, 'victim contract line 60')
        require(incoming in e['queued_active_request_ids'] and len(e['queued_active_request_ids']) <= 128, 'victim contract line 61')
        require(len(set(e['queued_active_request_ids'])) == len(e['queued_active_request_ids']), 'victim contract line 62')
        require(set(e['queued_active_request_ids']) <= set(edge['incoming_request_ids']) and e['actual_inflight_count'] >= 2, 'victim contract line 63')
        if e['prior_latest_request_id'] is None:
            m = removed[e['victim_request_id']]
            require(e['warm_removal_mutation_ms'] == m['mutation_ms'] <= e['mutation_ms'], 'victim contract line 65')
        else:
            require(e['warm_removal_mutation_ms'] is None and e['prior_latest_request_id'] == e['victim_request_id'] and (e['victim_request_id'] not in removed), 'victim contract line 67')
            permission(q, edge, e['mutation_ms'])
        if edge['closed_ms'] is not None:
            require(e['mutation_ms'] <= edge['closed_ms'], 'victim contract line 69')
    latest_by_tab = {tab: max((q['request_id'] for q in reqs.values() if q['tab_id'] == tab)) for tab in {q['tab_id'] for q in reqs.values()}}
    for q in reqs.values():
        ident = q['request_id']
        m = removed.get(ident)
        direct = next((e for e in events if e['victim_request_id'] == ident and e['prior_latest_request_id'] == ident), None)
        expected_cause = 'warm-replacement-removal' if m else 'direct-preempt' if direct else None
        require(q['actual_cause_kind'] == expected_cause, 'victim contract line 74')
        earliest = m['mutation_ms'] if m else direct['mutation_ms'] if direct else None
        require(q['invalidating_mutation_ms'] == earliest, 'victim contract line 76')
        marker = next((e for e in events if e['victim_request_id'] == ident and e['prior_latest_request_id'] is None), None)
        require(q['followup_preempt_ms'] == (marker['mutation_ms'] if marker else None), 'victim contract line 78')
        require(q['latest_measured_request'] == (ident == latest_by_tab[q['tab_id']]), 'victim contract line 79')
        role = q['terminal_role']
        require(role in ('required-latest-success', 'revoked-predecessor', 'completed-predecessor', *RETAINED_ROLES), 'victim contract line 80')
        require(q.get('stale_full_data_abort_duplicate', False) is False, 'victim contract line 81')
        require(q['latest_generation_required'] == (q['latest_measured_request'] and role not in RETAINED_ROLES), 'victim contract line 82')
        if q['latest_measured_request']:
            require(role in ('required-latest-success', *RETAINED_ROLES), 'victim contract line 83')
        elif role == 'revoked-predecessor':
            successor = reqs[q['planned_revoked_by']]
            require(successor['tab_id'] == q['tab_id'] and successor['request_id'] > ident, 'victim contract line 85')
        else:
            require(role == 'completed-predecessor' and q['completed_predecessor'] is True, 'victim contract line 86')
        if q['latest_generation_required']:
            require(role == 'required-latest-success' and q['terminal_kind'] == 'finished' and (q['terminal_source'] == r['source']), 'victim contract line 87')
        if role in RETAINED_ROLES:
            v = retained[ident]
            require(v['tab_id'] == q['tab_id'] and q['latest_measured_request'] is True and (earliest is not None), 'victim contract line 89')
            require(q['admitted_ms'] <= q['started_ms'] <= earliest <= q['terminal_offered_ms'] <= q['terminal_publish_ms'] <= q['request_processing_returned_ms'], 'victim contract line 90')
            require(q['skipped_closed_before_start'] is False and q['entries_emitted'] > 0, 'victim contract line 91')
            require(q['terminal_offer_current'] is False, 'victim contract line 92')
            if 'role' in v:
                require(v['role'] == role, 'victim contract line 93')
            if role == STALE_FULL_ROLE:
                a = q['stale_full_data_abort']
                require(a is not None and q['stale_full_data_abort_limit'] == 1 and (q['stale_full_data_abort_duplicate'] is False), 'victim contract line 95')
                require(declared[ident]['stage'] == 2 and declared[ident]['trace_tabs'][1] == q['tab_id'], 'victim contract line 96')
                require(q['terminal_kind'] == q['actual_terminal_offer_kind'] == 'failed' and q['terminal_offer_error'] == 'index receiver closed', 'victim contract line 97')
                require(a['reason'] == 'stale-full-data' and a['request_id'] == a['response_request_id'] == ident and (a['tab_id'] == q['tab_id']), 'victim contract line 98')
                require(a['data_kind'] in ('batch', 'replace-all') and a['latest_lookup_succeeded'] is True and (a['shutdown'] is False) and (a['latest_request_id'] != ident), 'victim contract line 99')
                require(earliest <= a['at_ms'] <= q['terminal_offered_ms'] and q['data_publish_end_ms'] is None, 'victim contract line 100')
                require(v['role'] == STALE_FULL_ROLE and 'partial failed' in q['emitted_workload_semantics'], 'victim contract line 101')
            else:
                require(q['terminal_kind'] == q['actual_terminal_offer_kind'] == 'canceled', 'victim contract line 103')
                require('partial canceled' in q['emitted_workload_semantics'], 'victim contract line 104')
            require(q['started_source'] == r['source'] and q['actual_started_root'] == q['expected_root'], 'victim contract line 105')
            require(q['latest_generation_required'] is False and q['planned_revoked_by'] is None, 'victim contract line 106')
            require(q['completed_predecessor'] is False and q['permission_reason'] == 'switch-evicts-previous-Warm' and (q['permission_ms'] is not None), 'victim contract line 107')
            require(v['full_seed_oracle_after_t3'] is True and v['seed_request_id'] != ident and (v['seed_request_id'] == v['actual_seed_request_id']), 'victim contract line 108')
            require(v['seed_root'] == v['actual_seed_root'] == q['expected_root'] and v['seed_source'] == v['actual_seed_source'] == r['source'], 'victim contract line 109')
            require(v['seed_all_count'] == v['actual_all_count'] == r['sample_entries'] and v['seed_visible_count'] == v['actual_visible_count'] == r['sample_entries'], 'victim contract line 110')
            require(v['seed_all_signature'] == v['actual_all_signature'] and v['seed_visible_signature'] == v['actual_visible_signature'], 'victim contract line 111')
            require(v['partial_emitted_entries'] == q['entries_emitted'] > 0 and 'not 100k indexing throughput' in v['workload_semantics'], 'victim contract line 112')
        else:
            require(ident not in retained, 'victim contract line 113')
    require(len(retained) == sum((q['terminal_role'] in RETAINED_ROLES for q in reqs.values())), 'victim contract line 114')

GROUPS = {
    "f1": ("F1-files", "F1-folders", "F1-ignore-list", "F1-ignore-case", "F1-mid-ignore"),
    "matched": ("S1-ignore", "S2-files"),
    "stable": ("T1-S1-selective", "T1-S1-dense", "T1-S2"),
}
PAIR_COUNTS = {"f1": 21, "matched": 7, "stable": 7}
FULL_POLICY = "observed-completion (data-end/Finished-offer/publication) waits for own committed snapshot before eviction; t0-included production frames; final unobserved publication/removal race remains strict failure"
STABLE_EDIT_INPUT_POLICY = "full-scale stable-A previous owned query evaluated in full before next input; normal production frames/indexing continue; all three inputs must overlap unsettled indexing"
PHASES = ("data_publish_end_ms", "terminal_publish_ms", "index_ready_ms", "results_ready_ms", "full_wait_ms", "max_frame_ms", "max_ingest_gap_ms", "max_no_work_progress_ms", "driver_overhead_ms")


def cells_for(group):
    require(group in GROUPS, "unsupported profile group")
    return [(case, source) for case in GROUPS[group]
            for source in (("Walker",) if case in ("F1-files", "F1-folders", "S2-files") else ("FileList", "Walker"))]


def parse_json(text):
    def object_pairs(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, "duplicate JSON field: " + key)
            result[key] = value
        return result
    def reject(value):
        raise ValidationError("nonfinite JSON number: " + value)
    try:
        return json.loads(text, object_pairs_hook=object_pairs, parse_constant=reject)
    except json.JSONDecodeError as error:
        raise ValidationError("invalid JSON record") from error


def number(value, label):
    require(type(value) in (int, float) and math.isfinite(value) and value >= 0,
            "invalid finite nonnegative number: " + label)
    return value


def integer(value, label, minimum=0):
    require(type(value) is int and value >= minimum, "invalid integer: " + label)
    return value


def validate_numeric_fields(value, label="sample"):
    boolean_fields = {"correct", "contention_eligible", "full_scale_cell", "native", "allocation_observed",
        "admitted_root_matches", "completed_predecessor", "latest_measured_request", "latest_generation_required",
        "planned_revocation", "mailbox_closed", "terminal_offer_current", "seeded_old_snapshot", "actual_success",
        "candidate_is_initial_A", "root_is_stable_A", "skipped_canceled", "preemption_observer_overflow",
        "warm_removal_observer_overflow", "stale_full_data_abort_duplicate"}
    boolean_fields.update({"files", "folders", "ignore_enabled", "ignore_case", "follow_links",
                           "skipped_closed_before_start", "actual_index_producer_interval_overlap"})
    integer_fields = {"request_id", "tab_id", "pair", "position", "stage", "candidates", "candidate_count",
        "evaluated_candidates", "stable_A_entries", "active_snapshot_entries", "all_entries", "visible_entries",
        "generation", "planned_revoked_by", "active_tab", "requested_tab", "overlap_executions",
        "full_candidate_evaluations", "GUI_ingested", "entries_emitted", "sample_entries", "epoch"}
    integer_fields.update({"frames", "batches", "blocked_batches", "full_count", "intended_extra_index_requests",
                           "preemption_observer_limit", "warm_removal_observer_limit", "aux_observer_limit_per_flow",
                           "search_sort_worker_completed_while_index_unsettled", "stale_full_data_abort_limit",
                           "previous_stage", "next_stage", "latest_request_id", "seed_request_id", "actual_seed_request_id", "victim_request_id",
                           "removed_request_id", "replacement_request_id", "response_request_id"})
    if isinstance(value, dict):
        for key, item in value.items():
            if key in boolean_fields and item is not None:
                require(type(item) is bool, "invalid boolean: " + label + "." + key)
            if key in integer_fields and item is not None:
                integer(item, label + "." + key)
            if key.endswith("_ms") and item is not None:
                for scalar in item if isinstance(item, list) else [item]:
                    number(scalar, label + "." + key)
            validate_numeric_fields(item, label + "." + key)
    elif isinstance(value, list):
        for item in value:
            validate_numeric_fields(item, label)


def validate_sample(row, case, source, condition):
    require(row["schema_version"] == 1 and type(row["schema_version"]) is int, "sample schema")
    require(row["profile_family"] == "extension" and row["measurement_kind"] == "headless-GUI-actual-workers", "sample kind")
    require(row["native"] is False and row["full_scale_cell"] is True, "not full headless profile")
    require(row["correct"] is True and row["contention_eligible"] is True, "incorrect/noneligible sample")
    require(row["sample_entries"] == 100000 and type(row["sample_entries"]) is int, "sample scale")
    require(row["tabchain_input_policy"] == FULL_POLICY, "trace policy drift")
    stable = case.startswith("T1-")
    require(row["comparison_kind"] == ("AA-variability" if case in ("F1-files", "F1-folders") else "AB-operation-cost"), "comparison kind")
    require(row["fixture_shape"] == ("FlatFiles" if stable else "FlatMixed"), "fixture shape")
    for field in ("fixture_signature", "snapshot_signature", "results_signature"):
        require(re.fullmatch(r"[0-9a-f]{16}", row[field]) is not None, "signature: " + field)
    validate_numeric_fields(row)
    for phase in PHASES:
        number(row[phase], phase)
    require(row["data_publish_end_ms"] <= row["terminal_publish_ms"] <= row["index_ready_ms"] <= row["results_ready_ms"], "phase order")
    number(row["last_confirmed_snapshot_unsettled_ms"], "indexing cutoff")
    require(row["last_confirmed_snapshot_unsettled_ms"] <= row["index_ready_ms"], "cutoff after t2")
    for load in (row["index_sender_load_at_t2"], row["final_index_sender_load"]):
        require(integer(load["queued"], "queued") == integer(load["inflight"], "inflight") == 0, "nonzero sender load")
        require(integer(load["capacity"], "index capacity") == 2, "worker limit drift")
    filter_settings = {
        "files": case != "F1-folders", "folders": case not in ("F1-files", "S2-files"),
        "ignore_enabled": case in ("F1-ignore-case", "S1-ignore") or condition and case in ("F1-ignore-list", "F1-mid-ignore"),
        "ignore_case": case != "F1-ignore-case" or condition, "follow_links": False,
        "query": "needle" if condition and case in ("S1-ignore", "T1-S1-selective") else "item" if condition and case == "T1-S1-dense" else "",
        "sort_mode": "Score", "sort_scope": "ShownResults",
    }
    require(row["settings"] == filter_settings, "operation settings drift")
    logical = 20000 if case == "F1-folders" else 80000 if case in ("F1-files", "S2-files") else 100000
    visible = logical
    if filter_settings["ignore_enabled"]:
        visible = 88234 if filter_settings["ignore_case"] else 94117
    matches = 882 if condition and case == "S1-ignore" else 1000 if condition and case == "T1-S1-selective" else visible
    require(integer(row["expected_final_logical_entries"], "logical") == logical, "logical work mismatch")
    require(integer(row["entries_emitted"], "emitted") == logical, "primary emitted work mismatch")
    require(integer(row["expected_query_match_count"], "matches") == matches, "query work mismatch")
    require(integer(row["initial_state"]["active_snapshot_entries"], "active initial entries") == 0 and row["initial_state"]["source"] == source and row["initial_state"]["depth"] == "unlimited", "initial state")
    require(row["initial_state"]["seeded_old_snapshot"] is stable and integer(row["initial_state"]["stable_A_entries"], "stable initial entries") == (100000 if stable else 0), "seeded prestate")
    requests = row["index_requests"]
    require(len(requests) == (2 if condition and case == "F1-mid-ignore" else 1), "request work count")
    ids = [integer(q["request_id"], "request id", 1) for q in requests]
    require(len(ids) == len(set(ids)), "duplicate request")
    for field in ("allocated_request_ids", "planned_request_ids", "released_request_ids"):
        values = [integer(i, field, 1) for i in row[field]]
        require(len(values) == len(set(values)) and set(values) == set(ids), "request ownership mismatch")
    require(not any(row[f] for f in ("actual_preemption_events", "actual_warm_removal_events", "declared_eviction_edges", "declared_retained_victims")), "unexpected victim in priority profiles")
    validate_victim_contract(row)
    for q in requests:
        integer(q["tab_id"], "tab id", 1)
        require(q["allocation_observed"] is True and q["admitted_root_matches"] is True, "missing actual allocation/admission")
        require(q["actual_admitted_root"] == q["expected_root"] and q["actual_started_root"] == q["expected_root"] and q["started_source"] == source, "request root/source mismatch")
        require(number(q["allocated_ms"], "allocated") <= number(q["admitted_ms"], "admitted") <= number(q["started_ms"], "started") <= number(q["request_processing_returned_ms"], "physical body return") <= row["index_ready_ms"], "request body order")
        require(number(q["bookkeeping_released_ms"], "release") <= row["index_ready_ms"] and q["mailbox_closed"] is True, "unreleased request")
        if q["latest_generation_required"]:
            require(q["terminal_kind"] == q["actual_terminal_offer_kind"] == "finished" and q["terminal_source"] == source, "latest must finish")
            require(integer(q["entries_emitted"], "completed work") == logical, "latest completed work")
            require(q["data_publish_end_ms"] <= q["terminal_offered_ms"] <= q["terminal_publish_ms"] <= q["request_processing_returned_ms"], "latest terminal/body order")
        elif q["terminal_role"] == "revoked-predecessor":
            require(case == "F1-mid-ignore" and condition and q["permission_reason"] == "IgnoreList-PreserveSort-refresh" and q["planned_revocation"] is True, "unauthorized revoked predecessor")
            require(number(q["permission_ms"], "refresh permission") <= number(q["mailbox_closed_ms"], "old close") <= row["index_ready_ms"], "revocation chronology")
    primary = next(q for q in requests if q["request_id"] == row["request_id"])
    require(primary["latest_generation_required"] is True, "primary is not required latest")
    for field in ("data_publish_end_ms", "terminal_publish_ms", "bookkeeping_released_ms", "entries_emitted"):
        require(primary[field] == row[field], "primary observation mismatch")
    inputs = row["input_trace"]
    expected_inputs = 0 if not condition else 3 if case in ("S2-files", "T1-S2") else 1 if case in ("F1-ignore-case", "F1-mid-ignore", "S1-ignore", "T1-S1-selective", "T1-S1-dense") else 0
    require(len(inputs) == expected_inputs, "input cardinality")
    for stage, event in enumerate(inputs):
        require(integer(event["stage"], "input stage") == stage and event["input"] == "production-handler", "input sequence")
        minimum = logical * ((10,25,50)[stage] if case in ("S2-files", "T1-S2") else 10) // 100
        require(minimum <= integer(event["GUI_ingested"], "ingested") < logical, "input checkpoint")
        require(number(event["at_ms"], "input time") <= row["last_confirmed_snapshot_unsettled_ms"], "input outside indexing")
        query = ("item", "needle", "")[stage] if case in ("S2-files", "T1-S2") else filter_settings["query"]
        require(event["query"] == query, "input query trace")
        require(integer(event["requested_tab"], "requested tab", 1) == event["active_tab"], "input active ownership")
        if stable:
            require(event["requested_tab"] != primary["tab_id"], "stable A input ownership")
        else:
            require(event["requested_tab"] == primary["tab_id"] and event["requested_root"] == primary["expected_root"], "input root ownership")
        if stage:
            require(inputs[stage-1]["at_ms"] < event["at_ms"] and inputs[stage-1]["GUI_ingested"] <= event["GUI_ingested"], "input chronology")
    if stable:
        setup = row["initial_state"]["B_committed_empty_setup"]
        require(setup["actual_success"] is True and setup["all_entries"] == setup["visible_entries"] == 0, "empty B acquisition")
        require(setup["root"] == primary["expected_root"] and setup["request_id"] == setup["generation"] != primary["request_id"], "B setup identity")
        require(setup["actual_source"] == ("Walker" if source == "Walker" else 'FileList("'+setup["root"]+'/FileList.txt")'), "B setup source")
    if condition and case in ("S1-ignore", "S2-files", "T1-S1-selective", "T1-S1-dense", "T1-S2"):
        bindings = {integer(b["request_id"], "search request", 1): b for b in row["search_dispatch_bindings"]}
        require(len(bindings) == len(row["search_dispatch_bindings"]), "duplicate search binding")
        workers = row["worker_observations"]
        require(len({w["request_id"] for w in workers}) == len(workers), "duplicate search observation")
        genuine, full = [], []
        for worker in workers:
            binding = bindings[worker["request_id"]]
            require(worker["candidates"] == binding["candidate_count"], "search candidates/binding")
            require(number(binding["dispatched_ms"], "search dispatch") <= number(worker["started_ms"], "search start"), "search start before dispatch")
            if worker["skipped_canceled"] is False and integer(worker["evaluated_candidates"], "evaluated") > 0 and worker["started_ms"] < row["last_confirmed_snapshot_unsettled_ms"] and number(worker["evaluation_completed_ms"], "evaluation completion") >= worker["started_ms"]:
                require(worker["evaluated_candidates"] <= worker["candidates"], "evaluated work")
                require(binding["root_is_stable_A"] is True and binding["sort_mode"] == "Score" and binding["sort_scope"] == "ShownResults", "search root/settings")
                integer(binding["epoch"], "search epoch", 1)
                if stable:
                    require(binding["candidate_is_initial_A"] is True and binding["candidate_count"] == 100000 and re.fullmatch(r"[0-9a-f]{16}", binding["candidate_signature"] or "") is not None, "stable candidate identity")
                    require(binding["tab_id"] != primary["tab_id"] and binding["tab_id"] == inputs[0]["requested_tab"], "stable A search identity")
                    if worker["evaluated_candidates"] == 100000:
                        full.append(binding)
                else:
                    require(binding["tab_id"] == primary["tab_id"], "active search identity")
                require(any(e["requested_tab"] == binding["tab_id"] and e["query"] == binding["query"] and e["at_ms"] <= binding["dispatched_ms"] for e in inputs), "search input binding")
                genuine.append(binding)
        needed = 2 if case in ("S2-files", "T1-S2") else 1
        require(len(genuine) >= needed and integer(row["overlap_executions"], "overlap") == len(genuine), "missing genuine search overlap")
        required_queries = {"item", "needle"} if needed == 2 else {"item" if case == "T1-S1-dense" else "needle"}
        require(required_queries <= {b["query"] for b in (full if stable else genuine)}, "missing substantive query")
        if stable:
            require(integer(row["full_candidate_evaluations"], "full evaluations") == len(full) and len(full) >= needed, "full evaluation count")

    if case == "T1-S2":
        require(row.get("stable_edit_input_policy") == STABLE_EDIT_INPUT_POLICY, "stable edit input policy drift")
        admissions = row.get("stable_edit_input_admissions")
        require(isinstance(admissions, list) and len(admissions) == (2 if condition else 0), "stable edit admission count")
        if condition:
            observations = {w["request_id"]: w for w in workers}
            for stage, admission in enumerate(admissions, 1):
                previous, following = inputs[stage-1], inputs[stage]
                require(integer(admission["previous_stage"], "previous stage") == stage-1 and integer(admission["next_stage"], "next stage") == stage, "stable edit admission sequence")
                request = integer(admission["request_id"], "admission request", 1)
                require(request in bindings and request in observations, "stable edit admission ownership")
                binding, worker = bindings[request], observations[request]
                require(admission["previous_query"] == previous["query"] == binding["query"], "stable edit preceding query")
                require(admission["tab_id"] == previous["requested_tab"] == following["requested_tab"] == binding["tab_id"] and admission["root"] == previous["requested_root"] == following["requested_root"], "stable edit input owner/root")
                require(binding["root_is_stable_A"] is True and binding["candidate_is_initial_A"] is True and binding["sort_mode"] == "Score" and binding["sort_scope"] == "ShownResults", "stable edit binding identity")
                require(admission["epoch"] == binding["epoch"] and admission["candidate_count"] == binding["candidate_count"] == worker["candidates"] == 100000 and admission["evaluated_candidates"] == worker["evaluated_candidates"] == 100000, "stable edit full candidate evaluation")
                require(admission["previous_input_ms"] == previous["at_ms"] and admission["input_admitted_ms"] == following["at_ms"], "stable edit input timestamp binding")
                for field in ("dispatched_ms",):
                    require(admission[field] == binding[field], "stable edit dispatch timestamp binding")
                for field in ("started_ms", "evaluation_completed_ms"):
                    require(admission[field] == worker[field], "stable edit worker timestamp binding")
                require(number(previous["at_ms"], "previous input") <= number(admission["dispatched_ms"], "dispatch") <= number(admission["started_ms"], "start") <= number(admission["evaluation_completed_ms"], "full evaluation") <= number(following["at_ms"], "next input"), "stable edit full evaluation must precede next input")
                require(worker["skipped_canceled"] is False and (worker["canceled_ms"] is None or worker["canceled_ms"] >= following["at_ms"]), "stable edit canceled before admission")
                latest = max((b for b in bindings.values() if b["tab_id"] == binding["tab_id"] and previous["at_ms"] <= b["dispatched_ms"] <= following["at_ms"]), key=lambda b:b["dispatched_ms"])
                require(latest["request_id"] == request, "stable edit superseded preceding request")


def validate_log(text, group):
    """Validate raw witnesses; actual process/source admission additionally needs receipt."""
    try:
        cells = cells_for(group)
        metas, events = [], []
        for line in text.splitlines():
            require("INDEX_PERF_UNSUPPORTED " not in line, "unsupported requested cell")
            for marker, tag in (("INDEX_PERF_META ", "meta"), ("INDEX_PERF_RUN_START ", "start"), ("INDEX_PERF_SAMPLE ", "row")):
                if marker in line:
                    record = parse_json(line.split(marker, 1)[1])
                    if tag == "meta":
                        metas.append(record)
                    else:
                        events.append((tag, record))
        summaries = re.findall(r"^test result:.*$", text, re.M)
        require(len(summaries) == 1 and re.match(r"^test result: ok\. 1 passed; 0 failed; 0 ignored;", summaries[0]), "runner did not pass one test")
        require(len(metas) == 1, "metadata cardinality")
        meta = metas[0]
        require(type(meta["schema_version"]) is int and meta["schema_version"] == 1 and meta["runner"] == "extended" and meta["native"] is False, "metadata schema/kind")
        require(type(meta["entries"]) is int and meta["entries"] == 100000 and type(meta["pairs"]) is int and meta["pairs"] == PAIR_COUNTS[group], "intended scale/pairs")
        require(meta["selected_cases"] == list(GROUPS[group]) and meta["selected_sources"] == ["FileList", "Walker"], "requested selection")
        require(meta["supported_cells"] == [{"case":c,"source":s} for c,s in cells] and not meta["unsupported_cells"] and meta["selected_source_cells"] == len(cells) and meta["expected_rows"] == len(cells)*2*PAIR_COUNTS[group], "metadata cell count")
        require(meta["tabchain_input_policy"] == FULL_POLICY and meta["coverage_kind"] == "selected-subset", "metadata trace identity")
        if group == "stable":
            require(meta["stable_edit_input_policy"] == STABLE_EDIT_INPUT_POLICY, "stable edit metadata policy drift")
        environment = meta["environment_identity"]
        require(environment["optimized"] is True and environment["frame_period_ms"] == 16 and environment["os"] in ("linux", "macos"), "optimized POSIX frame profile")
        require('channel = "1.97.1"' in environment["pinned_rust_toolchain"], "toolchain pin")
        cpus = integer(environment["logical_cpus"], "logical CPUs", 1)
        settings = meta["runtime_settings"]
        expected_defaults = {"search_parallel_threshold":25000,"search_threads":min(cpus,32),"walker_max_entries":500000,"filelist_auto_check_enabled":True,"window_trace_enabled":False,"window_trace_verbose":False,"history_persist_disabled":False,"restore_tabs_enabled":False,"emacs_keybindings_enabled":True,"ctrl_w_deletes_word_in_query":False,"tab_pin_moves_to_next_row":False,"update_feed_url":"https://api.github.com/repos/ShinjiKawamura255/flist-walker/releases/latest","update_allow_same_version":False,"update_allow_downgrade":False,"disable_self_update":False,"force_update_check_failure":""}
        require(set(settings) == set(expected_defaults) | {"window_trace_path"}, "runtime settings schema")
        for key, value in expected_defaults.items():
            require(type(settings[key]) is type(value) and settings[key] == value, "runtime default drift: " + key)
        require(isinstance(settings["window_trace_path"],str) and settings["window_trace_path"].endswith(".flistwalker_window_trace.log"), "window trace default")
        cursor, rows = 0, []
        for case, source in cells:
            def start(condition, role, pair=None, position=None):
                nonlocal cursor
                expected = dict(profile=case, source=source, condition=condition, role=role, entries=100000)
                if pair is not None:
                    expected.update(pair=pair, position=position)
                require(cursor < len(events) and events[cursor] == ("start",expected), "missing/duplicate/out-of-order run start")
                cursor += 1
            for condition in (False, True):
                start(condition, "untimed-warmup")
            for pair in range(PAIR_COUNTS[group]):
                for position, condition in enumerate((False, True) if pair%2 == 0 else (True, False)):
                    start(condition, "sample", pair, position)
                    require(cursor < len(events) and events[cursor][0] == "row", "missing raw row")
                    row = events[cursor][1]; cursor += 1
                    require(integer(row["pair"], "pair") == pair and integer(row["position"], "position") == position and row["order"] == ("AB" if pair%2 == 0 else "BA"), "ABBA order")
                    require(row["comparison"] == case and row["source"] == source and row["case"] == (case if condition else "B0"), "raw case identity")
                    validate_sample(row, case, source, condition)
                    rows.append(row)
        require(cursor == len(events), "extra/missing raw records")
        return {"metadata":meta,"rows":rows}
    except (KeyError, IndexError, TypeError, StopIteration) as error:
        raise ValidationError("missing or malformed measurement field: " + str(error)) from error


def summarize_rows(rows):
    groups = collections.defaultdict(list)
    for row in rows:
        groups[(row["comparison"],row["source"])].append(row)
    output=[]
    for (case,source), samples in sorted(groups.items()):
        selected = [group for group in GROUPS if (case,source) in cells_for(group)]
        require(len(selected) == 1, "unknown summary cell")
        pairs = PAIR_COUNTS[selected[0]]
        require(len(samples) == pairs*2 and
                {(r["pair"],r["case"]) for r in samples} ==
                {(p,c) for p in range(pairs) for c in ("B0",case)}, "summary pair inventory")
        phases={}
        for phase in PHASES:
            control=[next(r[phase] for r in samples if r["pair"]==p and r["case"]=="B0") for p in range(pairs)]
            condition=[next(r[phase] for r in samples if r["pair"]==p and r["case"]==case) for p in range(pairs)]
            ratios=[b/a if a else None for a,b in zip(control,condition)]
            phases[phase]={"control":control,"condition":condition,"paired_ratios":ratios,
                "control_median":statistics.median(control),"condition_median":statistics.median(condition),
                "control_max":max(control),"condition_max":max(condition),
                "control_range":max(control)-min(control),"condition_range":max(condition)-min(condition)}
        output.append({"case":case,"source":source,"comparison_kind":samples[0]["comparison_kind"],"phases":phases})
    return output
