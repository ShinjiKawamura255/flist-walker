"""Replay fixtures are synthetic validator controls, never current perf evidence."""
from __future__ import annotations

import copy
import gzip
import json
import unittest
import os
import sys
import tempfile
import ast
import subprocess
from unittest import mock
from types import SimpleNamespace
import re
from pathlib import Path

from scripts import indexing_perf_contract as contract
from scripts import indexing_perf as collector

ROOT = Path(__file__).resolve().parents[2]


@unittest.skipUnless(os.name == "posix", "collection requires POSIX; receipt validation is portable")
class ProcessTests(unittest.TestCase):
    def test_simulated_collection_keeps_compiler_cache_out_of_runtime_fixtures(self):
        text=control_log();blueprint=synthetic_receipt(text)
        with tempfile.TemporaryDirectory() as name:
            root=Path(name).resolve();out=root/"run"
            args=SimpleNamespace(root=str(root),output=str(out),revision="a"*40,group="f1",
                allow_local_controls=False,local_observation=True,build_timeout=2,measurement_timeout=2)
            binary=out/"build/lib"
            def process(argv,cwd,env,log,timeout):
                if argv[0]=="cargo":
                    self.assertEqual(Path(env["TMPDIR"]),out/"compiler-temp")
                    (Path(env["TMPDIR"])/"xcrun_db").write_text("synthetic compiler cache")
                    binary.parent.mkdir();binary.write_text("synthetic executable; not run")
                    log.write_text(json.dumps(dict(reason="compiler-artifact",executable=str(binary),features=[],
                        target=dict(kind=["lib"]),profile=dict(test=True)))+"\n")
                else:
                    self.assertEqual(Path(env["TMPDIR"]),out/"fixtures")
                    log.write_text(collector.TEST+": test\n" if "--list" in argv else text,encoding="utf-8",newline="")
                r=copy.deepcopy(blueprint["processes"]["build"]);r.update(argv=argv,cwd=str(cwd));return r
            with mock.patch.object(collector,"source_identity",return_value=blueprint["source_before"]), \
                 mock.patch.object(collector,"runner_identity",return_value=blueprint["runner"]), \
                 mock.patch.object(collector,"hardware_identity",return_value=blueprint["hardware"]), \
                 mock.patch.object(collector,"capture",side_effect=lambda argv,*a:blueprint["toolchain"][argv[0]]), \
                 mock.patch.object(collector,"run_owned_process",side_effect=process):
                collector.collect(args)
            receipt,result=collector.load_run(out,"f1")
            self.assertEqual(receipt["compiler_temp_after"],["xcrun_db"])
            self.assertEqual(receipt["fixtures_after"],[])
            self.assertEqual(len(result["rows"]),112)
            receipt["fixtures_after"]=["xcrun_db"]
            with self.assertRaises(contract.ValidationError):collector.validate_receipt(receipt,text,"f1")

    def test_success_nonzero_timeout_and_surviving_descendant(self):
        programs = [
            ("print('owned child')", 5, True),
            ("raise SystemExit(7)", 5, False),
            ("import time;time.sleep(10)", .05, False),
            ("import subprocess,sys;subprocess.Popen([sys.executable,'-c','import time;time.sleep(10)'])", 5, False),
        ]
        with tempfile.TemporaryDirectory() as name:
            for index, (program, bound, success) in enumerate(programs):
                with self.subTest(index=index):
                    receipt = collector.run_owned_process([sys.executable, "-c", program],
                        Path(name), dict(os.environ), Path(name)/f"{index}.log", bound, .5)
                    self.assertIs(receipt["success"], success)
                    self.assertIs(receipt["leader_reaped"], True)
                    self.assertEqual(receipt["orphan_detected"], index == 3)
                    self.assertEqual(receipt["timed_out"], index == 2)

    def test_child_configuration_is_explicit_without_runtime_override_values(self):
        inherited = {"PATH":"path", "HOME":"same", "FLISTWALKER_SEARCH_THREADS":"secret",
                     "FW_INDEX_PERF_EXTRA_ENTRIES":"1", "RUSTFLAGS":"-C opt-level=0",
                     "CARGO_BUILD_TARGET":"bad", "RUSTUP_TOOLCHAIN":"bad"}
        env = collector.child_environment(inherited, "stable", Path("/owned/fixture"))
        self.assertEqual(env["HOME"], "same")
        self.assertEqual(env["FW_INDEX_PERF_EXTRA_ENTRIES"], "100000")
        self.assertEqual(env["FW_INDEX_PERF_EXTRA_PAIRS"], "7")
        self.assertEqual(env["FW_INDEX_PERF_EXTRA_CASES"], ",".join(contract.GROUPS["stable"]))
        self.assertEqual(env["TMPDIR"], "/owned/fixture")
        for key in ["FLISTWALKER_SEARCH_THREADS", "RUSTFLAGS", "CARGO_BUILD_TARGET", "RUSTUP_TOOLCHAIN"]:
            self.assertNotIn(key, env)

    def test_failed_collection_preserves_receipt_log_and_owned_remnants_without_summary(self):
        with tempfile.TemporaryDirectory() as name:
            root=Path(name);out=root/"run"
            args=SimpleNamespace(root=str(root),output=str(out),revision="a"*40,
                group="f1",allow_local_controls=False,local_observation=True,build_timeout=2)
            real_run=collector.run_owned_process
            def failed_build(argv,cwd,env,log,timeout):
                program="from pathlib import Path;import os;Path(os.environ['TMPDIR'],'retained-root').mkdir();raise SystemExit(7)"
                return real_run([sys.executable,"-c",program],root,env,log,timeout)
            with mock.patch.object(collector,"source_identity",return_value={"head":"a"*40}), \
                 mock.patch.object(collector,"runner_identity",return_value={"kind":"local-observation"}), \
                 mock.patch.object(collector,"hardware_identity",return_value={}), \
                 mock.patch.object(collector,"capture",return_value="version"), \
                 mock.patch.object(collector,"run_owned_process",side_effect=failed_build):
                with self.assertRaises(contract.ValidationError):collector.collect(args)
            receipt=json.loads((out/"receipt.json").read_text(encoding="utf-8"))
            self.assertEqual(receipt["status"],"invalid")
            self.assertEqual(receipt["processes"]["build"]["returncode"],7)
            self.assertTrue((out/"build.log").exists())
            self.assertTrue((out/"compiler-temp/retained-root").exists())
            self.assertEqual(list((out/"fixtures").iterdir()),[])
            self.assertEqual(receipt["compiler_temp_after"],["retained-root"])
            self.assertFalse((out/"summary.json").exists())

    def test_disposable_source_identity_rejects_untracked_or_modified_source(self):
        with tempfile.TemporaryDirectory() as name:
            root=Path(name)
            def git(*args):
                return subprocess.run(["git",*args],cwd=root,check=True,capture_output=True,text=True).stdout.strip()
            git("init","-q")
            for path in ["rust/rust-toolchain.toml","scripts/indexing_perf.py","scripts/indexing_perf_contract.py","AGENTS.md"]:
                file=root/path;file.parent.mkdir(exist_ok=True);file.write_text("tracked")
            git("add",".");git("-c","user.name=Collector Test","-c","user.email=collector@example.invalid","commit","-qm","source")
            head=git("rev-parse","HEAD")
            before=collector.source_identity(root,head)
            (root/"AGENTS.md").write_text("temporary control")
            with self.assertRaises(contract.ValidationError):collector.source_identity(root,head)
            allowed=collector.source_identity(root,head,True)
            self.assertEqual(allowed["local_controls"],["AGENTS.md"])
            self.assertEqual(allowed["files"],before["files"])
            (root/"scripts/indexing_perf.py").write_text("changed source")
            with self.assertRaises(contract.ValidationError):collector.source_identity(root,head,True)
            with self.assertRaises(contract.ValidationError):collector.source_identity(root,"0"*40,True)


