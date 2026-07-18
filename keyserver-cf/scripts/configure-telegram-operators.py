#!/usr/bin/env python3
"""Safely configure the OSL operator bot without persisting its token."""

from __future__ import annotations

import getpass
import json
import re
import secrets
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any


WORKER_DIR = Path(__file__).resolve().parent.parent
WEBHOOK_URL = "https://keyserver.oslprivacy.com/v1/telegram/webhook"
TOKEN_RE = re.compile(r"^[0-9]{6,12}:[A-Za-z0-9_-]{30,80}$")


def fail(message: str) -> "NoReturn":
    print(f"Error: {message}", file=sys.stderr)
    raise SystemExit(1)


def telegram(token: str, method: str, **fields: object) -> dict[str, Any]:
    encoded = urllib.parse.urlencode(fields).encode("utf-8")
    request = urllib.request.Request(
        f"https://api.telegram.org/bot{token}/{method}",
        data=encoded,
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            payload = json.load(response)
    except (urllib.error.URLError, TimeoutError, json.JSONDecodeError) as exc:
        fail(f"Telegram {method} request failed: {type(exc).__name__}")
    if not isinstance(payload, dict) or payload.get("ok") is not True:
        fail(f"Telegram rejected {method}")
    return payload


def put_worker_secret(name: str, value: str) -> None:
    command = [
        "npx",
        "-y",
        "-p",
        "node@22",
        "node",
        "node_modules/wrangler/bin/wrangler.js",
        "secret",
        "put",
        name,
    ]
    result = subprocess.run(
        command,
        cwd=WORKER_DIR,
        input=f"{value}\n",
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if result.returncode != 0:
        fail(f"Cloudflare rejected {name}; Wrangler exited {result.returncode}")
    print(f"Stored Cloudflare secret: {name}")


def display_name(chat: dict[str, Any]) -> str:
    username = chat.get("username")
    if isinstance(username, str) and username:
        return f"@{username}"
    parts = [chat.get("first_name"), chat.get("last_name"), chat.get("title")]
    name = " ".join(part for part in parts if isinstance(part, str) and part)
    return name or "unnamed account"


def main() -> None:
    print("OSL Telegram operator setup")
    print("The token is hidden, used in memory, and never written to disk.\n")
    token = getpass.getpass("Paste the replacement Telegram bot token: ").strip()
    if not TOKEN_RE.fullmatch(token):
        fail("the bot token format is invalid")

    identity = telegram(token, "getMe").get("result")
    if not isinstance(identity, dict) or not isinstance(identity.get("username"), str):
        fail("Telegram returned an invalid bot identity")
    print(f"\nBot verified: @{identity['username']}")

    webhook = telegram(token, "getWebhookInfo").get("result")
    if isinstance(webhook, dict) and webhook.get("url"):
        print("Removing the existing webhook briefly so approved accounts can be discovered.")
        telegram(token, "deleteWebhook", drop_pending_updates="false")

    print("\nFrom every Telegram account that should receive OSL alerts:")
    print(f"  1. Open @{identity['username']}")
    print("  2. Press Start, then send /stats")
    input("\nWhen every intended account has sent a message, press Enter here... ")

    updates = telegram(token, "getUpdates", timeout="10", limit="100").get("result")
    if not isinstance(updates, list):
        fail("Telegram returned an invalid update list")

    chats: dict[str, dict[str, Any]] = {}
    for update in updates:
        if not isinstance(update, dict):
            continue
        message = update.get("message")
        chat = message.get("chat") if isinstance(message, dict) else None
        chat_id = chat.get("id") if isinstance(chat, dict) else None
        chat_type = chat.get("type") if isinstance(chat, dict) else None
        if chat_type == "private" and isinstance(chat_id, int) and chat_id != 0:
            chats[str(chat_id)] = chat

    if not chats:
        fail("no private accounts messaged the bot; send /stats and run this again")

    candidates = sorted(chats.items(), key=lambda item: int(item[0]))
    print("\nAccounts that deliberately messaged this bot:")
    for index, (chat_id, chat) in enumerate(candidates, start=1):
        print(f"  {index}. {display_name(chat)} (chat ID {chat_id})")

    raw_selection = input(
        "\nEnter the numbers to authorize, comma-separated (or 'all'): "
    ).strip().lower()
    if raw_selection == "all":
        selected = candidates
    else:
        try:
            indexes = {int(value.strip()) for value in raw_selection.split(",")}
        except ValueError:
            fail("selection must be 'all' or comma-separated numbers")
        if not indexes or min(indexes) < 1 or max(indexes) > len(candidates):
            fail("selection is outside the displayed range")
        selected = [candidate for index, candidate in enumerate(candidates, 1) if index in indexes]

    chat_ids = ",".join(chat_id for chat_id, _ in selected)
    webhook_secret = secrets.token_urlsafe(36)
    # Store the exact token used for getMe/setWebhook so the Worker cannot
    # accidentally retain a different replacement token from an earlier run.
    put_worker_secret("TELEGRAM_BOT_TOKEN", token)
    put_worker_secret("TELEGRAM_OPERATOR_CHAT_IDS", chat_ids)
    put_worker_secret("TELEGRAM_WEBHOOK_SECRET", webhook_secret)

    telegram(
        token,
        "setMyCommands",
        commands=json.dumps(
            [
                {"command": "stats", "description": "Live commerce summary"},
                {"command": "payments", "description": "Payments and Pro licenses"},
                {"command": "downloads", "description": "Download requests"},
            ],
            separators=(",", ":"),
        ),
    )
    telegram(
        token,
        "setWebhook",
        url=WEBHOOK_URL,
        secret_token=webhook_secret,
        allowed_updates=json.dumps(["message"]),
        drop_pending_updates="true",
    )
    info = telegram(token, "getWebhookInfo").get("result")
    if not isinstance(info, dict) or info.get("url") != WEBHOOK_URL:
        fail("Telegram did not retain the expected webhook URL")

    print(f"\nConfigured {len(selected)} approved operator account(s).")
    print(f"Webhook verified: {WEBHOOK_URL}")
    print("Send /stats from each approved account after the Worker deployment completes.")


if __name__ == "__main__":
    main()
