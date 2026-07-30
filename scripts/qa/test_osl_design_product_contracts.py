from __future__ import annotations

import re
import unittest
from collections.abc import Callable
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
GUI_PLAN = ROOT / "docs" / "design" / "osl-gui-final-plan.md"
SIMPLE_SPEC = ROOT / "docs" / "design" / "osl-simple-spec.md"
SUBJECTIVE_DESIGN_FEEL = ROOT / "docs" / "design" / "osl-subjective-design-feel.md"


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


def _unordered_items(section: str) -> list[str]:
    items: list[str] = []
    current: list[str] | None = None
    for line in section.splitlines():
        if line.startswith("- "):
            if current is not None:
                items.append(" ".join(current))
            current = [line.removeprefix("- ").strip()]
            continue
        if current is not None and line.startswith("  "):
            current.append(line.strip())
    if current is not None:
        items.append(" ".join(current))
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


def _errors_for_subjective_design_feel(markdown: str) -> list[str]:
    complexity = _section(markdown, "## Complexity belongs behind the product")
    frozen = _section(markdown, "## Frozen user-facing contract")
    normalized_complexity = re.sub(r"\s+", " ", complexity)
    lower_complexity = normalized_complexity.lower()
    errors: list[str] = []

    product_match = re.search(
        r"user-facing model is (.+?)\.", normalized_complexity, flags=re.IGNORECASE
    )
    product_nouns = set()
    if product_match:
        product_nouns = {
            item.strip()
            for item in re.split(
                r",\s+| and ",
                product_match.group(1),
            )
            if item.strip()
        }
    expected_product_nouns = {
        "protection state",
        "trusted people",
        "connected accounts",
        "private conversations",
        "cleanup actions",
        "activity history",
    }
    if product_nouns != expected_product_nouns:
        errors.append("product mental model is not the frozen six nouns")

    machinery_match = re.search(
        r"Engineering concepts such as (.+?)\s+belong behind",
        normalized_complexity,
        flags=re.IGNORECASE,
    )
    implementation_terms = set()
    if machinery_match:
        implementation_terms = {
            item.strip().removeprefix("or ")
            for item in re.split(r",\s+|\s+or\s+", machinery_match.group(1))
            if item.strip()
        }
    expected_implementation_terms = {
        "keyservers",
        "ratchets",
        "receipts",
        "browser profiles",
        "provider adapters",
    }
    if implementation_terms != expected_implementation_terms:
        errors.append("implementation machinery is not fully hidden")

    hidden_context_sentence = next(
        (
            sentence.lower()
            for sentence in _sentences(complexity)
            if sentence.startswith("They must not appear")
        ),
        "",
    )
    for required in (
        "navigation",
        "onboarding choices",
        "warning labels",
        "settings names",
        "status labels",
        "user-facing concepts",
    ):
        if required not in hidden_context_sentence:
            errors.append(f"hidden machinery context missing: {required}")

    consequence_sentence = next(
        (
            sentence.lower()
            for sentence in _sentences(complexity)
            if sentence.startswith("When an implementation detail affects")
        ),
        "",
    )
    if not {"plain consequence", "safe action"}.issubset(
        set(re.findall(r"[a-z]+(?: [a-z]+)?", consequence_sentence))
    ):
        errors.append("implementation details are not translated into outcomes")
    for forbidden in (
        "protocol names",
        "storage layouts",
        "automation internals",
        "service-specific plumbing",
    ):
        if forbidden not in lower_complexity:
            errors.append(f"user is not protected from {forbidden}")

    bullets = [_plain(item) for item in _unordered_items(frozen)]
    if len(bullets) != 4:
        errors.append("frozen contract must remain four explicit commitments")
    if not bullets or not all(noun in bullets[0] for noun in expected_product_nouns):
        errors.append("frozen contract does not speak in product nouns")
    if len(bullets) < 2 or not all(
        phrase in bullets[1]
        for phrase in (
            "refuses",
            "implementation machinery",
            "decision",
            "understand",
        )
    ):
        errors.append("frozen contract does not refuse machinery-as-decision")
    if len(bullets) < 3 or not all(
        phrase in bullets[2]
        for phrase in (
            "plain consequence",
            "next safe action",
            "explicit unknown state",
        )
    ):
        errors.append("frozen contract does not preserve the unknown-state refusal")
    if len(bullets) < 4 or not all(
        phrase in bullets[3]
        for phrase in (
            "advanced exports",
            "implementation fields",
            "main ui",
            "product answer",
            "ordinary language",
        )
    ):
        errors.append("frozen contract does not keep diagnostics behind UI language")

    final_sentence = [sentence.lower() for sentence in _sentences(frozen) if sentence][-1]
    if not all(
        phrase in final_sentence
        for phrase in (
            "protocol",
            "storage",
            "automation",
            "transport",
            "service-plumbing",
            "violates the design feel",
        )
    ):
        errors.append("final contract violation rule is incomplete")
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


def encode_burns_five_guarantees_and_banned_phrases() -> None:
    markdown = SIMPLE_SPEC.read_text(encoding="utf-8")
    broken = markdown.replace(
        '8. **Honest result:** OSL reports what it actually verified.',
        '8. **Optimistic result:** OSL reports cleanup requests as success.',
    ).replace('"gone for good"', '"secure cleanup"')
    _assert_contract(_errors_for_burn_contract, markdown, broken_documents=(broken,))


encode_burns_five_guarantees_and_banned_phrases.__name__ = (
    "Encode Burn's five guarantees and banned phrases."
)


def freeze_user_facing_complexity_hiding_product_contract() -> None:
    markdown = SUBJECTIVE_DESIGN_FEEL.read_text(encoding="utf-8")
    broken = markdown.replace(
        "protection state, trusted people, connected accounts, private conversations, cleanup actions and activity history",
        "keyservers, ratchets, receipts, browser profiles, provider adapters and activity history",
        1,
    ).replace(
        "including an explicit unknown state when certainty is unavailable",
        "including implementation detail when certainty is unavailable",
        1,
    )
    _assert_contract(
        _errors_for_subjective_design_feel,
        markdown,
        broken_documents=(broken,),
    )


freeze_user_facing_complexity_hiding_product_contract.__name__ = (
    "docs/design/osl-subjective-design-feel.md"
)


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
        encode_burns_five_guarantees_and_banned_phrases,
        freeze_user_facing_complexity_hiding_product_contract,
    ):
        suite.addTest(unittest.FunctionTestCase(test))
    return suite


if __name__ == "__main__":
    unittest.main(verbosity=2)
