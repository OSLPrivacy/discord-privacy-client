from __future__ import annotations

import re
import unittest
from collections.abc import Callable
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
GUI_PLAN = ROOT / "docs" / "design" / "osl-gui-final-plan.md"
SIMPLE_SPEC = ROOT / "docs" / "design" / "osl-simple-spec.md"


def _section(markdown: str, heading: str) -> str:
    marker = f"\n{heading}\n"
    try:
        body = markdown.split(marker, maxsplit=1)[1]
    except IndexError as exc:
        raise AssertionError(f"{heading} section is missing") from exc
    level = len(heading) - len(heading.lstrip("#"))
    next_heading = re.search(rf"\n#{{1,{level}}} ", body)
    return body[: next_heading.start()] if next_heading else body


def _table_rows(section: str, expected_header: str) -> list[dict[str, str]]:
    lines = [line for line in section.splitlines() if line.startswith("|")]
    header_index = next(
        index for index, line in enumerate(lines) if expected_header in line
    )
    headers = [cell.strip() for cell in lines[header_index].strip("|").split("|")]
    rows: list[dict[str, str]] = []
    for line in lines[header_index + 2 :]:
        cells = [cell.strip() for cell in line.strip("|").split("|")]
        if len(cells) != len(headers):
            break
        rows.append(dict(zip(headers, cells, strict=True)))
    return rows


def _ordered_items(section: str) -> list[tuple[str, str]]:
    items: list[tuple[str, str]] = []
    current_label: str | None = None
    current_body: list[str] = []
    for line in section.splitlines():
        match = re.match(r"^\d+\.\s+\*\*(.+?):\*\*\s+(.*)$", line)
        if match:
            if current_label is not None:
                items.append((current_label, " ".join(current_body)))
            current_label = match.group(1)
            current_body = [match.group(2).strip()]
            continue
        if current_label and line.startswith("   "):
            current_body.append(line.strip())
    if current_label is not None:
        items.append((current_label, " ".join(current_body)))
    return items


def _sentences(markdown: str) -> list[str]:
    normalized = re.sub(r"\s+", " ", markdown)
    return [sentence.strip() for sentence in re.split(r"(?<=[.!?])\s+", normalized)]


def _plain(markdown: str) -> str:
    return re.sub(r"\s+", " ", markdown).lower()


def _errors_for_information_architecture(markdown: str) -> list[str]:
    section = _section(markdown, "## Information architecture")
    rows = _table_rows(section, "Destination")
    destinations = [row["Destination"].strip("*") for row in rows]
    errors: list[str] = []
    expected = ["Home", "Inbox", "People", "Privacy", "Activity", "Connections"]
    if destinations != expected:
        errors.append("primary destinations are not exactly the fixed six")
    if "Settings" in destinations:
        errors.append("Settings is a competing destination")
    if any(not row["User's question"] or not row["Main content"] or not row["Primary action"] for row in rows):
        errors.append("destination rows must encode question, content, and action")
    settings_sentences = [
        sentence.lower() for sentence in _sentences(section) if sentence.startswith("Settings ")
    ]
    if not any(
        {"fixed", "bottom", "seventh"}.issubset(set(re.findall(r"[a-z]+", sentence)))
        for sentence in settings_sentences
    ):
        errors.append("Settings is not fixed at the bottom outside the six")
    return errors


