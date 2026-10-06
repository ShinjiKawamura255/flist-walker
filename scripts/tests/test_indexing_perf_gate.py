"""Synthetic comparison controls. No fixture here claims a perf execution."""
from __future__ import annotations

import copy
from contextlib import ExitStack
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from scripts import indexing_perf as collector
from scripts import indexing_perf_contract as contract
from scripts import indexing_perf_gate as gate
from scripts.tests.test_indexing_perf import control_log, synthetic_receipt, mutate_record

REFERENCE = gate.REFERENCE
POLICY = {"id": "rcr-median-v3", "slowdown_ratio": 1.5, "reference_drift_ratio": 1.25}
ROOT = Path(__file__).resolve().parents[2]


def sample_times(text, change):
    lines = []
    for line in text.splitlines():
        if "INDEX_PERF_SAMPLE " in line:
            row = json.loads(line.split("INDEX_PERF_SAMPLE ", 1)[1])
            change(row)
            line = "INDEX_PERF_SAMPLE " + json.dumps(row)
        lines.append(line)
    return "\n".join(lines) + "\n"


def write_fixture(folder, group="f1", change=None, enforced=True):
    """Invented process/source receipts + projected raw data, explicitly synthetic."""
    text = mutate_record(control_log(group), "INDEX_PERF_META",
                         lambda m: m["environment_identity"].update(os="linux"))
    candidate_source = synthetic_receipt(text, group)["source_before"]
    candidate_source["files"]["scripts/indexing_perf_gate.py"] = "c" * 64
    candidate_source["files"][".github/workflows/perf-regression.yml"] = "c" * 64
    legs = {}
    for index, role in enumerate(("reference-before", "candidate", "reference-after")):
        raw = sample_times(text, change) if role == "candidate" and change else text
        receipt = synthetic_receipt(raw, group, ident="synthetic-" + role)
        receipt["source_before"] = copy.deepcopy(candidate_source)
        receipt["collector_source"] = copy.deepcopy(candidate_source)
        if role != "candidate":
            receipt["source_before"]["head"] = REFERENCE
        receipt["runner"] = dict(kind="intended-ubuntu-24.04" if role == "candidate" else "same-job-reference",
            ImageOS="ubuntu24", ImageVersion="synthetic", GITHUB_RUN_ID="123", GITHUB_RUN_ATTEMPT="1",
            GITHUB_JOB="synthetic", GITHUB_SHA="a"*40, GITHUB_WORKFLOW_SHA="a"*40,
            GITHUB_WORKFLOW_REF="synthetic/repo/.github/workflows/perf-regression.yml@refs/heads/control")
        receipt["hardware"].update(os="linux", affinity=list(range(12)),
            memory_total="MemTotal: 12582912 kB", os_release='NAME="Ubuntu"\nVERSION_ID="24.04"\n')
        receipt["collection_interval_ns"] = dict(started=10*index+1, finished=10*index+2)
        receipt["source_after"] = copy.deepcopy(receipt["source_before"])
        raw_name = "measurement.log" if role == "candidate" else role + "-measurement.log"
        receipt_name = role + "-receipt.log"
        (folder / raw_name).write_text(raw, encoding="utf-8", newline="")
        collector.write_json(folder / receipt_name, receipt)
        legs[role] = {"raw": raw_name, "receipt": receipt_name,
                      "receipt_sha256": collector.sha(folder / receipt_name)}
    root = json.loads((folder / "candidate-receipt.log").read_text())
    root["comparison"] = {"protocol": gate.PROTOCOL, "reference_revision": REFERENCE,
                          "enforced": enforced, "policy": copy.deepcopy(POLICY), "legs": legs,
                          "fixture_provenance": "synthetic-validator-control"}
    collector.write_json(folder / "receipt.json", root)
    return root


