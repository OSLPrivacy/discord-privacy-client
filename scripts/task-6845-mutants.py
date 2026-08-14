#!/usr/bin/env python3
"""TASK 6845 — the throwaway copies and the mutations applied to them.

The 6844 check proves that a real Windows capture of an open view-once viewer
notifies the sender exactly once and that nothing else notifies anyone. This
file is the other half of that claim: it takes the *shipping* source, copies it
somewhere disposable, breaks one property in the copy, and hands the copy to
the same check.

Nothing here ever writes to the working tree. Every mutation lands in a copy
under a caller-supplied directory, and `task-6845-mutation-proof.sh` deletes
each copy as soon as its check has run.

    task-6845-mutants.py list
    task-6845-mutants.py copy  --dest <dir>
    task-6845-mutants.py apply --name <mutation> --root <dir>

`apply` fails loudly when its anchor is not present exactly once: a mutation
that silently did not apply would run the *unmutated* check, see it pass, and
be read as "the check caught nothing", which is the one wrong answer this whole
proof exists to avoid.
"""

import argparse
import re
import shutil
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
CRATE = "crates/view-once-capture"

# Each mutation is a plausible defect in shipped code, not a harness knob:
# something a person could write on a Tuesday and not notice.
#
#   name      -> what the proof calls it
#   category  -> which of the four properties in the task it attacks
#   file      -> the shipping file it changes
#   old/new   -> exact text, replaced exactly once
#   defect    -> what the mutated code now does wrong
MUTATIONS = [
    {
        "name": "forged-event-accepted",
        "category": "authenticity",
        "file": f"{CRATE}/src/event.rs",
        "defect": "the signature is checked against the key the event itself carries, "
        "so anyone can sign their own accusation",
        "old": """    if event.viewer_public_key != *expected_viewer_key.as_bytes() {
        return Err("the event names a different viewer device key".to_owned());
    }
""",
        "new": """    // MUTANT (TASK 6845, authenticity): verify against the key the event
    // carries instead of the one the sender already holds.
    let expected_viewer_key = &PublicKey::from_bytes(event.viewer_public_key);
""",
    },
    {
        "name": "replayed-event-notifies-again",
        "category": "authenticity",
        "file": f"{CRATE}/src/notifier.rs",
        "defect": "the dedupe ledger is never persisted, so a replay after a restart "
        "notifies a second time",
        "old": """        self.ledger.notified.insert(key);
        self.flush()?;
""",
        "new": """        self.ledger.notified.insert(key);
        // MUTANT (TASK 6845, authenticity): the ledger stays in memory.
""",
    },
    {
        "name": "message-binding-not-checked",
        "category": "binding",
        "file": f"{CRATE}/src/notifier.rs",
        "defect": "an event naming another message is accepted for this one",
        "old": """            ("message", &binding.message_id, &sent.message_id),
""",
        "new": """            // MUTANT (TASK 6845, binding): the message the event names is
            // no longer compared with the message that was sent.
""",
    },
    {
        "name": "viewer-binding-not-checked",
        "category": "binding",
        "file": f"{CRATE}/src/notifier.rs",
        "defect": "an event naming another viewer or another viewer device is accepted",
        "old": """            (
                "viewer",
                &binding.viewer_osl_user_id,
                &sent.viewer_osl_user_id,
            ),
            (
                "viewer device",
                &binding.viewer_device_id,
                &sent.viewer_device_id,
            ),
""",
        "new": """            // MUTANT (TASK 6845, binding): the viewer and the viewer device
            // the event names are no longer compared with the ones the
            // message was sent to.
""",
    },
    {
        "name": "outbox-not-durable",
        "category": "delivery",
        "file": f"{CRATE}/src/outbox.rs",
        "defect": "the outbox is memory-only, so an event queued while the sender is "
        "unreachable does not survive the restart that would deliver it",
        "old": """        let bytes = serde_json::to_vec_pretty(&self.records)
            .map_err(|error| format!("outbox cannot be encoded: {error}"))?;
        let temp = self.path.with_extension("json.tmp");
        fs::write(&temp, &bytes).map_err(|error| format!("outbox write: {error}"))?;
        fs::rename(&temp, &self.path).map_err(|error| format!("outbox rename: {error}"))?;
        Ok(())
""",
        "new": """        // MUTANT (TASK 6845, delivery): nothing is written, so the queue
        // lives only as long as the process that filled it.
        Ok(())
""",
    },
    {
        "name": "offline-event-dropped",
        "category": "delivery",
        "file": f"{CRATE}/src/outbox.rs",
        "defect": "an event that cannot be delivered immediately is dropped instead of queued",
        "old": """        self.records.push(OutboxRecord {
            event,
            attempts: 0,
            acknowledged: false,
        });
        self.flush()
            .map_err(|error| format!("the capture event was not persisted: {error}"))?;
        Ok(true)
""",
        "new": """        // MUTANT (TASK 6845, delivery): the event is thrown away rather than
        // queued for the reconnect.
        let _ = event;
        Ok(true)
""",
    },
    {
        "name": "viewer-copy-claims-universal-detection",
        "category": "honesty",
        "file": f"{CRATE}/src/disclosure.rs",
        "defect": "the viewer is promised that every screenshot is detected",
        "pattern": r'pub const CAPTURE_DISCLOSURE_VIEWER: &str = "(?:[^"\\]|\\.)*";',
        "new": "// MUTANT (TASK 6845, honesty): the limitation is replaced with a promise\n"
        "// of universal detection.\n"
        'pub const CAPTURE_DISCLOSURE_VIEWER: &str = "\\\n'
        "Before you open this: OSL detects all screenshots of view-once media, so \\\n"
        'the sender is always notified if you capture this.";',
    },
    {
        "name": "sender-copy-claims-universal-detection",
        "category": "honesty",
        "file": f"{CRATE}/src/disclosure.rs",
        "defect": "the sender is promised that every screenshot is detected",
        "pattern": r'pub const CAPTURE_DISCLOSURE_SENDER: &str = "(?:[^"\\]|\\.)*";',
        "new": "// MUTANT (TASK 6845, honesty): the limitation is replaced with a promise\n"
        "// of universal detection.\n"
        'pub const CAPTURE_DISCLOSURE_SENDER: &str = "\\\n'
        "Before you send this: OSL detects all screenshots of view-once media, so \\\n"
        'you are always notified if this is captured.";',
    },
]