def synthetic_receipt(text, group="f1", ident="synthetic-1"):
    """Only validator test data; never evidence of executing a perf process."""
    import hashlib
    required_sources=["rust/Cargo.toml","rust/Cargo.lock","rust/rust-toolchain.toml",
        "rust/src/app/tests/indexing_perf/mod.rs","rust/src/app/tests/indexing_perf/harness.rs",
        "rust/src/app/tests/indexing_perf/extensions/runner.rs","rust/src/app/tests/indexing_perf/extensions/driver.rs",
        "rust/src/app/tests/indexing_perf/extensions/fixture.rs","rust/src/app/tests/indexing_perf/extensions/oracle.rs",
        "rust/src/app/tests/indexing_perf/extensions/cases.rs","scripts/indexing_perf.py","scripts/indexing_perf_contract.py"]
    source=dict(head="a"*40,tree="b"*40,files={p:"c"*64 for p in required_sources},local_controls=[])
    process=dict(success=True,leader_reaped=True,group_absent=True,returncode=0,
                 timed_out=False,orphan_detected=False,pid=123,process_group=123,cwd="/synthetic/rust",
                 elapsed_seconds=1,timeout_seconds=10,cleanup_timeout_seconds=5)
    stages={s:dict(process) for s in ["build","discovery","measurement"]}
    stages["build"]["argv"]=["cargo","test","--release","--locked","--lib","--no-run","--message-format=json","--target-dir","/synthetic/build"]
    stages["discovery"]["argv"]=["/synthetic/build/lib",collector.TEST,"--exact","--list","--ignored"]
    stages["measurement"]["argv"]=["/synthetic/build/lib",collector.TEST,"--exact","--ignored","--nocapture","--test-threads=1"]
    return dict(schema_version=1,status="observed-valid",group=group,run_id=ident,
        raw_sha256=hashlib.sha256(text.encode()).hexdigest(),source_before=source,
        source_after=copy.deepcopy(source),processes=stages,executable="/synthetic/build/lib",artifact_features=[],
        rust_root="/synthetic/rust",build_target="/synthetic/build",cargo_config_before={},cargo_config_after={},
        compiler_temp_before=[],compiler_temp_after=[],
        fixtures_before=[],fixtures_after=[],build_profile=dict(release=True,locked=True,
        default_features=True,additional_features=[],incremental=False),executable_sha256="f"*64,
        test_leaf=collector.TEST,toolchain=dict(rustc="rustc 1.97.1 synthetic",cargo="cargo 1.97.1 synthetic"),
        runner=dict(kind="local-observation",image_os=None,image_version=None),hardware=dict(os="macos",arch="arm64",
        kernel="synthetic",logical_cpus=12,affinity=None,cpu_model="synthetic CPU",memory_total="12884901888",
        os_release="synthetic",storage=dict(device=1,block_size=4096,total_bytes=100000000)))