class GateTests(unittest.TestCase):
    def test_observer_protocol_pins_actual_healthy_measurement_checkpoint(self):
        self.assertEqual(gate.REFERENCE, "9640bef9884525ea641087d09122541877f40f29")
        self.assertEqual(gate.PROTOCOL, "same-job-RCR-f1-matched-21-observer-v7")

    def test_cli_rejects_numeric_slowdown_despite_valid_candidate_admission(self):
        with tempfile.TemporaryDirectory() as name:
            folder = Path(name)
            def slow(row):
                row["index_ready_ms"] *= 3
                row["results_ready_ms"] *= 3
            write_fixture(folder, change=slow)
            # Source/work/oracles/physical completion all remain validator-valid.
            collector.load_run(folder, "f1")
            run = subprocess.run([sys.executable, "scripts/indexing_perf.py", "validate",
                                  "--group", "f1", str(folder)], cwd=ROOT,
                                 capture_output=True, text=True, timeout=20)
            self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
            self.assertIn("timing-fail", run.stdout + run.stderr)

    def test_repaired_group_cadence_is_fixed_in_child_environment(self):
        for group, pairs in (("f1", 21), ("matched", 21), ("stable", 7)):
            with self.subTest(group=group):
                env = collector.child_environment({}, group, Path("synthetic-fixture"))
                self.assertEqual(env["FW_INDEX_PERF_EXTRA_PAIRS"], str(pairs))

    def test_f1_later_fourteen_pairs_affect_completion_median_and_maximum(self):
        with tempfile.TemporaryDirectory() as name:
            folder = Path(name)
            def slow(row):
                if row["pair"] >= 7:
                    row["index_ready_ms"] *= 3
                    row["results_ready_ms"] *= 3
            write_fixture(folder, change=slow)
            collector.load_run(folder, "f1")
            report = gate.load_comparison(folder, "f1")
            self.assertEqual(report["status"], "timing-fail")
            self.assertTrue(any(d["status"] == "timing-fail" and d["statistic"] == "median"
                                for d in report["decisions"]))
            self.assertTrue(any(d["status"] == "diagnostic" and not d["enforced"] and d["candidate_exceeds_limit"] and d["statistic"] == "max"
                                for d in report["decisions"]))
            for cell in report["cells"]["candidate"]:
                self.assertEqual(len(cell["phases"]["index_ready_ms"]["control"]), 21)

    def test_f1_twenty_first_pair_is_in_maximum_without_becoming_sustained_failure(self):
        with tempfile.TemporaryDirectory() as name:
            def tail(row):
                if row["pair"] == 20:
                    row["index_ready_ms"] *= 100
                    row["results_ready_ms"] *= 100
            write_fixture(Path(name), change=tail)
            collector.load_run(name, "f1")
            report = gate.load_comparison(name, "f1")
            self.assertEqual(report["status"], "pass")
            self.assertTrue(all(d["status"] == "pass" for d in report["decisions"]
                                if d["statistic"] == "median"))
            self.assertTrue(any(d["status"] == "diagnostic" and not d["enforced"] and d["candidate_exceeds_limit"] and d["meaning"] == "tail"
                                for d in report["decisions"]))

    def test_matched_later_fourteen_pairs_affect_completion_medians(self):
        with tempfile.TemporaryDirectory() as name:
            folder = Path(name)
            def slow(row):
                if row["pair"] >= 7:
                    row["index_ready_ms"] *= 3
                    row["results_ready_ms"] *= 3
            write_fixture(folder, "matched", change=slow)
            collector.load_run(folder, "matched")
            report = gate.load_comparison(folder, "matched")
            self.assertEqual(report["status"], "timing-fail")
            self.assertTrue(all(d["status"] == "timing-fail" for d in report["decisions"]
                                if d["enforced"]))
            self.assertEqual((report["enforced_count"], report["diagnostic_count"]), (12, 12))

    def test_matched_twenty_first_pair_retained_in_all_roles_without_median_failure(self):
        with tempfile.TemporaryDirectory() as name:
            folder = Path(name)
            def tail(row):
                if row["pair"] == 20:
                    row["index_ready_ms"] *= 100
                    row["results_ready_ms"] *= 100
            write_fixture(folder, "matched", change=tail)
            collector.load_run(folder, "matched")
            report = gate.load_comparison(folder, "matched")
            self.assertEqual(report["status"], "pass")
            self.assertTrue(any(not d["enforced"] and d["candidate_exceeds_limit"]
                                and d["status"] == "diagnostic" for d in report["decisions"]))
            for role in gate.ROLES:
                for cell in report["cells"][role]:
                    for endpoint in ("index_ready_ms", "results_ready_ms"):
                        for arm in ("control", "condition"):
                            self.assertEqual(len(cell["phases"][endpoint][arm]), 21)

    def test_old_v5_protocol_rejected_even_with_current_matched_cadence(self):
        with tempfile.TemporaryDirectory() as name:
            folder = Path(name)
            root = write_fixture(folder, "matched")
            collector.load_run(folder, "matched")
            root["comparison"]["protocol"] = "same-job-RCR-f1-21-observer-v5"
            (folder / "receipt.json").unlink()
            collector.write_json(folder / "receipt.json", root)
            with self.assertRaises(contract.ValidationError):
                gate.load_comparison(folder, "matched")

    def sessions(self, folder, group):
        root = write_fixture(folder, group)
        sessions = {}
        for role, paths in root["comparison"]["legs"].items():
            receipt = json.loads((folder / paths["receipt"]).read_text())
            raw = (folder / paths["raw"]).read_text()
            sessions[role] = receipt, collector.validate_receipt(receipt, raw, group)
        return sessions

    def test_all_68_medians_fail_and_68_single_sample_maxima_remain_diagnostic(self):
        count = 0
        for group in contract.GROUPS:
            with tempfile.TemporaryDirectory() as name:
                sessions = self.sessions(Path(name), group)
                healthy = gate.evaluate_triplet(sessions, group)
                self.assertEqual(healthy["status"], "pass")
                self.assertTrue(healthy["null_rust_sources_equal"])
                count += healthy["decision_count"]
                self.assertEqual(healthy["enforced_count"], healthy["diagnostic_count"])
                self.assertEqual(healthy["enforced_count"] * 2, healthy["decision_count"])
                self.assertEqual(len({tuple(d[k] for k in ("case", "source", "arm", "endpoint", "statistic"))
                                      for d in healthy["decisions"]}), healthy["decision_count"])
                self.assertTrue(all(d["enforced"] == (d["statistic"] == "median")
                                    for d in healthy["decisions"]))
                for decision in healthy["decisions"]:
                    mutant = copy.deepcopy(sessions)
                    chosen = [r for r in mutant["candidate"][1]["rows"]
                        if r["comparison"] == decision["case"] and r["source"] == decision["source"]
                        and (r["case"] == "B0") == (decision["arm"] == "control")]
                    if decision["statistic"] == "max":
                        chosen = [chosen[0]]
                    for row in chosen:
                        row[decision["endpoint"]] = decision["denominator"] * 3
                    result = gate.evaluate_triplet(mutant, group)
                    found = next(d for d in result["decisions"] if all(d[k] == decision[k]
                        for k in ("case", "source", "arm", "endpoint", "statistic")))
                    self.assertEqual(found["status"], "timing-fail" if found["enforced"] else "diagnostic", found)
                    self.assertTrue(found["candidate_exceeds_limit"])
                    if decision["statistic"] == "max":
                        self.assertEqual(result["status"], "pass")
                        median = next(d for d in result["decisions"] if d["statistic"] == "median"
                            and all(d[k] == decision[k] for k in ("case","source","arm","endpoint")))
                        self.assertEqual(median["status"], "pass")
                        self.assertEqual(found["meaning"], "tail")
        self.assertEqual(count, 136)

    def test_slower_reference_denominator_strict_boundaries_and_symmetric_drift(self):
        with tempfile.TemporaryDirectory() as name:
            sessions = self.sessions(Path(name), "f1")
            # Constant distributions isolate exact arithmetic from original timings.
            for role, (_, result) in sessions.items():
                for row in result["rows"]:
                    row["index_ready_ms"] = row["results_ready_ms"] = 100
            for row in sessions["reference-after"][1]["rows"]:
                row["index_ready_ms"] = row["results_ready_ms"] = 125
            for row in sessions["candidate"][1]["rows"]:
                row["index_ready_ms"] = row["results_ready_ms"] = 187.5
            report = gate.evaluate_triplet(sessions, "f1")
            self.assertEqual(report["status"], "pass")
            self.assertEqual(report["decisions"][0]["denominator"], 125)
            for row in sessions["candidate"][1]["rows"]:
                row["index_ready_ms"] = 187.500001
            self.assertEqual(gate.evaluate_triplet(sessions,"f1")["status"], "timing-fail")
            for role, values in [("reference-before", 99.999), ("reference-after", 125.001)]:
                mutant = copy.deepcopy(sessions)
                for row in mutant[role][1]["rows"]:
                    row["index_ready_ms"] = values
                self.assertEqual(gate.evaluate_triplet(mutant,"f1")["status"], "indeterminate")
            # Reference tails do not change median admission or its denominator.
            for row in sessions["candidate"][1]["rows"]:
                row["index_ready_ms"] = 187.5
            sessions["reference-after"][1]["rows"][0]["index_ready_ms"] = 200
            report = gate.evaluate_triplet(sessions,"f1")
            self.assertEqual(report["status"], "pass")

    def constant_sessions(self, folder):
        sessions = self.sessions(folder, "f1")
        for _, result in sessions.values():
            for row in result["rows"]:
                row["index_ready_ms"] = row["results_ready_ms"] = 100
        return sessions

    def test_maximum_excess_is_visible_without_becoming_median_failure(self):
        with tempfile.TemporaryDirectory() as name:
            sessions = self.constant_sessions(Path(name))
            for role in ("reference-before", "reference-after"):
                sessions[role][1]["rows"][0]["index_ready_ms"] = 140
            sessions["candidate"][1]["rows"][0]["index_ready_ms"] = 151
            report = gate.evaluate_triplet(sessions, "f1")
            self.assertEqual(report["status"], "pass")
            found = [d for d in report["decisions"] if d["status"] == "diagnostic" and d["candidate_exceeds_limit"]]
            self.assertEqual(len(found), 1)
            self.assertEqual(found[0]["statistic"], "max")
            self.assertEqual(found[0]["denominator"], 100)
            self.assertEqual(found[0]["limit_ms"], 150)

    def test_reference_tail_over_typical_ceiling_remains_diagnostic_when_medians_match(self):
        with tempfile.TemporaryDirectory() as name:
            sessions = self.constant_sessions(Path(name))
            for role in ("reference-before", "reference-after"):
                sessions[role][1]["rows"][0]["index_ready_ms"] = 180
            report = gate.evaluate_triplet(sessions, "f1")
            self.assertEqual(report["status"], "pass")
            affected = [d for d in report["decisions"] if d["reference_tail_ratio"] > 1.5]
            self.assertTrue(affected)
            self.assertTrue(all(not d["reference_admission_reasons"] for d in affected))
            self.assertTrue(all(d["status"] == "diagnostic" for d in affected if not d["enforced"]))

    def test_reference_maximum_bracket_drift_is_diagnostic_even_over_typical_ceiling(self):
        with tempfile.TemporaryDirectory() as name:
            sessions = self.constant_sessions(Path(name))
            sessions["reference-before"][1]["rows"][0]["index_ready_ms"] = 1000
            report = gate.evaluate_triplet(sessions, "f1")
            self.assertEqual(report["status"], "pass")
            max_decisions = [d for d in report["decisions"] if d["statistic"] == "max"]
            self.assertTrue(any(d["reference_drift_ratio"] == 10 for d in max_decisions))
            self.assertTrue(all(d["reference_median_drift_ratio"] == 1 for d in report["decisions"]))

    def test_reference_tail_boundary_equality_and_reciprocal_brackets(self):
        with tempfile.TemporaryDirectory() as name:
            base = self.constant_sessions(Path(name))
            for first, second in (("reference-before", "reference-after"),
                                  ("reference-after", "reference-before")):
                sessions = copy.deepcopy(base)
                for row in sessions[second][1]["rows"]:
                    row["index_ready_ms"] = 125
                sessions[first][1]["rows"][0]["index_ready_ms"] = 187.5
                sessions["candidate"][1]["rows"][0]["index_ready_ms"] = 187.5
                report = gate.evaluate_triplet(sessions, "f1")
                self.assertEqual(report["status"], "pass")
                sessions[first][1]["rows"][0]["index_ready_ms"] = 187.500001
                self.assertEqual(gate.evaluate_triplet(sessions, "f1")["status"], "pass")

    def test_uniform_slowdown_fails_all_medians_and_keeps_all_maximum_excess(self):
        with tempfile.TemporaryDirectory() as name:
            sessions = self.constant_sessions(Path(name))
            for row in sessions["candidate"][1]["rows"]:
                row["index_ready_ms"] = row["results_ready_ms"] = 151
            report = gate.evaluate_triplet(sessions, "f1")
            self.assertEqual(report["status"], "timing-fail")
            self.assertTrue(all(d["status"] == ("timing-fail" if d["enforced"] else "diagnostic")
                                and d["candidate_exceeds_limit"] for d in report["decisions"]))
            self.assertTrue(all(d["denominator"] == 100 for d in report["decisions"]))

    def test_reference_indeterminate_retains_candidate_limit_excess_diagnostic(self):
        with tempfile.TemporaryDirectory() as name:
            sessions = self.constant_sessions(Path(name))
            for row in sessions["reference-before"][1]["rows"]:
                row["index_ready_ms"] = 180
            for row in sessions["candidate"][1]["rows"]:
                row["index_ready_ms"] = 300
            report = gate.evaluate_triplet(sessions, "f1")
            self.assertEqual(report["status"], "indeterminate")
            self.assertTrue(any(d["status"] == "indeterminate" and d["candidate_exceeds_limit"]
                                for d in report["decisions"]))

    def test_v4_and_v3_protocols_and_policies_cannot_be_relabelled_as_v6(self):
        with tempfile.TemporaryDirectory() as name:
            folder = Path(name)
            root = write_fixture(folder)
            for old in ("same-job-RCR-f1-21-observer-v3", "same-job-RCR-f1-21-observer-v4"):
                root["comparison"]["protocol"] = old
                (folder / "receipt.json").unlink()
                collector.write_json(folder / "receipt.json", root)
                with self.assertRaises(contract.ValidationError):
                    gate.load_comparison(folder, "f1")
            root["comparison"]["protocol"] = gate.PROTOCOL
            for old in ("rcr-completion-v1", "rcr-median-ceiling-v2"):
                root["comparison"]["policy"]["id"] = old
                (folder / "receipt.json").unlink()
                collector.write_json(folder / "receipt.json", root)
                with self.assertRaises(contract.ValidationError):
                    gate.load_comparison(folder, "f1")

    def test_normal_cli_accepts_isolated_maximum_excess_and_retains_raw_sample(self):
        with tempfile.TemporaryDirectory() as name:
            folder = Path(name)
            def tail(row):
                if row["pair"] == 20:
                    row["index_ready_ms"] *= 100
                    row["results_ready_ms"] *= 100
            write_fixture(folder, change=tail)
            collector.load_run(folder, "f1")
            run = subprocess.run([sys.executable, "scripts/indexing_perf.py", "validate",
                                  "--group", "f1", str(folder)], cwd=ROOT,
                                 capture_output=True, text=True, timeout=20)
            self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
            report = gate.load_comparison(folder, "f1")
            self.assertEqual((report["enforced_count"], report["diagnostic_count"]), (32, 32))
            self.assertTrue(any(d["status"] == "diagnostic" and d["candidate_exceeds_limit"]
                                for d in report["decisions"]))
            self.assertTrue(all(len(cell["phases"]["index_ready_ms"]["control"]) == 21
                                for cell in report["cells"]["candidate"]))

    def test_shared_control_and_condition_slowdown_cannot_cancel_in_a_ratio(self):
        with tempfile.TemporaryDirectory() as name:
            folder = Path(name)
            write_fixture(folder, change=lambda r:r.update(index_ready_ms=r["index_ready_ms"]*3,
                                                           results_ready_ms=r["results_ready_ms"]*3))
            report = gate.load_comparison(folder, "f1")
            self.assertEqual(report["status"], "timing-fail")
            self.assertEqual({d["arm"] for d in report["decisions"] if d["status"] == "timing-fail"},
                             {"control", "condition"})

    def test_rejects_missing_duplicate_or_cross_source_job_and_work_context(self):
        with tempfile.TemporaryDirectory() as name:
            base = self.sessions(Path(name), "f1")
            changes = [lambda s:s.pop("reference-after"),
                lambda s:s["reference-before"][0].update(run_id=s["candidate"][0]["run_id"]),
                lambda s:s["reference-before"][0]["source_before"].update(head="0"*40),
                lambda s:s["reference-before"][0]["runner"].update(GITHUB_RUN_ID="124"),
                lambda s:s["reference-before"][0]["runner"].update(GITHUB_SHA=REFERENCE),
                lambda s:s["reference-before"][0]["hardware"].update(cpu_model="other"),
                lambda s:s["reference-before"][0]["source_before"]["files"].update({next(iter(gate.WORK_FILES)):"0"*64}),
                lambda s:s["reference-before"][0]["collection_interval_ns"].update(finished=100),
                lambda s:s["reference-after"][0].update(collector_source=None)]
            for change in changes:
                mutant = copy.deepcopy(base);change(mutant)
                with self.assertRaises(contract.ValidationError):gate.evaluate_triplet(mutant,"f1")

    def test_flat_replay_recomputes_and_rejects_tampering_instead_of_trusting_summary(self):
        with tempfile.TemporaryDirectory() as name:
            folder = Path(name);write_fixture(folder)
            (folder/"summary.json").write_text('{"status":"timing-fail"}')
            self.assertEqual(gate.load_comparison(folder,"f1")["status"],"pass")
            (folder/"reference-before-measurement.log").write_bytes(b"bad")
            with self.assertRaises(contract.ValidationError):gate.load_comparison(folder,"f1")
        with tempfile.TemporaryDirectory() as name:
            folder = Path(name);write_fixture(folder)
            (folder/"reference-after-receipt.log").unlink()
            with self.assertRaises(OSError):gate.load_comparison(folder,"f1")

    def test_calibration_is_explicitly_not_an_enforced_numeric_pass(self):
        with tempfile.TemporaryDirectory() as name:
            folder = Path(name)
            write_fixture(folder, enforced=False, change=lambda r:r.update(
                index_ready_ms=r["index_ready_ms"]*3, results_ready_ms=r["results_ready_ms"]*3))
            report = gate.load_comparison(folder,"f1")
            self.assertEqual(report["status"], "comparison-calibration")
            self.assertEqual(report["proposal_status"], "timing-fail")
            self.assertIsNone(report["timing_gate"])


