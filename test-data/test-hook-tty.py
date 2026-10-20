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


if __name__ == "__main__":
    unittest.main()
