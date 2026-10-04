"""Same-job R1/C/R2 completion comparison; proposed policy is not a guarantee.

All receipt/raw admission precedes numeric evaluation. Reference and execution
revisions are separate; actual GitHub locators are never relabelled as reference.
"""
from __future__ import annotations

import copy
import json
import os
import shutil
import time
from pathlib import Path
from types import SimpleNamespace

if __package__:
    from . import indexing_perf as collector
    from . import indexing_perf_contract as contract
else:
    import indexing_perf as collector
    import indexing_perf_contract as contract

REFERENCE = "afdd0c4e6b4c97a270e737ed9e14a2a694db2d7e"
PROTOCOL = "same-job-RCR-v1"
POLICY = {"id": "rcr-completion-v1", "slowdown_ratio": 1.5, "reference_drift_ratio": 1.25}
# Activate only after the pre-reserved null campaign and independent review.
ENFORCE_TIMING = False
ROLES = ("reference-before", "candidate", "reference-after")
ENDPOINTS = ("index_ready_ms", "results_ready_ms")
STATISTICS = ("median", "max")
WORK_FILES = {p for p in collector.REQUIRED_SOURCES if p.startswith("rust/src/")}


class Budget:
    """Shared bounds across all three legs, not three independent 75min caps."""
    def __init__(self, build, measurement):
        for value in (build, measurement):
            contract.require(contract.number(value, "budget") > 0, "positive budget")
        self.left = {"build": build, "measurement": measurement,
                     "discovery": 90., "setup": 240.}

    def run(self, kind, argv, cwd, env, log):
        limit = self.left[kind]
        contract.require(limit > 0, "shared " + kind + " budget exhausted")
        # Discovery and setup also have individual finite bounds.
        bound = min(limit, 30 if kind == "discovery" else 60 if kind == "setup" else limit)
        start = time.monotonic()
        try:
            return collector.run_owned_process(argv, cwd, env, log, bound)
        finally:
            self.left[kind] -= time.monotonic() - start


def _execution_runner(receipt):
    runner = dict(receipt["runner"])
    if runner["kind"] == "same-job-reference":
        runner["kind"] = "intended-ubuntu-24.04"
    return runner


def validate_triplet(sessions, group):
    """All legs were raw/receipt-validated. Reconcile cross-source work/context."""
    contract.require(set(sessions) == set(ROLES), "missing/unknown comparison leg")
    candidate, candidate_result = sessions["candidate"]
    contract.require(candidate["runner"]["kind"] == "intended-ubuntu-24.04", "candidate requires intended hosted context")
    executor = candidate["source_before"]
    contract.require("scripts/indexing_perf_gate.py" in executor["files"], "missing executing comparator")
    seen = set()
    last_finished = None
    for role in ROLES:
        receipt, result = sessions[role]
        contract.require(receipt["group"] == group, "comparison selection")
        contract.require(receipt["run_id"] not in seen, "duplicate comparison run")
        seen.add(receipt["run_id"])
        source = receipt["source_before"]
        contract.require(receipt.get("collector_source") == executor, "cross-source collector provenance")
        if role != "candidate":
            contract.require(source["head"] == REFERENCE, "unknown fixed reference")
            contract.require(receipt["runner"]["kind"] == "same-job-reference",
                             "reference execution/source conflation")
        for key in ("toolchain", "hardware", "cargo_config_before", "build_profile"):
            contract.require(receipt[key] == candidate[key], "different comparison " + key)
        contract.require(_execution_runner(receipt) == _execution_runner(candidate), "different job/image/workflow context")
        for path in WORK_FILES:
            contract.require(source["files"][path] == executor["files"][path], "benchmark work contract changed: " + path)
        # The package's displayed version is not an execution/build setting.
        for field in ("environment_identity", "runtime_settings"):
            ignore = {"crate_version"} if field == "environment_identity" else {"window_trace_path"}
            normalized = lambda m: {k:v for k,v in m[field].items() if k not in ignore}
            contract.require(normalized(result["metadata"]) == normalized(candidate_result["metadata"]),
                             "comparison runtime/environment drift")
        interval = receipt["collection_interval_ns"]
        start = contract.integer(interval["started"], "collection start", 1)
        finish = contract.integer(interval["finished"], "collection finish", 1)
        contract.require(start <= finish and (last_finished is None or last_finished <= start),
                         "reference/candidate collection order")
        last_finished = finish
    contract.require(sessions[ROLES[0]][0]["source_before"] == sessions[ROLES[2]][0]["source_before"],
                     "reference source changed between brackets")
    contract.require(sessions[ROLES[0]][0]["executable_sha256"] == sessions[ROLES[2]][0]["executable_sha256"],
                     "reference executable changed between brackets")