class OrchestrationTests(unittest.TestCase):
    def simulate(self, folder, slow=False, fail_last=False):
        blueprint = folder/"controls";blueprint.mkdir()
        write_fixture(blueprint, change=(lambda r:r.update(index_ready_ms=r["index_ready_ms"]*3,
            results_ready_ms=r["results_ready_ms"]*3)) if slow else None)
        root_receipt = json.loads((blueprint/"receipt.json").read_text())
        source = root_receipt["source_before"]
        options = SimpleNamespace(root=str(folder/"source"), output=str(folder/"out"), group="f1",
            revision=source["head"], allow_local_controls=False, local_observation=False,
            build_timeout=1800, measurement_timeout=2700)
        calls = []
        def setup(_budget, kind, argv, cwd, env, log):
            self.assertEqual(kind, "setup")
            log.write_text("synthetic setup, no Git operation\n")
            return dict(success=True, leader_reaped=True, group_absent=True, returncode=0,
                timed_out=False, orphan_detected=False, elapsed_seconds=1)
        def collect(opt):
            role = gate.ROLES[len(calls)];calls.append(opt)
            leg = Path(opt.output);leg.mkdir()
            record = json.loads((blueprint/(role+"-receipt.log")).read_text())
            raw = "measurement.log" if role == "candidate" else role+"-measurement.log"
            (leg/"measurement.log").write_bytes((blueprint/raw).read_bytes())
            if fail_last and role == "reference-after":
                record["status"] = "invalid"
            collector.write_json(leg/"receipt.json",record)
            if fail_last and role == "reference-after":
                raise contract.ValidationError("synthetic unclean reference")
        mocks = [mock.patch.object(collector,"source_identity",return_value=source),
                 mock.patch.object(collector,"runner_identity",return_value=root_receipt["runner"]),
                 mock.patch.object(collector,"hardware_identity",return_value=root_receipt["hardware"]),
                 mock.patch.object(gate.Budget,"run",setup),
                 mock.patch.object(collector,"collect_once",side_effect=collect)]
        return options, calls, mocks

    def test_numeric_failure_keeps_valid_legs_and_flat_upload_replay(self):
        from contextlib import ExitStack
        with tempfile.TemporaryDirectory() as name:
            folder = Path(name);options,calls,mocks = self.simulate(folder,slow=True)
            with ExitStack() as stack:
                for patch in mocks:stack.enter_context(patch)
                stack.enter_context(mock.patch.object(gate,"ENFORCE_TIMING",True))
                with self.assertRaisesRegex(contract.ValidationError,"timing-fail"):
                    gate.collect_triplet(options)
            out = Path(options.output)
            self.assertEqual(json.loads((out/"receipt.json").read_text())["status"],"observed-valid")
            self.assertEqual(gate.load_comparison(out,"f1")["status"],"timing-fail")
            self.assertEqual(len(calls),3)
            self.assertEqual(calls[0].build_target,calls[2].build_target)
            self.assertNotEqual(calls[0].build_target,calls[1].build_target)
            self.assertIs(calls[0].budget,calls[2].budget)
            self.assertEqual([c.revision for c in calls],[REFERENCE,"a"*40,REFERENCE])
            # Absolute candidate paths must not be longer only for reference legs.
            self.assertEqual(len({len(str(Path(c.output)/"fixtures").encode()) for c in calls}),1)
            # Copy exactly the existing workflow upload selection; nested dirs excluded.
            uploaded = folder/"uploaded";uploaded.mkdir()
            for file in [out/"receipt.json",out/"summary.json",*out.glob("*.log")]:
                (uploaded/file.name).write_bytes(file.read_bytes())
            self.assertEqual(gate.load_comparison(uploaded,"f1")["status"],"timing-fail")

    def test_unclean_last_reference_is_invalid_and_retains_prior_flat_logs(self):
        from contextlib import ExitStack
        with tempfile.TemporaryDirectory() as name:
            folder = Path(name);options,calls,mocks = self.simulate(folder,fail_last=True)
            with ExitStack() as stack:
                for patch in mocks:stack.enter_context(patch)
                with self.assertRaisesRegex(contract.ValidationError,"unclean reference"):
                    gate.collect_triplet(options)
            out = Path(options.output)
            self.assertEqual(json.loads((out/"receipt.json").read_text())["status"],"invalid")
            self.assertTrue((out/"measurement.log").exists())
            self.assertTrue((out/"reference-before-measurement.log").exists())
            self.assertEqual(json.loads((out/"reference-after-receipt.log").read_text())["status"],"invalid")
            self.assertFalse((out/"summary.json").exists())

    def test_budget_is_shared_and_exhaustion_cannot_spawn_another_child(self):
        budget = gate.Budget(10,20)
        with mock.patch.object(gate.time,"monotonic",side_effect=[100,105,105,112]), \
             mock.patch.object(collector,"run_owned_process",return_value={}) as process:
            budget.run("build",[],Path("."),{},Path("not-created"))
            budget.run("build",[],Path("."),{},Path("not-created"))
            with self.assertRaisesRegex(contract.ValidationError,"exhausted"):
                budget.run("build",[],Path("."),{},Path("not-created"))
        self.assertEqual([call.args[-1] for call in process.call_args_list],[10,5])

    def test_shallow_fetch_and_owned_shared_reference_checkout_leave_source_head_unchanged(self):
        # Real Git operations in disposable repos; no network or shared workspace mutation.
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name).resolve();remote=directory/"remote";remote.mkdir()
            def git(cwd,*args):
                result = subprocess.run(["git",*args],cwd=cwd,capture_output=True,text=True,timeout=20)
                self.assertEqual(result.returncode,0,result.stderr)
                return result.stdout.strip()
            git(remote,"init","-q")
            (remote/"file").write_text("baseline")
            git(remote,"add","file")
            git(remote,"-c","user.name=Synthetic Test","-c","user.email=test@example.invalid","commit","-qm","baseline")
            reference=git(remote,"rev-parse","HEAD")
            (remote/"file").write_text("candidate")
            git(remote,"-c","user.name=Synthetic Test","-c","user.email=test@example.invalid","commit","-qam","candidate")
            source=directory/"source"
            git(directory,"clone","--depth=1",remote.as_uri(),str(source))
            head=git(source,"rev-parse","HEAD")
            git(source,"fetch","--no-tags","--depth=1","origin",reference)
            clone=directory/"reference"
            git(directory,"clone","--shared","--no-checkout",str(source),str(clone))
            git(clone,"fetch","--no-tags","--depth=1","origin",reference)
            git(clone,"checkout","--detach",reference)
            self.assertEqual(git(source,"rev-parse","HEAD"),head)
            self.assertEqual(git(source,"status","--porcelain"),"")
            self.assertEqual(git(clone,"rev-parse","HEAD"),reference)
            self.assertEqual((clone/"file").read_text(),"baseline")


