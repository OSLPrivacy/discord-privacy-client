from __future__ import annotations

import contextlib
import copy
import io
import json
import tempfile
import unittest
from pathlib import Path

import check as subject


def write_contract(root: Path, value: dict | None = None) -> Path:
    path = root / "contract.json"
    contract = value if value is not None else subject.load_contract(subject.CONTRACT)
    path.write_text(json.dumps(contract), encoding="utf-8")
    return path


def cli(root: Path, contract: Path) -> tuple[int, str, str]:
    stdout, stderr = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
        code = subject.main(["--root", str(root), "--contract", str(contract)])
    return code, stdout.getvalue(), stderr.getvalue()


class MessengerContractOnlyCheck(unittest.TestCase):
    def test_repository_contract_and_four_shipping_inventories_pass(self) -> None:
        result = subject.check(subject.REPOSITORY, subject.CONTRACT)
        self.assertIn("contract_fields=176", result)
        self.assertIn("required_row_cases=21", result)
        self.assertIn("seam_rules=4", result)
        self.assertIn("production_imports=0", result)
        self.assertIn("packaged_painters=0", result)
        self.assertIn("installed_decrypted_pixels_actions=0", result)
        self.assertIn("release_manifest_rows=0", result)
        print(result)

    def test_emptying_each_contract_field_exits_one_and_names_it(self) -> None:
        original = subject.load_contract(subject.CONTRACT)
        tested = 0
        for field in ("schema", "status", "carrier", "source"):
            mutated = copy.deepcopy(original)
            mutated[field] = ""
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                code, _, error = cli(root, write_contract(root, mutated))
            self.assertEqual(code, 1)
            self.assertIn(f"contract field {field}", error)
            tested += 1
        for section, fields in subject.EXPECTED_SECTIONS.items():
            for field, value in fields.items():
                mutated = copy.deepcopy(original)
                mutated[section][field] = "" if isinstance(value, str) else None
                with tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    code, _, error = cli(root, write_contract(root, mutated))
                self.assertEqual(code, 1)
                self.assertIn(f"contract field {section}.{field}", error)
                tested += 1
        for row_index, row in enumerate(original["required_row_cases"]):
            for field, value in row.items():
                mutated = copy.deepcopy(original)
                mutated["required_row_cases"][row_index][field] = "" if isinstance(value, str) else None
                with tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    code, _, error = cli(root, write_contract(root, mutated))
                self.assertEqual(code, 1)
                if field == "id":
                    self.assertIn(f"required row case index {row_index}: field id", error)
                else:
                    self.assertIn(f"required row case {row['id']} field {field}", error)
                tested += 1
        self.assertEqual(tested, 176)
        print(f"TASK5132_BREAK emptied_contract_fields={tested} exit=1 each_named=true")

    def test_starving_each_required_row_case_exits_one_and_names_it(self) -> None:
        original = subject.load_contract(subject.CONTRACT)
        for row in original["required_row_cases"]:
            mutated = copy.deepcopy(original)
            mutated["required_row_cases"] = [item for item in mutated["required_row_cases"] if item["id"] != row["id"]]
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                code, _, error = cli(root, write_contract(root, mutated))
            self.assertEqual(code, 1)
            self.assertIn(f"required row case {row['id']}", error)
        print("TASK5132_BREAK starved_required_row_cases=21 exit=1 each_named=true")

    def test_promoting_each_component_exits_one_and_names_inventory(self) -> None:
        promotions = {
            "production_imports": (
                "apps/osl-hub/src/lib.rs",
                "pub mod messenger_eye_state;\n",
            ),
            "packaged_painters": (
                "apps/osl-hub-ui/src/promoted.ts",
                "export class MessengerDecryptedRowPainter {}\n",
            ),
            "installed_decrypted_pixels_actions": (
                "apps/osl-hub/permissions/promoted.toml",
                'identifier = "messenger-decrypted-row-action"\n',
            ),
            "release_manifest_rows": (
                "scripts/release-manifest.json",
                '{"rows":["messenger-decrypted-row"]}\n',
            ),
        }
        for inventory, (relative, contents) in promotions.items():
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(contents, encoding="utf-8")
                code, _, error = cli(root, write_contract(root))
            self.assertEqual(code, 1, inventory)
            self.assertIn(f"shipping inventory {inventory}: count=1", error)
            self.assertIn("promoted", error)
        print("TASK5132_BREAK promoted_components=4 exit=1 each_inventory_named=true")


if __name__ == "__main__":
    unittest.main()
