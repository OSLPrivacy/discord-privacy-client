import { spawnSync } from "node:child_process";
import path from "node:path";
import { describe, expect, it } from "vitest";

const scriptPath = path.resolve("scripts/configure-telegram-operators.py");

describe("configure-telegram-operators.py", () => {
  it("telegram", () => {
    const noTerminal = spawnSync("python3", [scriptPath], {
      input: "",
      encoding: "utf8",
    });
    expect(noTerminal.status).toBe(0);
    expect(noTerminal.stdout).toContain("Live activation requires an interactive owner terminal.");
    expect(noTerminal.stdout).toContain("No Telegram or Cloudflare request was made.");
    expect(noTerminal.stderr).toBe("");

    const driver = String.raw`
import contextlib
import importlib.util
import io
import json
import sys

script_path = sys.argv[1]
spec = importlib.util.spec_from_file_location("configure_telegram_operators", script_path)
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(module)

token = "1234567890:ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghi"
webhook_secret = "owner-held-webhook-secret-proof"
telegram_calls = []
stored_secrets = []
commands_set = None
webhook_set = False
get_webhook_count = 0

class FakeStdin:
    def isatty(self):
        return True

module.sys.stdin = FakeStdin()
module.getpass.getpass = lambda prompt: token
answers = iter(["", "all", "ACTIVATE"])
module.input = lambda prompt="": next(answers)
module.secrets.token_urlsafe = lambda length: webhook_secret

def fake_put_worker_secret(name, value):
    stored_secrets.append((name, value))

def fake_telegram(seen_token, method, **fields):
    global commands_set, webhook_set, get_webhook_count
    assert seen_token == token
    telegram_calls.append((method, fields))
    if method == "getMe":
        return {"ok": True, "result": {"username": "private_owner_bot"}}
    if method == "getWebhookInfo":
        get_webhook_count += 1
        url = module.WEBHOOK_URL if webhook_set else "https://old.example.invalid/hook"
        return {"ok": True, "result": {"url": url}}
    if method == "deleteWebhook":
        assert fields == {"drop_pending_updates": "false"}
        return {"ok": True, "result": True}
    if method == "getUpdates":
        return {
            "ok": True,
            "result": [
                {"message": {"chat": {"id": 1122334455, "type": "private", "username": "alice_owner"}}},
                {"message": {"chat": {"id": 5566778899, "type": "private", "first_name": "Bob"}}},
                {"message": {"chat": {"id": -1001234567890, "type": "supergroup", "title": "Ops"}}},
            ],
        }
    if method == "setMyCommands":
        commands_set = json.loads(fields["commands"])
        assert commands_set == module.BOT_COMMANDS
        return {"ok": True, "result": True}
    if method == "getMyCommands":
        assert commands_set is not None
        return {"ok": True, "result": commands_set}
    if method == "setWebhook":
        assert fields["url"] == module.WEBHOOK_URL
        assert fields["secret_token"] == webhook_secret
        assert json.loads(fields["allowed_updates"]) == ["message"]
        assert fields["drop_pending_updates"] == "true"
        webhook_set = True
        return {"ok": True, "result": True}
    raise AssertionError(f"unexpected Telegram method: {method}")

module.put_worker_secret = fake_put_worker_secret
module.telegram = fake_telegram

stdout = io.StringIO()
with contextlib.redirect_stdout(stdout):
    module.main()

output = stdout.getvalue()
methods = [method for method, _fields in telegram_calls]
assert methods == [
    "getMe",
    "getWebhookInfo",
    "deleteWebhook",
    "getUpdates",
    "setMyCommands",
    "getMyCommands",
    "setWebhook",
    "getWebhookInfo",
], methods
assert stored_secrets == [
    ("TELEGRAM_BOT_TOKEN", token),
    ("TELEGRAM_OPERATOR_CHAT_IDS", "1122334455,5566778899"),
    ("TELEGRAM_WEBHOOK_SECRET", webhook_secret),
], stored_secrets
assert commands_set == [
    {"command": "osl", "description": "Command hierarchy"},
    {"command": "stats", "description": "Live commerce summary"},
    {"command": "payments", "description": "Payments and Pro licenses"},
    {"command": "downloads", "description": "Download requests"},
], commands_set
assert "Command menu verified: /osl" in output
assert f"Webhook verified: {module.WEBHOOK_URL}" in output
for forbidden in [
    token,
    webhook_secret,
    "1122334455",
    "5566778899",
    "-1001234567890",
    "alice_owner",
    "Bob",
    "Ops",
    "private_owner_bot",
    "@",
    "chat ID",
]:
    assert forbidden not in output, forbidden
`;

    const liveProof = spawnSync("python3", ["-c", driver, scriptPath], {
      encoding: "utf8",
    });
    expect(liveProof.status, liveProof.stderr).toBe(0);
  });
});