def evaluate_triplet(sessions, group, policy=POLICY):
    contract.require(policy == POLICY, "unknown numeric policy")
    validate_triplet(sessions, group)
    summaries = {role:contract.summarize_rows(result["rows"]) for role, (_,result) in sessions.items()}
    decisions = []
    for i, cell in enumerate(summaries["candidate"]):
        for arm in ("control", "condition"):
            for endpoint in ENDPOINTS:
                for statistic in STATISTICS:
                    field = arm + "_" + statistic
                    values = [summaries[r][i]["phases"][endpoint][field] for r in ROLES]
                    for value in values:
                        contract.require(contract.number(value, "completion statistic") > 0, "zero completion statistic")
                    before, candidate, after = values
                    denominator = max(before, after)
                    drift = denominator / min(before, after)
                    limit = denominator * policy["slowdown_ratio"]
                    status = ("indeterminate" if drift > policy["reference_drift_ratio"] else
                              "timing-fail" if candidate > limit else "pass")
                    decisions.append(dict(case=cell["case"], source=cell["source"], arm=arm,
                        endpoint=endpoint, statistic=statistic,
                        meaning="sustained" if statistic == "median" else "tail",
                        reference_before=before, candidate=candidate, reference_after=after,
                        denominator=denominator, candidate_ratio=candidate/denominator,
                        reference_drift_ratio=drift, limit_ms=limit, status=status))
    expected = len(contract.cells_for(group)) * 8
    contract.require(len(decisions) == expected, "numeric family incomplete")
    statuses = {d["status"] for d in decisions}
    status = "indeterminate" if "indeterminate" in statuses else "timing-fail" if "timing-fail" in statuses else "pass"
    source_files = sessions["candidate"][0]["source_before"]["files"]
    reference_files = sessions[ROLES[0]][0]["source_before"]["files"]
    measured = lambda files: {k:v for k,v in files.items() if k.startswith(("rust/", ".cargo/"))}
    return dict(protocol=PROTOCOL, status=status, group=group, policy=copy.deepcopy(policy),
                decision_count=expected, decisions=decisions, cells=summaries,
                null_rust_sources_equal=measured(source_files) == measured(reference_files),
                limitation="Finite engineering check; no p95/false-positive guarantee. Comparison uses the slower reference bracket; candidate-only host interference can fail the gate.")


def _load_comparison(folder, group, proposal=False):
    """Flat upload artifacts suffice; do not trust saved numeric summaries."""
    folder = Path(folder)
    root = contract.parse_json((folder/"receipt.json").read_text(encoding="utf-8"))
    comparison = root["comparison"]
    contract.require(comparison["protocol"] == PROTOCOL and comparison["reference_revision"] == REFERENCE,
                     "unknown comparison protocol/reference")
    contract.require(type(comparison["enforced"]) is bool and comparison["policy"] == POLICY,
                     "comparison mode/policy")
    contract.require(set(comparison["legs"]) == set(ROLES), "comparison leg inventory")
    sessions = {}
    for role in ROLES:
        entry = comparison["legs"][role]
        raw = "measurement.log" if role == "candidate" else role + "-measurement.log"
        record = role + "-receipt.log"
        contract.require(entry["raw"] == raw and entry["receipt"] == record, "artifact role/path mismatch")
        contract.require(collector.sha(folder/record) == entry["receipt_sha256"], "component receipt digest")
        receipt = contract.parse_json((folder/record).read_text(encoding="utf-8"))
        text = (folder/raw).read_bytes().decode("utf-8")
        sessions[role] = (receipt, collector.validate_receipt(receipt, text, group))
    original = {k:v for k,v in root.items() if k != "comparison"}
    contract.require(original == sessions["candidate"][0], "root/candidate receipt mismatch")
    report = evaluate_triplet(sessions, group, comparison["policy"])
    if not proposal and not comparison["enforced"]:
        report.update(proposal_status=report["status"], status="comparison-calibration", timing_gate=None)
    return report