BY_NAME = {mutation["name"]: mutation for mutation in MUTATIONS}


def command_list(_arguments: argparse.Namespace) -> int:
    for mutation in MUTATIONS:
        print(f"{mutation['name']}|{mutation['category']}|{mutation['file']}|{mutation['defect']}")
    return 0


def command_copy(arguments: argparse.Namespace) -> int:
    """Make a throwaway copy of the crate that the 6844 check can be built from.

    The copy is a one-member workspace rather than the whole repository: the
    check only ever compiles this crate, and copying a multi-gigabyte tree per
    mutation would make the proof too slow to run, which is its own way of not
    running it. The workspace package and dependency tables come from the real
    root manifest, so the copy is built with the same versions the repository
    pins.
    """
    destination = Path(arguments.dest)
    if destination.exists():
        shutil.rmtree(destination)
    shutil.copytree(REPO_ROOT / CRATE, destination / CRATE)

    manifest = (REPO_ROOT / "Cargo.toml").read_text(encoding="utf-8")
    manifest, members = re.subn(
        r"members = \[.*?\]",
        f'members = ["{CRATE}"]',
        manifest,
        count=1,
        flags=re.S,
    )
    manifest, excludes = re.subn(r"exclude = \[.*?\]", "exclude = []", manifest, count=1, flags=re.S)
    if members != 1 or excludes != 1:
        print(
            "6845 the root manifest no longer has the members/exclude lists this copy rewrites",
            file=sys.stderr,
        )
        return 2
    (destination / "Cargo.toml").write_text(
        "# TASK 6845 throwaway copy. Generated by scripts/task-6845-mutants.py from\n"
        "# the repository root manifest, reduced to the one crate the 6844 check\n"
        "# compiles. Deleted as soon as the check that reads it has run.\n" + manifest,
        encoding="utf-8",
    )
    lock = REPO_ROOT / "Cargo.lock"
    if lock.exists():
        shutil.copy2(lock, destination / "Cargo.lock")

    files = sum(1 for path in destination.rglob("*") if path.is_file())
    print(f"6845 copy dest={destination} files={files}")
    return 0


def command_apply(arguments: argparse.Namespace) -> int:
    mutation = BY_NAME.get(arguments.name)
    if mutation is None:
        print(f"6845 unknown mutation {arguments.name}", file=sys.stderr)
        return 2
    target = Path(arguments.root) / mutation["file"]
    if not target.exists():
        print(f"6845 the copy has no {mutation['file']} to mutate", file=sys.stderr)
        return 2
    source = target.read_text(encoding="utf-8")

    if "pattern" in mutation:
        matches = re.findall(mutation["pattern"], source, flags=re.S)
        if len(matches) != 1:
            print(
                f"6845 mutation {mutation['name']} matched {len(matches)} sites in "
                f"{mutation['file']}; exactly 1 is required",
                file=sys.stderr,
            )
            return 2
        mutated = re.sub(mutation["pattern"], lambda _: mutation["new"], source, count=1, flags=re.S)
    else:
        occurrences = source.count(mutation["old"])
        if occurrences != 1:
            print(
                f"6845 mutation {mutation['name']} matched {occurrences} sites in "
                f"{mutation['file']}; exactly 1 is required",
                file=sys.stderr,
            )
            return 2
        mutated = source.replace(mutation["old"], mutation["new"], 1)

    if mutated == source:
        print(f"6845 mutation {mutation['name']} changed nothing", file=sys.stderr)
        return 2
    target.write_text(mutated, encoding="utf-8")
    changed = sum(1 for line in mutated.splitlines() if "MUTANT (TASK 6845" in line)
    print(
        f"6845 mutated name={mutation['name']} category={mutation['category']} "
        f"file={mutation['file']} marker_lines={changed}"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("list")
    copy_parser = sub.add_parser("copy")
    copy_parser.add_argument("--dest", required=True)
    apply_parser = sub.add_parser("apply")
    apply_parser.add_argument("--name", required=True)
    apply_parser.add_argument("--root", required=True)
    arguments = parser.parse_args()
    return {"list": command_list, "copy": command_copy, "apply": command_apply}[
        arguments.command
    ](arguments)


if __name__ == "__main__":
    raise SystemExit(main())
