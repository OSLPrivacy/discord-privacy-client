from __future__ import annotations

import contextlib
import io
import shutil
import tempfile
import unittest
from pathlib import Path

from scripts import check_osl_audit_live_setup_waits as checker


FIXTURE = """\
TASK 0004 - log in to the Windows test computers
gates: -
build: decision
who: liam
run by: liam
do: Sign in to each existing Windows test computer.
done when: each listed computer has a successful desktop capture.

TASK 0033 - save the two-copy test command
gates: -
build: test
who: agent
run by: codex fast
do: Put the fast two-copy command and its required switches in the test guide.
done when: a new shell gets two live status results.

TASK 1201 - make the website driver use a real browser
gates: -
build: backend
who: agent
run by: claude
do: Add the real browser connection behind the website driver instead of the fake test browser.
done when: a direct driver command opens a local test page and reads its title.

TASK 2000 - check one real account send
gates: 0004, 0033
build: test
who: liam
run by: liam
do: With two real Discord accounts on two machines, send one random marked protected message.
done when: the run names the two signed-in accounts it used.

TASK 2001 - place text in a browser page through the shared job
gates: 1201
build: test
who: agent
run by: codex build
do: Place a random marked message in an email compose box in a real browser using 3406.
done when: the browser reports a real driver and the text is placed.

TASK 2002 - run a two-machine private message
gates: 0004, 0033
build: test
who: agent
run by: codex build
do: With two real Signal accounts on two machines, send one private message.
done when: both people read the marked text.

TASK 2003 - fixture signed-in wording is not live setup
gates: -
build: backend
who: agent
run by: codex fast
do: A signed-in account fixture renders the local owner row.
done when: the fixture passes.
"""


class LiveSetupWaitsTests(unittest.TestCase):
    def write_fixture(self, text: str = FIXTURE) -> Path:
        directory = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, directory)
        todo = directory / "todo"
        todo.mkdir()
        (todo / "01-foundation.txt").write_text(text, encoding="utf-8")
        return todo

    def test_fixture_passes_all_three_setup_paths(self) -> None:
        tasks = checker.parse_tasks(self.write_fixture())
        checked, failures = checker.validate(tasks)
        self.assertEqual(failures, [])
        self.assertEqual(checked, {"real-account": 2, "real-browser": 1, "two-machine": 2})

    def test_removed_known_real_browser_wait_names_the_task(self) -> None:
        todo = self.write_fixture(FIXTURE.replace("gates: 1201\nbuild: test", "gates: -\nbuild: test"))
        stdout = io.StringIO()
        with contextlib.redirect_stdout(stdout):
            code = checker.main(["--todo-dir", str(todo)])
        self.assertEqual(code, 1)
        self.assertIn("1 real-browser tasks lack a path to 1201", stdout.getvalue())
        self.assertIn("TASK 2001 lacks path to 1201", stdout.getvalue())


if __name__ == "__main__":
    unittest.main()
