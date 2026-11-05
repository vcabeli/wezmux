"""Regression checks for hook tty routing: python3 test-data/test-hook-tty.py."""

import json
import os
from pathlib import Path
import pty
import select
import sqlite3
import subprocess
import sys
import tempfile
import unittest


BIN = Path(__file__).resolve().parents[1] / "bin/hooks"
HOOKS = [
    BIN / "on-stop.sh",
    BIN / "on-prompt-submit.sh",
    BIN / "codex/on-stop.sh",
]

# Stands in for the agent: runs the hook detached from any controlling
# terminal, the way Claude Code 2.1.143+ and the Codex daemon do.
AGENT = """
import subprocess, sys
subprocess.run([sys.argv[1]], input=sys.argv[2], text=True,
               start_new_session=True, timeout=5)
"""

PAYLOAD = json.dumps({"session_id": "t", "last_assistant_message": "Done"})

# Stands in for the Codex app-server daemon: a process named codex (bash run
# through a symlink), with no terminal, whose command line says app-server, running the hook as a child.
DAEMON = '"$0" <<<"$1"; true'


class HookTtyTests(unittest.TestCase):
    def setUp(self):
        self.env = {k: v for k, v in os.environ.items() if k != "WEZMUX_HOOK_TTY"}

    def test_hook_writes_to_the_agents_terminal(self):
        for hook in HOOKS:
            with self.subTest(hook=hook.name):
                pid, master = pty.fork()
                if pid == 0:
                    os.execve(sys.executable, [sys.executable, "-c", AGENT, str(hook), PAYLOAD],
                              self.env)
                try:
                    output = b""
                    while select.select([master], [], [], 5)[0]:
                        try:
                            chunk = os.read(master, 65536)
                        except OSError:
                            break
                        if not chunk:
                            break
                        output += chunk
                    self.assertIn(b"\x1b]7777;status;", output)
                finally:
                    os.close(master)
                    os.waitpid(pid, 0)

    def test_agent_without_terminal_ignores_inherited_pane_tty(self):
        # A daemon started from some pane still carries that pane's tty in a
        # variable; writing there would put this agent on that pane's card.
        with tempfile.TemporaryDirectory() as temp:
            stale = Path(temp) / "other-pane"
            env = {**self.env, "WEZMUX_TTY": str(stale)}
            for hook in HOOKS:
                with self.subTest(hook=hook.name):
                    subprocess.run([sys.executable, "-c", AGENT, str(hook), PAYLOAD],
                                   env=env, start_new_session=True, timeout=10,
                                   stdin=subprocess.DEVNULL, capture_output=True)
                    self.assertFalse(stale.exists())


