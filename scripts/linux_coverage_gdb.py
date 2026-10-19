"""GDB core extraction; only explicit JSON records are publishable."""
import json
import os
import re

import gdb


def emit(value):
    print(json.dumps({"coverage_record": value}), flush=True)


for command in ("set pagination off", "set confirm off", "set auto-load off",
                "set debuginfod enabled off", "set print frame-arguments none",
                "set print entry-values no", "set print thread-events off", "set index-cache enabled off"):
    gdb.execute(command, to_string=True)
core = os.environ["FLISTWALKER_COVERAGE_CORE"]
if not re.fullmatch(r"/[A-Za-z0-9_./-]+", core):
    emit("coverage_unavailable=unsupported_core_path")
    raise SystemExit(2)
try:
    # GDB 15.1 accepts a safe bare filename; quoted names are version-dependent.
    # Discard the header, which can include original argv.
    gdb.execute("core-file " + core, to_string=True)
    pid = gdb.selected_inferior().pid
    signo = int(gdb.parse_and_eval("$_siginfo.si_signo"))
    if pid != int(os.environ["FLISTWALKER_COVERAGE_PID"]) or signo != int(os.environ["FLISTWALKER_COVERAGE_SIGNAL"]):
        emit("coverage_unavailable=core_pid_or_signal_mismatch")
        emit("coverage_actual_identity=" + json.dumps({"pid": pid, "signal": signo}))
        raise SystemExit(2)
except gdb.error as error:
    emit("coverage_unavailable=" + ("core_file_not_found" if "No such file" in str(error) else "core_load_or_siginfo"))
    raise SystemExit(2)
emit("coverage_identity_verified=true")
emit("coverage_signal=" + str(signo))


def capture(command):
    try:
        output = gdb.execute(command, to_string=True)
        if "PRIVATE_ARGUMENT_PROBE" in output:
            emit("coverage_unavailable=private_output_rejected")
            return False
        else:
            emit(output.rstrip("\n"))
            return True
    except gdb.error:
        emit("coverage_unavailable=" + command)
        return False


# Publish the faulting thread first. A later timeout/output limit still leaves
# its stack available; don't buffer every thread in one GDB command.
fault = gdb.selected_thread()
threads = gdb.selected_inferior().threads()
leader = next((thread for thread in threads if thread.ptid[1] == pid), None)
emit("coverage_fault_lwp=" + str(fault.ptid[1]))
emit("coverage_fault_is_leader=" + str(fault.ptid[1] == pid).lower())
if leader is not None:
    emit("coverage_leader_lwp=" + str(leader.ptid[1]))
else:
    emit("coverage_unavailable=leader_thread_missing")
emit("coverage_thread=" + str(fault.num))
complete = capture("bt 48")
capture("info registers")
capture("x/12i $pc")
complete = complete and leader is not None
if len(threads) > 256:
    emit("coverage_unavailable=thread_count_limit")
    complete = False
for thread in [t for t in threads if t.num != fault.num][:255]:
    thread.switch()
    emit("coverage_thread=" + str(thread.num))
    complete = capture("bt 48") and complete
fault.switch()
capture("info sharedlibrary")
if complete:
    emit("coverage_threads_complete=true")