class HostedActivationTests(unittest.TestCase):
    def test_default_hosted_collection_rejects_numeric_only_completion_regression(self):
        with tempfile.TemporaryDirectory() as name:
            folder = Path(name)
            options, calls, patches = OrchestrationTests().simulate(folder, slow=True)
            with ExitStack() as stack:
                for patch in patches:
                    stack.enter_context(patch)
                # No enforcement-mode mock: exercise the published hosted default.
                with self.assertRaisesRegex(contract.ValidationError, "timing-fail"):
                    gate.collect_triplet(options)
            receipt = json.loads((Path(options.output)/"receipt.json").read_text())
            self.assertEqual(len(calls), 3)
            self.assertEqual(receipt["status"], "observed-valid")
            self.assertTrue(receipt["comparison"]["enforced"])
            self.assertEqual(gate.load_comparison(options.output, "f1")["status"], "timing-fail")

    def test_default_hosted_collection_accepts_normal_completion_and_records_enforcement(self):
        with tempfile.TemporaryDirectory() as name:
            options, calls, patches = OrchestrationTests().simulate(Path(name))
            with ExitStack() as stack:
                for patch in patches:
                    stack.enter_context(patch)
                gate.collect_triplet(options)
            receipt = json.loads((Path(options.output)/"receipt.json").read_text())
            self.assertEqual(len(calls), 3)
            self.assertTrue(receipt["comparison"]["enforced"])
            self.assertEqual(gate.load_comparison(options.output, "f1")["status"], "pass")


    def test_default_hosted_collection_keeps_isolated_maximum_excess_diagnostic(self):
        import subprocess
        import sys
        with tempfile.TemporaryDirectory() as name:
            folder=Path(name)
            options,calls,patches=OrchestrationTests().simulate(folder)
            blueprint=folder/'controls'
            for path in blueprint.iterdir():
                path.unlink()
            def tail(row):
                if row['pair']==20:
                    row['index_ready_ms']*=100
                    row['results_ready_ms']*=100
            write_fixture(blueprint,change=tail)
            with ExitStack() as stack:
                for patch in patches:
                    stack.enter_context(patch)
                # Published default is intentionally not mocked.
                gate.collect_triplet(options)
            root=json.loads((Path(options.output)/'receipt.json').read_text())
            self.assertTrue(root['comparison']['enforced'])
            self.assertEqual(root['status'],'observed-valid')
            self.assertEqual(len(calls),3)
            report=gate.load_comparison(options.output,'f1')
            self.assertEqual(report['status'],'pass')
            self.assertEqual((report['enforced_count'],report['diagnostic_count']),(32,32))
            self.assertTrue(any(d['status']=='diagnostic' and d['candidate_exceeds_limit'] for d in report['decisions']))
            run=subprocess.run([sys.executable,'scripts/indexing_perf.py','validate','--group','f1',options.output],cwd=ROOT,capture_output=True,text=True,timeout=20)
            self.assertEqual(run.returncode,0,run.stdout+run.stderr)
            self.assertEqual(json.loads(run.stdout),report)


if __name__ == "__main__":
    unittest.main()