def _errors_for_send_contract(simple_markdown: str, gui_markdown: str) -> list[str]:
    sending = _section(simple_markdown, "## Sending")
    lower_sending = _plain(sending)
    errors: list[str] = []
    outcome_match = re.search(r"outcomes:\s+(.+?)\.", sending)
    outcomes = set()
    if outcome_match:
        outcomes = {
            item.strip()
            for item in re.split(r",\s+or\s+|,\s+| or ", outcome_match.group(1))
        }
    if outcomes != {"sent", "not sent", "delivery uncertain"}:
        errors.append("sending outcomes are not the honest tri-state")
    for required in (
        "never treats delivery uncertain as sent",
        "never auto-retries",
        "never asks the user to resend as if the first attempt certainly failed",
    ):
        if required not in lower_sending:
            errors.append(f"missing sending refusal: {required}")
    for required in (
        "first distinct user enter",
        "second separate user enter after key-up",
        "key repeat",
        "synthetic event",
        "preserves the local draft",
        "never retries automatically",
    ):
        if required not in lower_sending:
            errors.append(f"missing simple Double Enter rule: {required}")

    double_enter = _section(gui_markdown, "### Double Enter state machine")
    transitions = [
        line.strip().removeprefix("-> ").lower()
        for line in double_enter.splitlines()
        if line.strip().startswith("-> ")
    ]
    required_transitions = [
        "first user enter",
        "verify focus + service + account + conversation + recipients + mode",
        "encrypt and place capsule",
        "awaiting second user enter",
        "reverify the same context",
        "second distinct user enter passes through to native send",
        "verify outcome, or report unknown",
    ]
    position = -1
    for transition in required_transitions:
        try:
            next_position = transitions.index(transition, position + 1)
        except ValueError:
            errors.append(f"missing Double Enter transition: {transition}")
            continue
        position = next_position
    lower_double = _plain(double_enter)
    for required in (
        "first enter is consumed",
        "cannot send the plaintext",
        "second enter must be a separate, trusted user key press after key-up",
        "key repeat",
        "synthetic event",
        "expiry returns to a safe draft state; it does not send",
        "mismatch cancels",
        "crash recovery may restore the draft, never the armed-to-send state",
        "never retries automatically",
    ):
        if required not in lower_double:
            errors.append(f"missing GUI Double Enter refusal: {required}")
    return errors


def _errors_for_burn_contract(markdown: str) -> list[str]:
    burn = _section(markdown, "## Burn")
    lower_burn = _plain(burn)
    errors: list[str] = []
    items = _ordered_items(burn)
    labels = [label for label, _body in items]
    if labels != [
        "Local cleanup",
        "OSL server cleanup",
        "Cooperative peer request",
        "Exact scope",
        "Honest result",
    ]:
        errors.append("Burn guarantees are not exactly the five named guarantees")
    item_text = {label: body.lower() for label, body in items}
    required_by_label = {
        "Local cleanup": ("shreds", "local stored ciphertext", "nonce", "cached attachments"),
        "OSL server cleanup": ("server-side state", "selected osl scope"),
        "Cooperative peer request": ("absence of consent", "binding", "authority", "refused", "unavailable"),
        "Exact scope": ("reviewed and confirmed scope", "requires confirmation again"),
        "Honest result": ("actually verified", "never displayed as deletion", "unsupported", "unverified"),
    }
    for label, required_terms in required_by_label.items():
        body = item_text.get(label, "")
        if not all(term in body for term in required_terms):
            errors.append(f"Burn guarantee is incomplete: {label}")
    if "burn does not un-send" not in lower_burn:
        errors.append("Burn must not claim unsend")
    for required in (
        "does not delete carrier messages",
        "does not erase provider retention",
        "screenshots",
        "already-read plaintext",
        "can remain readable",
    ):
        if required not in lower_burn:
            errors.append(f"Burn limitation is missing: {required}")
    banned = set(re.findall(r'"([^"]+)"', burn))
    expected_banned = {
        "cryptographic burn",
        "destroys keys, not messages",
        "permanent ciphertext",
        "permanent gibberish",
        "mathematically opaque",
        "disappears forever",
        "permanently undecryptable",
        "gone for good",
    }
    if banned != expected_banned:
        errors.append("Burn banned phrases changed")
    return errors


def _errors_for_browser_and_monetization(markdown: str) -> list[str]:
    connections = _section(markdown, "## Connections")
    errors: list[str] = []
    receipt_sentence = next(
        (
            sentence
            for sentence in _sentences(connections)
            if "completed import-source receipt" in sentence
        ),
        "",
    )
    receipt_choices = re.findall(r"`([^`]+)`", receipt_sentence)
    if receipt_choices != ["Browser account", "New account"]:
        errors.append("browser import receipt path must expose exactly two choices")
    without_receipt = next(
        (sentence.lower() for sentence in _sentences(connections) if "without one" in sentence),
        "",
    )
    if not {"fixed", "official", "sign-in", "origin", "directly"}.issubset(
        set(re.findall(r"[a-z-]+", without_receipt))
    ):
        errors.append("missing direct fixed-origin behavior without import receipt")
    new_account = next(
        (sentence.lower() for sentence in _sentences(connections) if sentence.startswith("`New account`")),
        "",
    )
    if not {"fixed", "owner-scoped", "osl", "browser", "profile"}.issubset(
        set(re.findall(r"[a-z-]+", new_account))
    ):
        errors.append("New account is not owner-scoped to an OSL browser profile")
    renderer_sentence = next(
        (sentence.lower() for sentence in _sentences(connections) if "renderer-provided" in sentence),
        "",
    )
    if not {"executable", "url", "profile", "path", "browser", "argument"}.issubset(
        set(re.findall(r"[a-z]+", renderer_sentence))
    ):
        errors.append("renderer-provided browser launch authority is not refused")

    monetization_sentences = [
        sentence.lower()
        for sentence in _sentences(markdown)
        if "monetization labels" in sentence.lower()
        or "free and paid states" in sentence.lower()
    ]
    if not any(
        "never interrupt" in sentence
        and "safety warning" in sentence
        and "destructive confirmation" in sentence
        and "capability refusal" in sentence
        for sentence in monetization_sentences
    ):
        errors.append("monetization can interrupt safety or capability refusal")
    return errors