class ReceiptTests(unittest.TestCase):
    def test_test_leaf_matches_actual_rust_module_graph(self):
        expected="app::tests::indexing_perf::harness::extensions::runner::perf_indexing_extended_paired"
        self.assertEqual(collector.TEST,expected)
        declarations=[("rust/src/app/tests/mod.rs","indexing_perf"),
            ("rust/src/app/tests/indexing_perf/mod.rs","harness"),
            ("rust/src/app/tests/indexing_perf/harness.rs","extensions"),
            ("rust/src/app/tests/indexing_perf/extensions/mod.rs","runner")]
        for path,module in declarations:
            self.assertRegex((ROOT/path).read_text(encoding="utf-8"),r"\bmod "+module+r"\s*;")
        self.assertRegex((ROOT/"rust/src/app/tests/indexing_perf/extensions/runner.rs").read_text(encoding="utf-8"),
                         r"\bfn perf_indexing_extended_paired\s*\(")

    def test_source_and_hardware_schema_reject_missing_or_malformed_provenance(self):
        text=control_log();base=synthetic_receipt(text)
        mutants=[]
        for field in ["kernel","logical_cpus","affinity","cpu_model","memory_total","os_release","storage"]:
            r=copy.deepcopy(base);r["hardware"].pop(field);mutants.append(r)
        for field,value in [("logical_cpus",True),("logical_cpus",0),("cpu_model",""),
                            ("memory_total","zero"),("kernel",[]),("affinity",[True])]:
            r=copy.deepcopy(base);r["hardware"][field]=value;mutants.append(r)
        for field,value in [("tree","bad"),("files",{"unrelated":"0"*64}),
                            ("files",{"../rust/Cargo.toml":"0"*64}),("local_controls",["scripts/unrelated.py"])]:
            r=copy.deepcopy(base)
            for side in ["source_before","source_after"]:r[side][field]=value
            mutants.append(r)
        for value in [{"config":"not-digest"},[],{"relative":"0"*64}]:
            r=copy.deepcopy(base);r["cargo_config_before"]=r["cargo_config_after"]=value;mutants.append(r)
        r=copy.deepcopy(base);r["build_profile"].update(release=1,locked=1,default_features=1,incremental=0);mutants.append(r)
        for r in mutants:
            with self.subTest(receipt=r),self.assertRaises(contract.ValidationError):collector.validate_receipt(r,text,"f1")

    def test_auxiliary_count_and_identity_booleans_are_not_integers(self):
        text=control_log()
        mutations=[lambda r:r["initial_state"].update(active_snapshot_entries=False),
                   lambda r:r["initial_state"].update(stable_A_entries=False),
                   lambda r:r["index_requests"][0].update(latest_generation_required=1),
                   lambda r:r.update(overlap_executions=False),
                   lambda r:r["settings"].update(files=1),lambda r:r.update(frames=True)]
        for mutate in mutations:
            with self.assertRaises(contract.ValidationError):
                contract.validate_log(mutate_record(text,"INDEX_PERF_SAMPLE",mutate),"f1")

    def test_intended_runner_requires_clean_image_and_workflow_source_locators(self):
        text=mutate_record(control_log(),"INDEX_PERF_META",lambda r:r["environment_identity"].update(os="linux"))
        base=synthetic_receipt(text)
        base["hardware"].update(os="linux",affinity=list(range(12)),memory_total="MemTotal:       12582912 kB",
                                os_release='NAME="Ubuntu"\nVERSION_ID="24.04"\n')
        for side in ["source_before","source_after"]:base[side]["files"][".github/workflows/perf-regression.yml"]="0"*64
        base["runner"]=dict(kind="intended-ubuntu-24.04",ImageOS="ubuntu24",ImageVersion="20261002.1.0",
            GITHUB_RUN_ID="123",GITHUB_RUN_ATTEMPT="1",GITHUB_JOB="indexing",GITHUB_SHA="a"*40,
            GITHUB_WORKFLOW_SHA="a"*40,GITHUB_WORKFLOW_REF="owner/repo/.github/workflows/perf-regression.yml@refs/heads/master")
        collector.validate_receipt(base,text,"f1")
        changes=[lambda r:r["runner"].update(ImageVersion="unknown"),lambda r:r["runner"].update(ImageVersion=True),
                 lambda r:r["runner"].update(GITHUB_RUN_ATTEMPT="0"),
                 lambda r:r["runner"].update(GITHUB_WORKFLOW_SHA="b"*40),
                 lambda r:r["runner"].update(GITHUB_WORKFLOW_REF="owner/repo/.github/workflows/missing.yml@refs/heads/master"),
                 lambda r:r["hardware"].update(cpu_model="unknown"),
                 lambda r:r["hardware"].pop("storage")]
        for change in changes:
            r=copy.deepcopy(base);change(r)
            with self.assertRaises(contract.ValidationError):collector.validate_receipt(r,text,"f1")
        r=copy.deepcopy(base)
        for side in ["source_before","source_after"]:r[side]["local_controls"]=["AGENTS.md"]
        with self.assertRaises(contract.ValidationError):collector.validate_receipt(r,text,"f1")

    def test_raw_and_receipt_both_required_and_failures_are_not_observations(self):
        text=control_log();receipt=synthetic_receipt(text)
        self.assertEqual(len(collector.validate_receipt(receipt,text,"f1")["rows"]),112)
        mutations=[lambda r:r.update(status="invalid"),lambda r:r.update(group="stable"),
            lambda r:r.update(raw_sha256="0"*64),lambda r:r["source_after"].update(head="0"*40),
            lambda r:r.update(fixtures_after=["owned-root"]),lambda r:r["build_profile"].update(release=False),
            lambda r:r["toolchain"].update(rustc="rustc 1.96.0 synthetic"),
            lambda r:r["hardware"].update(os="linux"),lambda r:r.update(test_leaf="zero-tests"),
            lambda r:r["processes"]["measurement"]["argv"].remove("--exact"),
            lambda r:r["processes"]["discovery"].update(pid=True),
            lambda r:r["processes"]["build"].update(elapsed_seconds=float("inf")),
            lambda r:r.update(artifact_features=["unexpected"]),
            lambda r:r.update(cargo_config_after={"config":"changed"})]
        for stage in ["build","discovery","measurement"]:
            for field,value in [("success",False),("returncode",1),("returncode",False),("timed_out",True),
                                ("orphan_detected",True),("leader_reaped",False),("group_absent",False)]:
                mutations.append(lambda r,s=stage,f=field,v=value:r["processes"][s].update({f:v}))
        for mutate in mutations:
            r=copy.deepcopy(receipt);mutate(r)
            with self.subTest(receipt=r),self.assertRaises(contract.ValidationError):
                collector.validate_receipt(r,text,"f1")

    def test_summarizer_revalidates_each_run_rejects_duplicates_and_mixed_cohorts(self):
        text=control_log()
        with tempfile.TemporaryDirectory() as name:
            folders=[]
            for n in range(2):
                folder=Path(name)/str(n);folder.mkdir();folders.append(folder)
                (folder/"measurement.log").write_text(text,encoding="utf-8",newline="")
                collector.write_json(folder/"receipt.json",synthetic_receipt(text,ident=f"synthetic-{n}"))
            summary=collector.summarize_runs(folders,"f1")
            self.assertEqual(summary["mode"],"observation-only")
            self.assertIsNone(summary["timing_gate"])
            self.assertEqual(summary["run_count"],2)
            self.assertEqual(len(summary["runs"][0]["cells"][0]["phases"]["index_ready_ms"]["control"]),7)
            (folders[0]/"measurement.log").write_bytes(text.replace("\n","\r\n").encode())
            with self.assertRaises(contract.ValidationError):collector.load_run(folders[0],"f1")
            (folders[0]/"measurement.log").write_bytes(text.encode())
            with self.assertRaises(contract.ValidationError):collector.summarize_runs([folders[0]]*2,"f1")
            r=synthetic_receipt(text,ident="synthetic-other");r["hardware"]["cpu_model"]="different"
            (folders[1]/"receipt.json").write_text(json.dumps(r))
            with self.assertRaises(contract.ValidationError):collector.summarize_runs(folders,"f1")
            (folders[0]/"measurement.log").write_text(text+"changed",encoding="utf-8",newline="")
            with self.assertRaises(contract.ValidationError):collector.summarize_runs([folders[0]],"f1")

    def test_unknown_comparison_input_sequence_and_runtime_override_fail_closed(self):
        text=control_log("matched")
        mutants=[mutate_record(text,"INDEX_PERF_SAMPLE",lambda r:r.update(comparison_kind="unknown")),
                 mutate_record(text,"INDEX_PERF_META",lambda r:r["runtime_settings"].update(search_threads=1))]
        lines=text.splitlines();index=next(i for i,s in enumerate(lines) if s.startswith("INDEX_PERF_SAMPLE ")
                and json.loads(s.partition(" ")[2])["case"]=="S2-files")
        base=json.loads(lines[index].partition(" ")[2])
        for mutate in [lambda r:r["input_trace"][1].update(query="item"),
                       lambda r:r["input_trace"][0].update(requested_root="wrong"),
                       lambda r:r["input_trace"][1].update(at_ms=0),
                       lambda r:r["input_trace"][0].update(GUI_ingested=1)]:
            r=copy.deepcopy(base);mutate(r);parts=list(lines);parts[index]="INDEX_PERF_SAMPLE "+json.dumps(r)
            mutants.append("\n".join(parts))
        for mutant in mutants:
            with self.assertRaises(contract.ValidationError):contract.validate_log(mutant,"matched")


