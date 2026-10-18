"""GDB-only fault reporting. No locals, argument values, environment dump or raw core."""
import json

import gdb


def on_stop(event):
    if not isinstance(event, gdb.SignalEvent):
        return
    signal = event.stop_signal
    if signal not in ("SIGSEGV", "SIGBUS", "SIGABRT", "SIGILL"):
        return
    print("coverage_diagnostic_stop=" + json.dumps({"signal": signal}), flush=True)
    for command in ("info program", "info threads", "thread apply all bt 48",
                    "info registers", "x/12i $pc", "info sharedlibrary"):
        try:
            gdb.execute(command)
        except gdb.error:
            print("coverage_diagnostic_unavailable=" + command, flush=True)
    # A stopped inferior has no normal child exit status; preserve failure explicitly.
    gdb.execute("quit " + str({"SIGSEGV": 139, "SIGBUS": 135, "SIGABRT": 134, "SIGILL": 132}[signal]))


for command in ("set pagination off", "set confirm off", "set auto-load off",
                "set debuginfod enabled off", "set print frame-arguments none",
                "set disable-randomization off", "set startup-with-shell off",
                "set print thread-events off", "set follow-fork-mode parent",
                "set detach-on-fork on", "handle SIGPIPE nostop noprint pass"):
    gdb.execute(command)
gdb.events.stop.connect(on_stop)
gdb.execute("run")