def _assert_contract(
    validate: Callable[..., list[str]],
    *documents: str,
    broken_documents: tuple[str, ...],
) -> None:
    testcase = unittest.TestCase()
    testcase.assertEqual(validate(*documents), [])
    testcase.assertNotEqual(validate(*broken_documents), [])


def encode_the_six_fixed_information_architecture_destinations() -> None:
    markdown = GUI_PLAN.read_text(encoding="utf-8")
    broken = markdown.replace("| **Activity** | What did OSL actually do?", "| **Settings** | What did OSL actually do?")
    _assert_contract(_errors_for_information_architecture, markdown, broken_documents=(broken,))


encode_the_six_fixed_information_architecture_destinations.__name__ = (
    "Encode the six fixed information-architecture destinations."
)


def encode_honest_tri_state_sending_and_double_enter_without_auto_retry() -> None:
    simple = SIMPLE_SPEC.read_text(encoding="utf-8")
    gui = GUI_PLAN.read_text(encoding="utf-8")
    broken_simple = simple.replace("delivery uncertain", "failed", 1).replace(
        "never auto-retries it, ", ""
    )
    _assert_contract(
        _errors_for_send_contract,
        simple,
        gui,
        broken_documents=(broken_simple, gui),
    )
    broken_gui = gui.replace("-> Second distinct user Enter passes through to native Send", "-> OSL synthetic Enter sends")
    testcase = unittest.TestCase()
    testcase.assertNotEqual(_errors_for_send_contract(simple, broken_gui), [])


encode_honest_tri_state_sending_and_double_enter_without_auto_retry.__name__ = (
    "Encode honest tri-state sending and Double Enter without auto-retry."
)


def encode_browser_import_choices_and_noninterrupting_monetization() -> None:
    markdown = GUI_PLAN.read_text(encoding="utf-8")
    broken = markdown.replace(
        "a web app shows only `Browser account` and `New account`",
        "a web app shows `Browser account`, `Existing profile` and `New account`",
    ).replace("must never interrupt", "may interrupt", 1)
    _assert_contract(_errors_for_browser_and_monetization, markdown, broken_documents=(broken,))


encode_browser_import_choices_and_noninterrupting_monetization.__name__ = (
    "Encode browser import choices and noninterrupting monetization."
)


def gui_final_plan_document_contract() -> None:
    markdown = GUI_PLAN.read_text(encoding="utf-8")
    testcase = unittest.TestCase()
    testcase.assertEqual(_errors_for_information_architecture(markdown), [])
    testcase.assertEqual(_errors_for_browser_and_monetization(markdown), [])

    settings_destination = markdown.replace(
        "| **Activity** | What did OSL actually do?",
        "| **Settings** | What did OSL actually do?",
    )
    testcase.assertIn(
        "primary destinations are not exactly the fixed six",
        _errors_for_information_architecture(settings_destination),
    )

    missing_fixed_settings = markdown.replace(
        "Settings remains a fixed item at the bottom of the sidebar rather than a seventh competing destination.",
        "Settings is available from the sidebar.",
    )
    testcase.assertIn(
        "Settings is not fixed at the bottom outside the six",
        _errors_for_information_architecture(missing_fixed_settings),
    )

    widened_import_choices = markdown.replace(
        "a web app shows only `Browser account` and `New account`",
        "a web app shows `Browser account`, `Existing profile` and `New account`",
    )
    testcase.assertIn(
        "browser import receipt path must expose exactly two choices",
        _errors_for_browser_and_monetization(widened_import_choices),
    )

    permissive_renderer_browser = markdown.replace(
        "OSL never accepts a renderer-provided executable, URL, profile path or browser argument",
        "OSL may accept a renderer-provided URL",
    )
    testcase.assertIn(
        "renderer-provided browser launch authority is not refused",
        _errors_for_browser_and_monetization(permissive_renderer_browser),
    )

    interrupting_paid_state = markdown.replace("must never interrupt", "may interrupt", 1)
    testcase.assertIn(
        "monetization can interrupt safety or capability refusal",
        _errors_for_browser_and_monetization(interrupting_paid_state),
    )


