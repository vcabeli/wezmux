"""Regression checks for hook tty routing: python3 test-data/test-hook-tty.py."""

import json
import os
from pathlib import Path
import pty
import select
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


if __name__ == "__main__":
    unittest.main()