def load_comparison(folder, group, proposal=False):
    try:
        return _load_comparison(folder, group, proposal)
    except (KeyError, TypeError, ValueError, AttributeError) as error:
        raise contract.ValidationError("malformed comparison: " + str(error)) from error


def _flatten(leg, out, role):
    """Keep original bytes, including failed component receipts, for always-upload."""
    for path in sorted(leg.glob("*.log")):
        name = path.name if role == "candidate" else role + "-" + path.name
        with (out/name).open("xb") as target:
            target.write(path.read_bytes())
    record = leg/"receipt.json"
    if record.exists():
        with (out/(role+"-receipt.log")).open("xb") as target:
            target.write(record.read_bytes())


def collect_triplet(args):
    root = Path(args.root).resolve()
    out = Path(args.output).resolve()
    contract.require(not args.local_observation and not args.allow_local_controls, "hosted comparison requires clean source")
    contract.require(not out.exists(), "output already exists")
    source = collector.source_identity(root, args.revision)
    runner = collector.runner_identity(os.environ, False)
    collector.validate_runner_record(runner, source, collector.hardware_identity(root))
    out.mkdir(parents=True)
    budget = Budget(args.build_timeout, args.measurement_timeout)
    setup = {}
    final = {"schema_version":1, "status":"invalid", "group":args.group}
    try:
        def prepare(name, argv, cwd):
            setup[name] = budget.run("setup", argv, cwd, dict(os.environ), out/(name+".log"))
            contract.require(setup[name]["success"], "reference setup failed: " + name)
        # actions/checkout is shallow. Fetch only the fixed object; never move HEAD.
        prepare("reference-fetch", ["git","fetch","--no-tags","--depth=1","origin",REFERENCE], root)
        reference = out/"reference-source"
        prepare("reference-clone", ["git","clone","--shared","--no-checkout",str(root),str(reference)], out)
        # Git may disable local sharing for a shallow source and clone only HEAD.
        # Explicitly fetch the admitted object from that owned source, not a ref.
        prepare("reference-object-fetch", ["git","fetch","--no-tags","--depth=1","origin",REFERENCE], reference)
        prepare("reference-checkout", ["git","checkout","--detach",REFERENCE], reference)
        legs = {}
        for role in ROLES:
            candidate = role == "candidate"
            leg = out/(role+"-session")
            options = dict(vars(args), root=str(root if candidate else reference),
                revision=args.revision if candidate else REFERENCE, output=str(leg),
                collector_source=source, budget=budget,
                build_target=str(out/("candidate-build" if candidate else "reference-build")))
            try:
                collector.collect_once(SimpleNamespace(**options))
            finally:
                if leg.exists():
                    _flatten(leg, out, role)
            receipt_name = role+"-receipt.log"
            legs[role] = dict(raw="measurement.log" if candidate else role+"-measurement.log",
                              receipt=receipt_name, receipt_sha256=collector.sha(out/receipt_name))
        contract.require(collector.source_identity(root,args.revision) == source, "execution source changed during comparison")
        final = contract.parse_json((out/"candidate-receipt.log").read_text(encoding="utf-8"))
        final["comparison"] = dict(protocol=PROTOCOL, reference_revision=REFERENCE,
                                   enforced=ENFORCE_TIMING, policy=copy.deepcopy(POLICY), legs=legs)
        # Write before evaluating: timing failures keep valid component evidence.
        collector.write_json(out/"receipt.json", final)
        report = load_comparison(out, args.group)
        collector.write_json(out/"summary.json", report)
        if ENFORCE_TIMING:
            contract.require(report["status"] == "pass", "numeric gate " + report["status"])
    except BaseException as error:
        # Never replace a valid measurement receipt with a timing-failure fiction.
        if not (out/"receipt.json").exists():
            final.update(status="invalid", error=type(error).__name__+": "+str(error), setup=setup)
            collector.write_json(out/"receipt.json", final)
        raise