def control_log(group="f1"):
    """Project retained raw values into a deliberately synthetic current schema."""
    with gzip.open(ROOT / "docs/history/indexing-perf-2026-10-04/stale-full-extra-full.log.gz", "rt",encoding="utf-8") as stream:
        lines = list(stream)
    meta = json.loads(next(s.split("INDEX_PERF_META ", 1)[1] for s in lines if "INDEX_PERF_META " in s))
    cells = contract.cells_for(group)
    rows = [json.loads(s.partition(" ")[2]) for s in lines if s.startswith("INDEX_PERF_SAMPLE ")]
    meta.update(selected_cases=list(contract.GROUPS[group]), selected_sources=["FileList", "Walker"],
                supported_cells=[{"case":c,"source":s} for c,s in cells], selected_source_cells=len(cells),
                unsupported_cells=[], coverage_kind="selected-subset", expected_rows=14*len(cells),
                tabchain_input_policy=contract.FULL_POLICY)
    output = ["running 1 test", "INDEX_PERF_META " + json.dumps(meta)]
    for case, source in cells:
        for condition in [False, True]:
            output.append("INDEX_PERF_RUN_START " + json.dumps(dict(profile=case, source=source, condition=condition, role="untimed-warmup", entries=100000)))
        selected = [r for r in rows if r["comparison"]==case and r["source"]==source]
        for row in selected:
            row["tabchain_input_policy"] = contract.FULL_POLICY
            output.append("INDEX_PERF_RUN_START " + json.dumps(dict(profile=case, source=source, condition=row["case"]!="B0", role="sample", entries=100000, pair=row["pair"], position=row["position"])))
            output.append("INDEX_PERF_SAMPLE " + json.dumps(row))
    output.append("test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1600 filtered out; finished in 1.00s")
    return "\n".join(output)+"\n"


