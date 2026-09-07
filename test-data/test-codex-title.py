"""Regression checks for Codex title hooks: python3 test-data/test-codex-title.py."""

import json
import os
from pathlib import Path
import pty
import select
import sqlite3
import subprocess
import tempfile
import unittest


HOOK_DIR = Path(__file__).resolve().parents[1] / "bin/hooks/codex"
TITLE_PREFIX = "\x1b]7777;title;"


class CodexTitleTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="wezmux-codex-title-")
        self.addCleanup(self.temp.cleanup)
        self.state_dir = Path(self.temp.name)
        self.output = self.state_dir / "terminal-output"
        self.env = {
            **os.environ,
            "CODEX_HOME": str(self.state_dir),
            "CODEX_THREAD_ID": "environment-thread",
            "WEZMUX_TTY": str(self.output),
        }

    def database(self, name=None, title="Fix workcard titles", *, legacy=False,
                 filename="state_5.sqlite", thread_id="test-thread"):
        db_path = self.state_dir / filename
        with sqlite3.connect(db_path) as db:
            if legacy:
                db.execute("CREATE TABLE threads (id TEXT PRIMARY KEY, title TEXT)")
                db.execute("INSERT INTO threads VALUES (?, ?)", (thread_id, title))
            else:
                db.execute("CREATE TABLE threads (id TEXT PRIMARY KEY, title TEXT, name TEXT)")
                db.execute("INSERT INTO threads VALUES (?, ?, ?)", (thread_id, title, name))
        return db_path

    def run_hook(self, payload=None, *, script="update-title.sh", args=None):
        if payload is None:
            payload = {"session_id": "test-thread"}
        if args is None:
            args = ["--hook", "--once"] if script == "update-title.sh" else []
        self.output.unlink(missing_ok=True)
        result = subprocess.run(
            [str(HOOK_DIR / script), *args], input=json.dumps(payload),
            env=self.env, capture_output=True, text=True, timeout=5,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "")
        self.assertEqual(result.stderr, "")
        return self.output.read_text() if self.output.exists() else ""

    def assert_title(self, output, title):
        self.assertEqual(output, f"{TITLE_PREFIX}{title}\x07")

    def test_saved_name_wins_over_first_and_followup_prompts(self):
        self.database(name="Réparer les titres; Codex")
        self.assert_title(self.run_hook({"session_id": "test-thread", "prompt": "yes"}),
                          "Réparer les titres; Codex")

    def test_unnamed_session_keeps_its_first_prompt_on_resume_and_followup(self):
        self.database()
        self.assert_title(self.run_hook(), "Fix workcard titles")
        self.assert_title(self.run_hook({"session_id": "test-thread", "prompt": "yes"}),
                          "Fix workcard titles")

    def test_new_session_uses_sanitized_bounded_unicode_prompt_without_database(self):
        prompt = "  Fix\n\t\r" + "é" * 100 + "\x1b\x07\x9c"
        output = self.run_hook({"session_id": "test-thread", "prompt": prompt})
        self.assert_title(output, "Fix " + "é" * 73 + "...")
        self.assert_title(self.run_hook({"session_id": "test-thread", "prompt": " A\x1b\x07\x9cB\tC "}),
                          "A B C")

    def test_legacy_database_without_name_column(self):
        self.database(legacy=True)
        self.assert_title(self.run_hook(), "Fix workcard titles")

    def test_name_in_later_database_wins_over_earlier_fallback(self):
        self.database(legacy=True, filename="state_4.sqlite")
        self.database(name="Saved short name")
        self.assert_title(self.run_hook(), "Saved short name")

    def test_missing_or_corrupt_state_is_quiet_and_does_not_create_database(self):
        self.assertEqual(self.run_hook(), "")
        self.assertEqual(list(self.state_dir.glob("*.sqlite")), [])
        (self.state_dir / "state_5.sqlite").write_text("invalid database")
        self.assertEqual(self.run_hook(), "")
        self.assert_title(self.run_hook({"session_id": "test-thread", "prompt": "Fix titles"}),
                          "Fix titles")

    def test_invalid_thread_id_is_ignored(self):
        self.database()
        self.assertEqual(self.run_hook({"session_id": "' OR 1=1 --", "prompt": "oops"}), "")

    def test_environment_id_and_positional_interface(self):
        self.database(thread_id="environment-thread")
        self.assert_title(self.run_hook({}), "Fix workcard titles")
        self.assert_title(self.run_hook(args=["environment-thread", str(self.output)]),
                          "Fix workcard titles")

    def test_tool_and_stop_hooks_refresh_unnamed_title(self):
        self.database()
        for script, extra, expected in [
            ("on-pre-tool-use.sh", {"tool_name": "Bash"}, "\x1b]7777;tool;Bash\x07"),
            ("on-stop.sh", {"last_assistant_message": "Done"}, "\x1b]7777;status;idle\x07"),
        ]:
            with self.subTest(script=script):
                # A pseudo-TTY preserves all writes from the lifecycle hook.
                output = self.capture_tty_hook(script, {"session_id": "test-thread", **extra})
                self.assertIn(f"{TITLE_PREFIX}Fix workcard titles\x07", output)
                self.assertIn(expected, output)

    def capture_tty_hook(self, script, payload):
        master, slave = pty.openpty()
        try:
            result = subprocess.run(
                [str(HOOK_DIR / script)], input=json.dumps(payload), text=True,
                env={**self.env, "WEZMUX_TTY": os.ttyname(slave)},
                capture_output=True, timeout=5,
            )
            self.assertEqual((result.returncode, result.stdout, result.stderr), (0, "", ""))
            self.assertTrue(select.select([master], [], [], 1)[0], "No terminal events")
            return os.read(master, 65536).decode()
        finally:
            os.close(master)
            os.close(slave)

    def test_background_hook_emits_fallback_before_late_name(self):
        db_path = self.database()
        master, slave = pty.openpty()
        proc = subprocess.Popen(
            [str(HOOK_DIR / "update-title.sh"), "--hook"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            env={**self.env, "WEZMUX_TTY": os.ttyname(slave)}, text=True,
        )
        try:
            proc.stdin.write(json.dumps({"session_id": "test-thread", "prompt": "yes"}))
            proc.stdin.close()
            self.assertTrue(select.select([master], [], [], 3)[0], "Fallback was not immediate")
            self.assert_title(os.read(master, 65536).decode(), "Fix workcard titles")
            # Let one poll run with no name: it must not resend the same fallback.
            self.assertFalse(select.select([master], [], [], 1.2)[0], "Duplicate fallback")
            with sqlite3.connect(db_path) as db:
                db.execute("UPDATE threads SET name = 'Reliable Codex titles'")
            self.assertTrue(select.select([master], [], [], 3)[0], "Late name was not emitted")
            self.assert_title(os.read(master, 65536).decode(), "Reliable Codex titles")
            self.assertEqual(proc.wait(timeout=3), 0)
            self.assertEqual(proc.stdout.read(), "")
            self.assertEqual(proc.stderr.read(), "")
        finally:
            if proc.poll() is None:
                proc.kill()
            proc.wait()
            proc.stdout.close()
            proc.stderr.close()
            os.close(master)
            os.close(slave)


if __name__ == "__main__":
    unittest.main()