class CodexDaemonHookTests(unittest.TestCase):
    """The daemon runs every Codex TUI's hooks; route them by thread."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name).resolve()
        self.project = self.root / "project"
        self.project.mkdir()
        bin_dir = self.root / "bin"
        bin_dir.mkdir()
        self.daemon = bin_dir / "codex"
        self.daemon.symlink_to("/bin/bash")
        self.tui = self.root / "tui" / "codex"
        self.tui.parent.mkdir()
        self.tui.symlink_to("/bin/sleep")
        db = sqlite3.connect(self.root / "state_5.sqlite")
        db.execute("CREATE TABLE threads (id TEXT PRIMARY KEY, originator TEXT)")
        db.executemany("INSERT INTO threads VALUES (?, ?)",
                       [("tui-thread", "codex-tui"), ("exec-thread", "codex_exec")])
        db.commit()
        db.close()
        self.env = {k: v for k, v in os.environ.items() if k != "WEZMUX_HOOK_TTY"}
        self.env["CODEX_HOME"] = str(self.root)
        self.tuis = []

    def tearDown(self):
        for pid, master in self.tuis:
            os.kill(pid, 9)
            os.waitpid(pid, 0)
            os.close(master)
        self.temp.cleanup()

    def start_tui(self):
        pid, master = pty.fork()
        if pid == 0:
            try:
                os.chdir(self.project)
                os.execv(self.tui, [str(self.tui), "30"])
            finally:
                os._exit(127)
        self.tuis.append((pid, master))
        return master

    def run_hook(self, thread):
        payload = json.dumps({"session_id": thread, "cwd": str(self.project),
                              "last_assistant_message": "Done"})
        subprocess.run([str(self.daemon), "-c", DAEMON, str(BIN / "codex/on-stop.sh"),
                        payload, "app-server"],
                       env=self.env, start_new_session=True, timeout=10,
                       stdin=subprocess.DEVNULL, capture_output=True)

    def read(self, master):
        output = b""
        while select.select([master], [], [], 1)[0]:
            output += os.read(master, 65536)
        return output

    def test_hook_reaches_the_threads_tui(self):
        master = self.start_tui()
        self.run_hook("tui-thread")
        self.assertIn(b"\x1b]7777;status;idle", self.read(master))

    def test_non_tui_thread_reports_nothing(self):
        master = self.start_tui()
        self.run_hook("exec-thread")
        self.assertEqual(self.read(master), b"")

    def test_two_tuis_in_the_directory_report_nothing(self):
        masters = [self.start_tui(), self.start_tui()]
        self.run_hook("tui-thread")
        for master in masters:
            self.assertEqual(self.read(master), b"")


class ClaudeStatusHookTests(unittest.TestCase):
    """The status each Claude hook reports, sent to a pty via WEZMUX_HOOK_TTY.

    A pty rather than a file: each write reopens the destination with `>`,
    which truncates a file and would keep only the last sequence."""

    def run_hook(self, hook, payload):
        master, slave = pty.openpty()
        try:
            subprocess.run([str(BIN / hook)], input=json.dumps(payload), text=True,
                           env={**os.environ, "WEZMUX_HOOK_TTY": os.ttyname(slave)},
                           timeout=10)
            data = b""
            while select.select([master], [], [], 0.2)[0]:
                data += os.read(master, 65536)
        finally:
            os.close(master)
            os.close(slave)
        data = data.decode()
        statuses = [s.split("\x07")[0] for s in data.split("\x1b]7777;status;")[1:]]
        return statuses, data

    def notification(self, notification_type, message):
        payload = {"hook_event_name": "Notification", "message": message}
        if notification_type is not None:
            payload["notification_type"] = notification_type
        return self.run_hook("on-notification.sh", payload)

    def test_idle_prompt_reports_idle_not_needs_input(self):
        statuses, data = self.notification("idle_prompt", "Claude is waiting for your input")
        self.assertEqual(statuses, ["idle"])
        # The generic text must not displace the last turn's preview.
        self.assertNotIn("\x1b]7777;message;", data)

    def test_prompts_for_input_report_needs_input(self):
        for kind, message in [
            ("permission_prompt", "Claude needs your permission to use Bash"),
            ("elicitation_dialog", "MCP server needs input"),
            ("agent_needs_input", "Teammate needs your input"),
        ]:
            with self.subTest(kind=kind):
                self.assertEqual(self.notification(kind, message)[0], ["needs_input"])

    def test_informational_notifications_leave_status_alone(self):
        for kind in ["auth_success", "agent_completed", "push_notification"]:
            with self.subTest(kind=kind):
                self.assertEqual(self.notification(kind, "Waiting for nothing")[0], [])

    def test_untyped_notifications_fall_back_to_the_text(self):
        self.assertEqual(self.notification(None, "Claude is waiting for your input")[0], [])
        self.assertEqual(
            self.notification(None, "Claude needs your permission to use Bash")[0],
            ["needs_input"],
        )

    def test_tool_batch_reports_working(self):
        self.assertEqual(self.run_hook("on-tool-batch.sh", {})[0], ["working"])

    def test_stop_failure_reports_idle(self):
        statuses, data = self.run_hook("on-stop-failure.sh", {"error": "rate_limit"})
        self.assertEqual(statuses, ["idle"])
        self.assertIn("rate_limit", data)


if __name__ == "__main__":
    unittest.main()