def mutate_record(text, marker, change):
    lines=text.splitlines()
    index=next(i for i,s in enumerate(lines) if s.startswith(marker+" "))
    record=json.loads(lines[index].split(" ",1)[1]);change(record)
    lines[index]=marker+" "+json.dumps(record)
    return "\n".join(lines)+"\n"


class IndexingContractTests(unittest.TestCase):
    def test_all_73_frozen_victim_predicates_preserved_without_asserts(self):
        frozen=ast.parse((ROOT/"docs/history/indexing-perf-2026-10-04/summarize_extensions-stale-full.py").read_text(encoding="utf-8"))
        original=next(n for n in frozen.body if isinstance(n,ast.FunctionDef) and n.name=="validate_victim_contract")
        port=ast.parse((ROOT/"scripts/indexing_perf_contract.py").read_text(encoding="utf-8"))
        current=next(n for n in port.body if isinstance(n,ast.FunctionDef) and n.name=="validate_victim_contract")
        old=[ast.dump(n.test) for n in ast.walk(original) if isinstance(n,ast.Assert)]
        new=[ast.dump(n.args[0]) for n in ast.walk(current) if isinstance(n,ast.Call)
             and isinstance(n.func,ast.Name) and n.func.id=="require"]
        self.assertEqual(len(old),73)
        self.assertEqual(old,new)
        self.assertFalse(any(isinstance(n,ast.Assert) for n in ast.walk(port)))

    def test_retained_real_victim_controls_and_typed_failed_mutants(self):
        with gzip.open(ROOT/"docs/history/indexing-perf-2026-10-04/stale-full-extra-full.log.gz","rt",encoding="utf-8") as stream:
            rows=[json.loads(s.partition(" ")[2]) for s in stream if s.startswith("INDEX_PERF_SAMPLE ")]
        # The frozen validator applies this contract only to GUI-worker rows;
        # parser-only throughput has a separate schema (28 of the 616 rows).
        gui_rows=[r for r in rows if r["measurement_kind"]=="headless-GUI-actual-workers"]
        self.assertEqual(len(gui_rows),588)
        for row in gui_rows:
            contract.validate_victim_contract(row)
        failed=next(r for r in gui_rows if any(q["terminal_role"]==contract.STALE_FULL_ROLE for q in r["index_requests"]))
        mutations=[lambda q:q.update(stale_full_data_abort=None),
                   lambda q:q.update(terminal_offer_error="unrelated"),
                   lambda q:q.update(stale_full_data_abort_duplicate=True),
                   lambda q:q["stale_full_data_abort"].update(shutdown=True),
                   lambda q:q["stale_full_data_abort"].update(request_id=0),
                   lambda q:q["stale_full_data_abort"].update(latest_request_id=q["request_id"]),
                   lambda q:q["stale_full_data_abort"].update(at_ms=0),
                   lambda q:q.update(terminal_kind="finished"),
                   lambda q:q.update(terminal_offer_current=True)]
        for change in mutations:
            row=copy.deepcopy(failed);q=next(q for q in row["index_requests"] if q["terminal_role"]==contract.STALE_FULL_ROLE)
            change(q)
            with self.assertRaises(contract.ValidationError):contract.validate_victim_contract(row)

    def test_validation_is_effective_under_python_optimization(self):
        program="from scripts.indexing_perf_contract import require,ValidationError\ntry:require(False,'reject')\nexcept ValidationError:raise SystemExit(0)\nraise SystemExit(9)"
        for flags,env_value in [([],None),(["-O"],None),(["-OO"],None),([],"1"),([],"2")]:
            env=dict(os.environ);env.pop("PYTHONOPTIMIZE",None)
            if env_value is not None:env["PYTHONOPTIMIZE"]=env_value
            result=subprocess.run([sys.executable,*flags,"-c",program],cwd=ROOT,env=env,capture_output=True,text=True,timeout=10)
            self.assertEqual(result.returncode,0,result.stderr)

    def test_complete_priority_controls_and_phase_summaries(self):
        for group, count in [("f1",112),("matched",42),("stable",84)]:
            with self.subTest(group=group):
                result=contract.validate_log(control_log(group),group)
                self.assertEqual(len(result["rows"]),count)
                self.assertEqual(len(contract.summarize_rows(result["rows"])),count//14)

    def test_rejects_incomplete_malformed_or_wrong_intended_run(self):
        valid=control_log()
        mutants=[valid.replace("1 passed; 0 failed", "0 passed; 1 failed"),
                 valid.replace("INDEX_PERF_RUN_START ","LOST_START ",1),
                 valid+next(s for s in valid.splitlines() if s.startswith("INDEX_PERF_SAMPLE "))+"\n"]
        for field,value in [("entries",4096),("pairs",6),("native",True),("schema_version",2),("unsupported_cells",[{}])]:
            mutants.append(mutate_record(valid,"INDEX_PERF_META",lambda r,f=field,v=value:r.update({f:v})))
        mutants.append(mutate_record(valid,"INDEX_PERF_META",lambda r:r["environment_identity"].update(optimized=False)))
        for field,value in [("pair",True),("case","unknown"),("full_scale_cell",False),("correct",False),
                            ("results_ready_ms",-1),("full_wait_ms",float("nan")),("max_frame_ms",True),
                            ("expected_final_logical_entries",100000)]:
            mutants.append(mutate_record(valid,"INDEX_PERF_SAMPLE",lambda r,f=field,v=value:r.update({f:v})))
        mutants.append(mutate_record(valid,"INDEX_PERF_SAMPLE",lambda r:r["index_sender_load_at_t2"].update(queued=1)))
        mutants.append(mutate_record(valid,"INDEX_PERF_SAMPLE",lambda r:r["index_requests"][0].update(request_processing_returned_ms=None)))
        mutants.append(mutate_record(valid,"INDEX_PERF_SAMPLE",lambda r:r["index_requests"][0].update(actual_started_root="wrong-root")))
        for index,text in enumerate(mutants):
            with self.subTest(index=index),self.assertRaises(contract.ValidationError):contract.validate_log(text,"f1")

    def test_selection_and_json_duplicates_fail_closed(self):
        with self.assertRaises(contract.ValidationError):contract.validate_log(control_log(),"stable")
        with self.assertRaises(contract.ValidationError):contract.cells_for("all")
        with self.assertRaises(contract.ValidationError):contract.parse_json('{"pair":0,"pair":1}')

    def test_search_requires_real_evaluation_and_stable_full_candidate_identity(self):
        valid=control_log("stable")
        lines=valid.splitlines();i=next(i for i,s in enumerate(lines) if s.startswith("INDEX_PERF_SAMPLE ") and json.loads(s.partition(" ")[2])["case"]!="B0")
        base=json.loads(lines[i].partition(" ")[2])
        for update in [lambda r:r["worker_observations"].clear(),
                       lambda r:r["worker_observations"][0].update(evaluated_candidates=0),
                       lambda r:r["search_dispatch_bindings"][0].update(candidate_count=99999),
                       lambda r:r["search_dispatch_bindings"][0].update(candidate_is_initial_A=False)]:
            row=copy.deepcopy(base);update(row);mutant=list(lines);mutant[i]="INDEX_PERF_SAMPLE "+json.dumps(row)
            with self.assertRaises(contract.ValidationError):contract.validate_log("\n".join(mutant),"stable")

    def test_additional_partially_evaluated_cancellation_does_not_replace_full_search_proof(self):
        valid=control_log("stable");lines=valid.splitlines()
        i=next(i for i,s in enumerate(lines) if s.startswith("INDEX_PERF_SAMPLE ") and json.loads(s.partition(" ")[2])["case"]=="T1-S2")
        row=json.loads(lines[i].partition(" ")[2])
        binding=copy.deepcopy(row["search_dispatch_bindings"][0]);binding["request_id"]+=10000
        worker=copy.deepcopy(row["worker_observations"][0]);worker["request_id"]=binding["request_id"];worker["evaluated_candidates"]=1000
        row["search_dispatch_bindings"].append(binding);row["worker_observations"].append(worker);row["overlap_executions"]+=1
        lines[i]="INDEX_PERF_SAMPLE "+json.dumps(row)
        contract.validate_log("\n".join(lines),"stable")
        row["full_candidate_evaluations"]+=1;lines[i]="INDEX_PERF_SAMPLE "+json.dumps(row)
        with self.assertRaises(contract.ValidationError):contract.validate_log("\n".join(lines),"stable")
