"""GDB-only fault reporting. No locals, argument values, environment dump or raw core."""
import json
import os

import gdb


def emit(value):
    if os.environ.get("COVERAGE_DIAGNOSTIC_CORE"):
        print(json.dumps({"coverage_diagnostic_output": value}), flush=True)
    else:
        print(value, flush=True)


def dump_stacks(identity):
    emit("coverage_diagnostic_stop=" + json.dumps(identity))
    for command in ("info program", "info threads", "thread apply all bt 48",
                    "info registers", "x/12i $pc", "info sharedlibrary"):
        try:
            output = gdb.execute(command, to_string=True)
            if output:
                if "PRIVATE_ARGUMENT_PROBE" in output:
                    emit("coverage_diagnostic_private_output_rejected=" + command)
                else:
                    emit(output.rstrip("\n"))
        except gdb.error:
            emit("coverage_diagnostic_unavailable=" + command)


def on_stop(event):
    if isinstance(event, gdb.SignalEvent) and event.stop_signal in ("SIGSEGV", "SIGBUS", "SIGABRT", "SIGILL"):
        dump_stacks({"signal": event.stop_signal})
        # A stopped inferior has no normal child exit status; preserve failure explicitly.
        gdb.execute("quit " + str({"SIGSEGV": 139, "SIGBUS": 135, "SIGABRT": 134, "SIGILL": 132}[event.stop_signal]))


for command in ("set pagination off", "set confirm off", "set auto-load off",
                "set debuginfod enabled off", "set print frame-arguments none",
                "set disable-randomization off", "set startup-with-shell off",
                "set print thread-events off", "set follow-fork-mode parent",
                "set detach-on-fork on", "handle SIGPIPE nostop noprint pass"):
    gdb.execute(command)
core = os.environ.get("COVERAGE_DIAGNOSTIC_CORE")
if core:
    # GDB's default core header prints the original argv. Discard that header.
    gdb.execute("core-file " + json.dumps(core), to_string=True)
    dump_stacks({"signal_number": int(gdb.parse_and_eval("$_siginfo.si_signo")), "mode": "core"})
else:
    gdb.events.stop.connect(on_stop)
    gdb.execute("run")
