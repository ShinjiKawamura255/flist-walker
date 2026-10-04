"""Recompute point-in-time tables from retained, already accepted R3 records.

This is an evidence calculation, not a new timing gate or raw-log collector.
Run only after all four measurement processes and main readbacks finish.
"""
import csv
import gzip
import hashlib
import json
import math
import statistics
import sys
from pathlib import Path

if not __debug__:
    raise SystemExit("Evidence calculation refuses optimized Python")

COMMIT = {
    "v27": "37195d639338f10549efabb1abad729546230914",
    "v28": "ff7df8330f1ea7ec4a11c7f470dbbb8efeff74f8",
    "v29": "ec4d9b8b12e0b92a5a441f45016f55621faac93e",
    "v30": "129ec84cbf24ffb02bba1cade5b75fa042fd0c45",
}
METRICS = ("data_publish_end_ms", "terminal_publish_ms", "index_ready_ms",
           "results_ready_ms", "full_wait_ms", "max_frame_ms",
           "max_ingest_gap_ms", "max_no_work_progress_ms", "worker_drained_ms")
sha = lambda b: hashlib.sha256(b).hexdigest()
directory, output = map(Path, sys.argv[1:])
summaries, inputs, statuses = [], [], {}
for short, commit in COMMIT.items():
    manifest_path = directory / ("historical-" + short + "-r5-live-evidence.json")
    manifest_bytes = manifest_path.read_bytes()
    manifest = json.loads(manifest_bytes)
    verified = {}
    for asset in manifest["assets"]:
        stored = (directory / asset["path"]).read_bytes()
        assert sha(stored) == asset["sha256"] and len(stored) == asset["bytes"]
        plain = gzip.decompress(stored) if asset["encoding"].startswith("gzip") else stored
        assert sha(plain) == asset.get("uncompressed_sha256", asset["sha256"])
        verified[Path(asset["original_path"]).name] = plain
    data = json.loads(verified[short + "-ordinary-full-validated.json"])
    assert data["source_identity"]["original_commit"] == commit
    assert data["protocol_validation"] == "PASS" and data["status_counts"]["NOT_RUN"] == 0
    assert len(data["statuses"]) == 44 and sum(data["status_counts"].values()) == 44
    assert data["raw_exit_code"] == (0 if data["status_counts"]["FAIL"] == 0 else 101)
    assert len(data["accepted_raw_rows"]) == data["accepted_rows"]
    assert all(s["positive_cleanup"] is True and s["root_restored"] is True for s in data["statuses"])
    statuses[short] = [{k: s.get(k) for k in (
        "case", "source", "status", "panic", "actual_partial_rows", "accepted_rows",
        "positive_cleanup", "root_restored")} for s in data["statuses"]]
    inputs.append({"manifest": manifest_path.name, "sha256": sha(manifest_bytes),
                   "accepted_record_sha256": sha(verified[short + "-ordinary-full-validated.json"]),
                   "source_identity": data["source_identity"], "whole_exit": data["raw_exit_code"],
                   "whole_trailer": data["raw_libtest_trailer"], "status_counts": data["status_counts"],
                   "accepted_rows": data["accepted_rows"], "excluded_partial_rows": data["excluded_partial_rows"]})
    groups = {}
    for row in data["accepted_raw_rows"]:
        assert row["contention_eligible"] and row["correct"]
        key = (row["comparison"], row["source"], "A" if row["case"] == "B0" else "B")
        groups.setdefault(key, []).append(row)
    for (case, source, half), rows in sorted(groups.items()):
        assert len(rows) == 7 and {r["pair"] for r in rows} == set(range(7))
        contract = lambda r: {k: r.get(k) for k in (
            "fixture_shape", "fixture_signature", "sample_entries", "measurement_kind",
            "comparison_kind", "condition_description", "settings", "initial_state",
            "expected_final_logical_entries", "expected_query_match_count")}
        assert all(contract(r) == contract(rows[0]) for r in rows)
        request_lists = [r.get("index_requests") for r in rows]
        if all(v is None for v in request_lists):
            request_count_range = None  # Worker-only rows have no GUI request ledger.
        else:
            assert all(isinstance(v, list) for v in request_lists)
            request_count_range = [min(map(len, request_lists)), max(map(len, request_lists))]
        item = {"version": short, "case": case, "source": source, "half": half,
                "declared_contract": contract(rows[0]), "metrics": {},
                "actual_entries_emitted_range": [min(r["entries_emitted"] for r in rows), max(r["entries_emitted"] for r in rows)],
                "actual_request_count_range": request_count_range}
        for metric in METRICS:
            values = [r.get(metric) for r in rows]
            if all(v is None for v in values):
                continue
            assert all(isinstance(v, (float, int)) and math.isfinite(v) and v >= 0 for v in values)
            item["metrics"][metric] = {"median": statistics.median(values), "maximum": max(values), "raw7": values}
        summaries.append(item)
result = {"scope": "Four actual tags; accepted PASS condition halves only; no new gate or whole-test PASS claim",
          "limits": "Same declared contract does not imply identical retired partial work, native operation implementation, overshoot or CPU work. AA is variability; seven pairs do not establish p95. Parser consumer terminal differs from GUI t2/t3.",
          "inputs": inputs, "statuses": statuses, "condition_summaries": summaries}
json_path = output.with_suffix(".json")
csv_path = output.with_suffix(".csv")
assert not json_path.exists() and not csv_path.exists()
json_path.write_text(json.dumps(result, indent=2) + "\n")
with csv_path.open("w", newline="") as f:
    writer = csv.writer(f)
    writer.writerow(["version", "case", "source", "half", "fixture_inputs", "final_logical_entries", "actual_emitted_min", "actual_emitted_max", "metric", "median_ms", "maximum_ms"])
    for item in summaries:
        c = item["declared_contract"]
        for metric, values in item["metrics"].items():
            writer.writerow([item["version"], item["case"], item["source"], item["half"], c["sample_entries"], c["expected_final_logical_entries"], *item["actual_entries_emitted_range"], metric, values["median"], values["maximum"]])
print("Retained-input calculation complete:", len(summaries), "condition halves;", json_path, csv_path)