gui_final_plan_document_contract.__name__ = "docs/design/osl-gui-final-plan.md'"


def encode_burns_five_guarantees_and_banned_phrases() -> None:
    markdown = SIMPLE_SPEC.read_text(encoding="utf-8")
    broken = markdown.replace(
        '5. **Honest result:** OSL reports what it actually verified.',
        '5. **Optimistic result:** OSL reports cleanup requests as success.',
    ).replace('"gone for good"', '"secure cleanup"')
    _assert_contract(_errors_for_burn_contract, markdown, broken_documents=(broken,))


encode_burns_five_guarantees_and_banned_phrases.__name__ = (
    "Encode Burn's five guarantees and banned phrases."
)


def simple_spec_burn_contract() -> None:
    markdown = SIMPLE_SPEC.read_text(encoding="utf-8")
    gui = GUI_PLAN.read_text(encoding="utf-8")
    testcase = unittest.TestCase()
    testcase.assertEqual(_errors_for_send_contract(markdown, gui), [])
    testcase.assertEqual(_errors_for_burn_contract(markdown), [])

    optimistic_send = markdown.replace(
        "Sending has three honest outcomes: sent, not sent, or delivery uncertain.",
        "Sending has two outcomes: sent or failed.",
    )
    testcase.assertIn(
        "sending outcomes are not the honest tri-state",
        _errors_for_send_contract(optimistic_send, gui),
    )

    auto_retry = markdown.replace("never auto-retries it, ", "")
    testcase.assertIn(
        "missing sending refusal: never auto-retries",
        _errors_for_send_contract(auto_retry, gui),
    )

    optimistic_uncertain_delivery = markdown.replace(
        "never treats delivery uncertain as sent",
        "treats delivery uncertain as sent",
    )
    testcase.assertIn(
        "missing sending refusal: never treats delivery uncertain as sent",
        _errors_for_send_contract(optimistic_uncertain_delivery, gui),
    )

    synthetic_second_enter = markdown.replace("synthetic event", "automation event")
    testcase.assertIn(
        "missing simple Double Enter rule: synthetic event",
        _errors_for_send_contract(synthetic_second_enter, gui),
    )

    missing_peer_refusal = markdown.replace(
        "absence of consent, binding, authority, transport delivery or\n   verification means the peer cleanup is refused or reported as unavailable.",
        "peer cleanup is attempted whenever transport delivery is available.",
    )
    testcase.assertIn(
        "Burn guarantee is incomplete: Cooperative peer request",
        _errors_for_burn_contract(missing_peer_refusal),
    )

    optimistic_result = markdown.replace(
        "A cleanup request\n   is never displayed as deletion",
        "A cleanup request\n   is displayed as deletion",
    )
    testcase.assertIn(
        "Burn guarantee is incomplete: Honest result",
        _errors_for_burn_contract(optimistic_result),
    )

    missing_ban = markdown.replace('"permanent ciphertext"', '"durable cleanup"')
    testcase.assertIn("Burn banned phrases changed", _errors_for_burn_contract(missing_ban))


simple_spec_burn_contract.__name__ = "docs/design/osl-simple-spec.md'"


def load_tests(
    loader: unittest.TestLoader,
    tests: unittest.TestSuite,
    pattern: str | None,
) -> unittest.TestSuite:
    del loader, tests, pattern
    suite = unittest.TestSuite()
    for test in (
        encode_the_six_fixed_information_architecture_destinations,
        encode_honest_tri_state_sending_and_double_enter_without_auto_retry,
        encode_browser_import_choices_and_noninterrupting_monetization,
        gui_final_plan_document_contract,
        encode_burns_five_guarantees_and_banned_phrases,
        simple_spec_burn_contract,
    ):
        suite.addTest(unittest.FunctionTestCase(test))
    return suite


if __name__ == "__main__":
    unittest.main(verbosity=2)
