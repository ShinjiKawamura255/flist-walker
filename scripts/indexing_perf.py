#!/usr/bin/env python3
"""Collect source-bound indexing observations and same-run reference comparisons.

The process wrapper supports POSIX. validate/summarize-runs use only retained
JSON/text and work on other systems. Receipts describe execution, not signatures
or security attestations; admission assumes trusted source and runner artifacts.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import signal
import statistics
import subprocess
import sys
import time
import uuid
import re
from pathlib import Path, PurePosixPath

if __package__:
    from . import indexing_perf_contract as contract
else:
    import indexing_perf_contract as contract

TEST = "app::tests::indexing_perf::harness::extensions::runner::perf_indexing_extended_paired"
HEX = re.compile(r"[0-9a-f]{64}")
REVISION = re.compile(r"[0-9a-f]{40}")
REQUIRED_SOURCES = {
    "rust/Cargo.toml", "rust/Cargo.lock", "rust/rust-toolchain.toml",
    "rust/src/app/tests/indexing_perf/mod.rs", "rust/src/app/tests/indexing_perf/harness.rs",
    *{"rust/src/app/tests/indexing_perf/extensions/"+name+".rs"
      for name in ("runner", "driver", "fixture", "oracle", "cases")},
    "scripts/indexing_perf.py", "scripts/indexing_perf_contract.py",
}
CONTROLS = {"AGENTS.md", "docs/EXECUTION-PLAN-20261004-roadmap-indexing-perf-rollout.md",
            "docs/EXECUTION-PLAN-20261004-slice-a-indexing-perf-rollout.md",
            "docs/EXECUTION-PLAN-20261004-slice-b-indexing-perf-calibration.md",
            "docs/EXECUTION-WORK-ITEMS-20261004-indexing-perf-rollout.json",
            "docs/EXECUTION-PLAN-20261005-roadmap-indexing-speed-gate.md",
            "docs/EXECUTION-PLAN-20261005-slice-a-indexing-speed-gate.md",
            "docs/EXECUTION-PLAN-20261005-slice-b-indexing-speed-gate.md",
            "docs/EXECUTION-WORK-ITEMS-20261005-indexing-speed-gate.json"}


def sha(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def write_json(path, value):
    """Only own caller-reserved output paths; never replace an existing artifact."""
    with Path(path).open("x", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2, allow_nan=False)
        stream.write("\n")


def child_environment(inherited, group, fixture):
    contract.cells_for(group)
    excluded = ("FLISTWALKER_", "FW_INDEX_PERF_", "CARGO_PROFILE_", "CARGO_BUILD_", "CARGO_TARGET_")
    exact = {"RUSTUP_TOOLCHAIN", "RUSTC", "RUSTDOC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER",
             "RUSTFLAGS", "RUSTDOCFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_ENCODED_RUSTDOCFLAGS",
             "CARGO_TARGET_DIR", "CARGO_INCREMENTAL"}
    result = {k:v for k,v in inherited.items() if k not in exact and not k.startswith(excluded)}
    result.update(TMPDIR=str(fixture), TMP=str(fixture), TEMP=str(fixture), CARGO_INCREMENTAL="0",
                  FW_INDEX_PERF_EXTRA_ENTRIES="100000", FW_INDEX_PERF_EXTRA_PAIRS=str(contract.PAIR_COUNTS[group]),
                  FW_INDEX_PERF_EXTRA_CASES=",".join(contract.GROUPS[group]),
                  FW_INDEX_PERF_EXTRA_SOURCES="FileList,Walker")
    return result


def group_alive(pid):
    try:
        os.killpg(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        # macOS may briefly report EPERM for a terminated, reaping group.
        # This is never absence: keep waiting and admit only an actual ESRCH.
        return True


def run_owned_process(argv, cwd, env, log_path, timeout, cleanup_timeout=5):
    """A new session owns descendants. Any timeout/orphan invalidates evidence.

    Reap the leader even on wrapper exceptions. TERM then KILL only this session's
    group; bound absence checks separately. Escaped process sessions are outside
    this mechanism and are forbidden by the source-bound harness contract.
    """
    contract.require(os.name == "posix", "collection requires POSIX process groups")
    contract.number(timeout, "process timeout")
    contract.number(cleanup_timeout, "cleanup timeout")
    contract.require(timeout > 0 and cleanup_timeout > 0, "positive process bounds required")
    started = time.monotonic()
    timed_out = orphan = False
    with Path(log_path).open("xb") as log:
        process = subprocess.Popen(argv, cwd=cwd, env=env, stdout=log,
                                   stderr=subprocess.STDOUT, start_new_session=True)
        try:
            try:
                process.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                timed_out = True
            orphan = not timed_out and group_alive(process.pid)
        finally:
            if group_alive(process.pid):
                try:
                    os.killpg(process.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                deadline = time.monotonic() + cleanup_timeout / 2
                while time.monotonic() < deadline:
                    process.poll()
                    if not group_alive(process.pid):
                        break
                    time.sleep(.01)
                if group_alive(process.pid):
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
            process.wait(timeout=cleanup_timeout)
            deadline = time.monotonic() + cleanup_timeout / 2
            while group_alive(process.pid) and time.monotonic() < deadline:
                time.sleep(.01)
    absent = not group_alive(process.pid)
    return dict(argv=argv, cwd=str(cwd), pid=process.pid, process_group=process.pid,
                returncode=process.returncode, timed_out=timed_out, orphan_detected=orphan,
                leader_reaped=True, group_absent=absent, elapsed_seconds=time.monotonic()-started,
                timeout_seconds=timeout, cleanup_timeout_seconds=cleanup_timeout,
                success=process.returncode == 0 and not timed_out and not orphan and absent)


def capture(argv, cwd, env):
    result = subprocess.run(argv, cwd=cwd, env=env, text=True, capture_output=True,
                            timeout=30, check=True)
    return result.stdout.rstrip("\n")


def source_identity(root, expected, allow_controls=False):
    head = capture(["git", "rev-parse", "HEAD"], root, os.environ)
    contract.require(head == expected, "source HEAD differs from requested exact revision")
    status = capture(["git", "status", "--porcelain=v1", "--untracked-files=all"], root, os.environ)
    changes = [line[3:] for line in status.splitlines()]
    contract.require(not changes or allow_controls and set(changes) <= CONTROLS,
                     "source checkout is dirty (local control allowance is explicit and fixed)")
    paths = capture(["git", "ls-files", "-z"], root, os.environ).split("\0")
    files = {p:sha(root/p) for p in paths if p and
             (p.startswith(("rust/", "scripts/", ".github/workflows/", ".cargo/")))}
    contract.require(files and "rust/rust-toolchain.toml" in files and
                     "scripts/indexing_perf.py" in files and
                     "scripts/indexing_perf_contract.py" in files, "collector/build source not tracked")
    return dict(head=head, tree=capture(["git", "rev-parse", "HEAD^{tree}"], root, os.environ),
                files=files, local_controls=sorted(changes))


def build_config_identity(root, env):
    """Cargo searches ancestors and CARGO_HOME. Retain hashes, never file contents."""
    directories = [root/"rust", root, *root.parents,
                   Path(env.get("CARGO_HOME", str(Path.home()/".cargo")))]
    files = {}
    for directory in directories:
        cargo = directory if directory == directories[-1] else directory/".cargo"
        for name in ("config", "config.toml"):
            path = cargo/name
            if path.is_file():
                files[str(path.resolve())] = sha(path)
    return files


def hardware_identity(root):
    facts = dict(os=platform.system().lower(), arch=platform.machine(), kernel=platform.release(),
                 logical_cpus=os.cpu_count(), affinity=sorted(os.sched_getaffinity(0))
                 if hasattr(os,"sched_getaffinity") else None)
    if sys.platform == "linux":
        cpu = Path("/proc/cpuinfo").read_text(encoding="utf-8")
        facts["cpu_model"] = next((s.partition(":")[2].strip() for s in cpu.splitlines()
                                  if s.startswith("model name")), "unknown")
        facts["memory_total"] = next(s for s in Path("/proc/meminfo").read_text(encoding="utf-8").splitlines()
                                     if s.startswith("MemTotal:"))
        facts["os_release"] = Path("/etc/os-release").read_text(encoding="utf-8")
    elif sys.platform == "darwin":
        facts["os"] = "macos"
        facts["cpu_model"] = capture(["sysctl","-n","machdep.cpu.brand_string"],root,os.environ)
        facts["memory_total"] = capture(["sysctl","-n","hw.memsize"],root,os.environ)
        facts["os_release"] = platform.mac_ver()[0]
    else:
        raise contract.ValidationError("unsupported collection OS")
    disk = os.statvfs(root)
    facts["storage"] = dict(device=os.stat(root).st_dev, block_size=disk.f_frsize,
                            total_bytes=disk.f_blocks*disk.f_frsize)
    return facts


def runner_identity(env, local):
    if local:
        contract.require(env.get("GITHUB_ACTIONS") != "true", "CI cannot claim local observation")
        return dict(kind="local-observation", image_os=None, image_version=None)
    required = ("GITHUB_RUN_ID","GITHUB_RUN_ATTEMPT","GITHUB_JOB","GITHUB_SHA",
                "GITHUB_WORKFLOW_REF","GITHUB_WORKFLOW_SHA","ImageOS","ImageVersion")
    contract.require(env.get("GITHUB_ACTIONS") == "true" and all(env.get(k) for k in required),
                     "intended runner requires complete CI/image locators")
    contract.require(env["ImageOS"] == "ubuntu24", "intended image is ubuntu-24.04")
    return dict(kind="intended-ubuntu-24.04", **{k:env[k] for k in required})


def discover_executable(build_log):
    artifacts=[]
    for line in Path(build_log).read_text(encoding="utf-8").splitlines():
        if line.startswith("{"):
            record=contract.parse_json(line)
            if record.get("reason") == "compiler-artifact" and record.get("executable") and \
                    record.get("profile",{}).get("test") and record["target"]["kind"] == ["lib"]:
                artifacts.append(record["executable"])
    contract.require(len(artifacts) == 1, "expected one release lib test executable")
    path=Path(artifacts[0]).resolve()
    contract.require(path.is_file(), "missing built executable")
    return path


def validate_digest_map(value, source=False):
    contract.require(type(value) is dict, "digest map schema")
    for path,digest in value.items():
        contract.require(type(path) is str and type(digest) is str and HEX.fullmatch(digest), "path/digest schema")
        if source:
            parts=PurePosixPath(path).parts
            contract.require(path and not PurePosixPath(path).is_absolute() and
                             path == str(PurePosixPath(path)) and ".." not in parts and
                             "\\" not in path and ":" not in path and
                             path.startswith(("rust/","scripts/",".github/workflows/",".cargo/")), "source path")
        else:
            contract.require(path.startswith("/") or re.match(r"^[A-Za-z]:[\\/]",path), "Cargo config absolute path")
            contract.require(path.endswith(("/config","/config.toml","\\config","\\config.toml")), "Cargo config filename")


def validate_source_record(source):
    contract.require(type(source) is dict and set(source) == {"head","tree","files","local_controls"}, "source schema")
    for field in ("head","tree"):
        contract.require(type(source[field]) is str and REVISION.fullmatch(source[field]), "source " + field)
    validate_digest_map(source["files"],source=True)
    contract.require(REQUIRED_SOURCES <= set(source["files"]), "missing required collector/build source")
    controls=source["local_controls"]
    contract.require(type(controls) is list and all(type(p) is str for p in controls) and
                     controls == sorted(set(controls)) and set(controls) <= CONTROLS, "local controls schema")


def validate_hardware_record(hardware):
    contract.require(type(hardware) is dict and set(hardware) == {"os","arch","kernel","logical_cpus",
                     "affinity","cpu_model","memory_total","os_release","storage"}, "hardware schema")
    contract.require(hardware["os"] in ("linux","macos"), "hardware OS")
    for field in ("arch","kernel","cpu_model","os_release"):
        contract.require(type(hardware[field]) is str and bool(hardware[field].strip()), "hardware " + field)
    cpus=contract.integer(hardware["logical_cpus"],"hardware logical CPUs",1)
    affinity=hardware["affinity"]
    if hardware["os"] == "linux":
        contract.require(type(affinity) is list and affinity and
                         all(type(n) is int and 0 <= n < cpus for n in affinity) and
                         affinity == sorted(set(affinity)), "hardware affinity")
        match=re.fullmatch(r"MemTotal:\s+([0-9]+) kB",hardware["memory_total"])
    else:
        contract.require(affinity is None, "macOS affinity schema")
        match=re.fullmatch(r"([0-9]+)",hardware["memory_total"])
    contract.require(match is not None and int(match[1]) > 0, "hardware memory total")
    storage=hardware["storage"]
    contract.require(type(storage) is dict and set(storage)=={"device","block_size","total_bytes"}, "hardware storage schema")
    contract.integer(storage["device"],"storage device")
    contract.integer(storage["block_size"],"storage block size",1)
    contract.integer(storage["total_bytes"],"storage total bytes",1)


def validate_runner_record(runner, source, hardware, collector_source=None):
    contract.require(type(runner) is dict, "runner schema")
    kind=runner["kind"]
    if kind == "same-job-reference":
        contract.require(collector_source is not None, "reference collector provenance missing")
        validate_source_record(collector_source)
        execution_runner = dict(runner, kind="intended-ubuntu-24.04")
        validate_runner_record(execution_runner, collector_source, hardware)
        contract.require(not source["local_controls"], "reference source must be clean")
        return
    if kind == "local-observation":
        contract.require(set(runner)=={"kind","image_os","image_version"} and
                         runner["image_os"] is None and runner["image_version"] is None, "local runner schema")
        return
    contract.require(kind == "intended-ubuntu-24.04", "runner kind")
    keys={"GITHUB_RUN_ID","GITHUB_RUN_ATTEMPT","GITHUB_JOB","GITHUB_SHA","GITHUB_WORKFLOW_REF",
          "GITHUB_WORKFLOW_SHA","ImageOS","ImageVersion"}
    contract.require(set(runner)==keys|{"kind"} and all(type(runner[k]) is str and runner[k].strip() for k in keys), "CI locator schema")
    contract.require(runner["ImageOS"] == "ubuntu24" and runner["ImageVersion"].lower() != "unknown", "intended runner image")
    for key in ("GITHUB_RUN_ID","GITHUB_RUN_ATTEMPT"):
        contract.require(re.fullmatch(r"[1-9][0-9]*",runner[key]), "CI numeric locator")
    contract.require(runner["GITHUB_SHA"] == runner["GITHUB_WORKFLOW_SHA"] == source["head"], "CI source/workflow revision")
    locator=re.fullmatch(r"[^/@]+/[^/@]+/(\.github/workflows/[^/@]+\.ya?ml)@refs/heads/.+",runner["GITHUB_WORKFLOW_REF"])
    contract.require(locator is not None and locator[1] in source["files"], "workflow source manifest locator")
    contract.require(not source["local_controls"] and hardware["os"] == "linux" and
                     'VERSION_ID="24.04"' in hardware["os_release"] and hardware["cpu_model"].lower() != "unknown", "intended clean Ubuntu hardware")


def validate_receipt(receipt, text, group):
    """Recheck retained receipt plus raw data, never accept a summary alone."""
    try:
        contract.require(type(receipt["schema_version"]) is int and receipt["schema_version"] == 1
                         and receipt["status"] == "observed-valid" and receipt["group"] == group,
                         "receipt schema/status/selection")
        contract.require(receipt["raw_sha256"] == hashlib.sha256(text.encode()).hexdigest(), "raw digest")
        before,after=receipt["source_before"],receipt["source_after"]
        validate_source_record(before)
        validate_source_record(after)
        contract.require(before == after, "source changed during collection")
        validate_hardware_record(receipt["hardware"])
        for name in ("build", "discovery", "measurement"):
            process=receipt["processes"][name]
            contract.require(process["success"] is True and process["leader_reaped"] is True
                             and process["group_absent"] is True and process["returncode"] == 0
                             and type(process["returncode"]) is int and process["timed_out"] is False
                             and process["orphan_detected"] is False, "unclean process: " + name)
            contract.require(contract.integer(process["pid"], "process pid", 1) ==
                             contract.integer(process["process_group"], "process group", 1), "process ownership")
            for field in ("elapsed_seconds", "timeout_seconds", "cleanup_timeout_seconds"):
                contract.number(process[field], "process " + field)
            contract.require(process["timeout_seconds"] > 0 and process["cleanup_timeout_seconds"] > 0,
                             "process bounds")
        contract.require(set(receipt["processes"]) == {"build","discovery","measurement"}, "process stages")
        executable=receipt["executable"]
        contract.require(receipt["processes"]["measurement"]["argv"] ==
                         [executable,TEST,"--exact","--ignored","--nocapture","--test-threads=1"] and
                         receipt["processes"]["discovery"]["argv"] == [executable,TEST,"--exact","--list","--ignored"],
                         "actual exact test invocation")
        contract.require(receipt["processes"]["build"]["argv"] == ["cargo","test","--release","--locked",
                         "--lib","--no-run","--message-format=json","--target-dir",receipt["build_target"]],
                         "actual build invocation")
        contract.require(all(p["cwd"] == receipt["rust_root"] for p in receipt["processes"].values()), "process working directory")
        validate_digest_map(receipt["cargo_config_before"])
        validate_digest_map(receipt["cargo_config_after"])
        contract.require(receipt["cargo_config_before"] == receipt["cargo_config_after"], "Cargo config changed")
        contract.require(receipt["fixtures_before"] == receipt["fixtures_after"] == [], "fixture remnants")
        scratch=receipt["compiler_temp_after"]
        contract.require(receipt["compiler_temp_before"] == [] and type(scratch) is list and
                         all(type(p) is str and p and p not in (".","..") and "/" not in p and "\\" not in p for p in scratch)
                         and scratch == sorted(set(scratch)), "compiler temporary inventory schema")
        profile=receipt["build_profile"]
        contract.require(type(profile) is dict and set(profile)=={"release","locked","default_features","additional_features","incremental"}, "build profile schema")
        for field in ("release","locked","default_features"):
            contract.require(profile[field] is True, "build profile " + field)
        contract.require(profile["incremental"] is False and type(profile["additional_features"]) is list and
                         profile["additional_features"] == [], "build profile drift")
        contract.require(HEX.fullmatch(receipt["executable_sha256"]) is not None and
                         receipt["artifact_features"] == [], "executable/features")
        contract.require(receipt["test_leaf"] == TEST, "test discovery identity")
        contract.require(receipt["toolchain"]["rustc"].startswith("rustc 1.97.1 ") and
                         receipt["toolchain"]["cargo"].startswith("cargo 1.97.1 "), "actual toolchain")
        executor = receipt.get("collector_source")
        if executor is not None:
            validate_source_record(executor)
            contract.require("scripts/indexing_perf_gate.py" in executor["files"], "comparison collector missing")
            if receipt["runner"]["kind"] != "same-job-reference":
                contract.require(executor == before, "candidate collector/source mismatch")
        validate_runner_record(receipt["runner"],before,receipt["hardware"],executor)
        result=contract.validate_log(text,group)
        identity=result["metadata"]["environment_identity"]
        contract.require(identity["os"] == receipt["hardware"]["os"] and
                         identity["arch"] == {"arm64":"aarch64"}.get(receipt["hardware"]["arch"],receipt["hardware"]["arch"]), "compiled platform mismatch")
        return result
    except (KeyError,TypeError,AttributeError,ValueError) as error:
        raise contract.ValidationError("malformed receipt: " + str(error)) from error


def load_run(folder, group):
    folder=Path(folder)
    receipt=contract.parse_json((folder/"receipt.json").read_text(encoding="utf-8"))
    text=(folder/"measurement.log").read_bytes().decode("utf-8")
    return receipt,validate_receipt(receipt,text,group)


def summarize_runs(folders, group):
    runs=[load_run(folder,group) for folder in folders]
    def cohort(receipt,result):
        return dict(source=receipt["source_before"],toolchain=receipt["toolchain"],
                    cargo_config=receipt["cargo_config_before"],
                    hardware=receipt["hardware"],runner={k:v for k,v in receipt["runner"].items()
                    if k in ("kind","ImageOS","ImageVersion")},build=receipt["build_profile"],
                    environment=result["metadata"]["environment_identity"],
                    settings={k:v for k,v in result["metadata"]["runtime_settings"].items()
                              if k != "window_trace_path"})
    identity=cohort(*runs[0]);seen=set()
    summaries=[]
    for receipt,result in runs:
        contract.require(cohort(receipt,result)==identity,"mixed healthy source/runner/hardware/settings cohort")
        key=receipt["run_id"]
        contract.require(key not in seen,"duplicate run identity")
        seen.add(key)
        summaries.append(dict(run_id=key,cells=contract.summarize_rows(result["rows"])))
    across=[]
    for i,cell in enumerate(summaries[0]["cells"]):
        phases={}
        for phase in contract.PHASES:
            phases[phase]={}
            for arm in ("control","condition"):
                for statistic in ("median","max"):
                    values=[s["cells"][i]["phases"][phase][arm+"_"+statistic] for s in summaries]
                    phases[phase][arm+"_"+statistic]=dict(values=values,median=statistics.median(values),
                                                        minimum=min(values),maximum=max(values),spread=max(values)-min(values))
        across.append(dict(case=cell["case"],source=cell["source"],phases=phases))
    return dict(schema_version=1,mode="observation-only",group=group,cohort=identity,
                run_count=len(runs),runs=summaries,across_runs=across,
                timing_gate=None,limitation="No timing threshold or statistical guarantee; all fixed-group pairs retained. Different work and platform observations remain separate.")


def collect_once(args):
    root=Path(args.root).resolve();out=Path(args.output).resolve()
    contract.require(os.name == "posix", "collection requires POSIX")
    contract.require(not out.exists(), "output already exists")
    source=source_identity(root,args.revision,args.allow_local_controls)
    runner=runner_identity(os.environ,args.local_observation)
    executor = getattr(args, "collector_source", None)
    if executor is not None:
        validate_source_record(executor)
        executing_root = Path(__file__).resolve().parents[1]
        contract.require(source_identity(executing_root, executor["head"]) == executor,
                         "executing collector source changed")
        if source["head"] != executor["head"]:
            contract.require(runner["kind"] == "intended-ubuntu-24.04", "reference requires real hosted context")
            runner = dict(runner, kind="same-job-reference")
    execution_head = executor["head"] if executor is not None else source["head"]
    contract.require(runner["kind"] == "local-observation" or runner["GITHUB_SHA"] == execution_head, "CI source mismatch")
    out.mkdir(parents=True)
    fixtures=out/"fixtures";fixtures.mkdir()
    build_temp=out/"compiler-temp";build_temp.mkdir()
    env=child_environment(os.environ,args.group,fixtures)
    build_env=child_environment(os.environ,args.group,build_temp)
    receipt=dict(schema_version=1,status="invalid",group=args.group,run_id=str(uuid.uuid4()),
                 source_before=source,runner=runner,
                 build_profile=dict(release=True,locked=True,default_features=True,additional_features=[],incremental=False),
                 processes={},fixtures_before=sorted(p.name for p in fixtures.iterdir()),test_leaf=TEST,
                 compiler_temp_before=sorted(p.name for p in build_temp.iterdir()))
    if executor is not None:
        receipt.update(collector_source=executor, collection_interval_ns={"started":time.monotonic_ns()})
    try:
        rust=root/"rust"
        build_target = Path(getattr(args, "build_target", out/"build")).resolve()
        receipt.update(hardware=hardware_identity(out),rust_root=str(rust),build_target=str(build_target),
                       cargo_config_before=build_config_identity(root,build_env))
        receipt["toolchain"]={"rustc":capture(["rustc","--version","--verbose"],rust,build_env),
                              "cargo":capture(["cargo","--version"],rust,build_env)}
        build=["cargo","test","--release","--locked","--lib","--no-run","--message-format=json",
               "--target-dir",str(build_target)]
        budget = getattr(args, "budget", None)
        def stage(kind, argv, env, log, bound):
            if budget is None:
                return run_owned_process(argv,rust,env,log,bound)
            return budget.run(kind,argv,rust,env,log)
        receipt["processes"]["build"]=stage("build",build,build_env,out/"build.log",args.build_timeout)
        contract.require(receipt["processes"]["build"]["success"], "build did not exit cleanly")
        binary=discover_executable(out/"build.log")
        contract.require(binary.is_relative_to(build_target), "artifact outside owned build")
        receipt["executable_sha256"]=sha(binary)
        receipt["executable"]=str(binary)
        receipt["artifact_features"]=next(contract.parse_json(line)["features"] for line in
            (out/"build.log").read_text(encoding="utf-8").splitlines() if line.startswith("{") and
            contract.parse_json(line).get("executable") == str(binary))
        receipt["processes"]["discovery"]=stage("discovery",[str(binary),TEST,"--exact","--list","--ignored"],env,out/"discovery.log",30)
        contract.require(receipt["processes"]["discovery"]["success"],"test discovery failed")
        listed=(out/"discovery.log").read_text(encoding="utf-8").splitlines()
        contract.require([s for s in listed if s.endswith(": test")] == [TEST+": test"],"missing/duplicate exact test")
        receipt["processes"]["measurement"]=stage("measurement",[str(binary),TEST,"--exact","--ignored","--nocapture","--test-threads=1"],env,out/"measurement.log",args.measurement_timeout)
        text=(out/"measurement.log").read_bytes().decode("utf-8")
        receipt.update(raw_sha256=sha(out/"measurement.log"),source_after=source_identity(root,args.revision,args.allow_local_controls),
                       cargo_config_after=build_config_identity(root,env),
                       compiler_temp_after=sorted(p.name for p in build_temp.iterdir()),
                       fixtures_after=sorted(p.name for p in fixtures.iterdir()),status="observed-valid")
        result=validate_receipt(receipt,text,args.group)
        # Build/discovery timing is outside Rust t0 endpoints. Logs/fixtures survive failures.
        write_json(out/"summary.json",dict(schema_version=1,mode="observation-only",group=args.group,
                                          cells=contract.summarize_rows(result["rows"]),timing_gate=None))
    except BaseException as error:
        receipt["status"]="invalid"
        receipt["error"]=type(error).__name__+": "+str(error)
        raise
    finally:
        if executor is not None:
            receipt["collection_interval_ns"]["finished"] = time.monotonic_ns()
        receipt["compiler_temp_after"]=sorted(p.name for p in build_temp.iterdir())
        write_json(out/"receipt.json",receipt)


def comparison_module():
    if __package__:
        from . import indexing_perf_gate
    else:
        import indexing_perf_gate
    return indexing_perf_gate


def collect(args):
    if args.local_observation:
        return collect_once(args)
    return comparison_module().collect_triplet(args)


def main(argv=None):
    parser=argparse.ArgumentParser(description=__doc__)
    commands=parser.add_subparsers(dest="command",required=True)
    run=commands.add_parser("collect")
    run.add_argument("--root",default=str(Path(__file__).resolve().parents[1]))
    run.add_argument("--group",choices=contract.GROUPS,required=True)
    run.add_argument("--revision",required=True)
    run.add_argument("--output",required=True)
    run.add_argument("--build-timeout",type=float,default=1800)
    run.add_argument("--measurement-timeout",type=float,default=2700)
    run.add_argument("--local-observation",action="store_true")
    run.add_argument("--allow-local-controls",action="store_true")
    validate=commands.add_parser("validate")
    validate.add_argument("--group",choices=contract.GROUPS,required=True)
    validate.add_argument("folder")
    comparison=commands.add_parser("compare-proposal")
    comparison.add_argument("--group",choices=contract.GROUPS,required=True)
    comparison.add_argument("--output",required=True)
    comparison.add_argument("folder")
    summary=commands.add_parser("summarize-runs")
    summary.add_argument("--group",choices=contract.GROUPS,required=True)
    summary.add_argument("--output",required=True)
    summary.add_argument("folders",nargs="+")
    args=parser.parse_args(argv)
    previous_handler = None
    if args.command == "collect" and os.name == "posix":
        def terminated(_signum, _frame):
            raise KeyboardInterrupt("SIGTERM")
        previous_handler = signal.signal(signal.SIGTERM, terminated)
    try:
        if args.command == "collect":
            collect(args)
        elif args.command == "validate":
            receipt=contract.parse_json((Path(args.folder)/"receipt.json").read_text(encoding="utf-8"))
            if "comparison" in receipt:
                report=comparison_module().load_comparison(args.folder,args.group)
                print(json.dumps(report,ensure_ascii=False,allow_nan=False))
                if receipt["comparison"]["enforced"]:
                    contract.require(report["status"] == "pass", "numeric gate " + report["status"])
            else:
                contract.require(not (receipt.get("runner",{}).get("kind") == "intended-ubuntu-24.04" and
                    "scripts/indexing_perf_gate.py" in receipt.get("source_before",{}).get("files",{})),
                    "hosted comparison receipt missing")
                _,result=load_run(args.folder,args.group)
                print(json.dumps(dict(status="observed-valid",rows=len(result["rows"]))))
        elif args.command == "compare-proposal":
            report=comparison_module().load_comparison(args.folder,args.group,proposal=True)
            write_json(args.output,report)
            contract.require(report["status"] == "pass", "proposal " + report["status"])
        else:
            write_json(args.output,summarize_runs(args.folders,args.group))
    except (contract.ValidationError,OSError,subprocess.SubprocessError) as error:
        print("indexing perf rejected: "+str(error),file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        print("indexing perf interrupted; retained evidence is invalid",file=sys.stderr)
        return 130
    finally:
        if previous_handler is not None:
            signal.signal(signal.SIGTERM, previous_handler)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
