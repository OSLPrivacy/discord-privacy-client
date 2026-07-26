# OSL Chats Lab

An unwired, localhost-only product prototype for the first-party OSL Chats experience.

It contains seeded fake users and history, encrypted-chat states, Circles, calls, a drawable chat canvas, bots, user automations, instant data exports, developer tooling, message privacy rules, and deep appearance controls. It has no real accounts, network transport, key material, provider APIs, or remote mutations.

The default visual system mirrors the current OSL client: `#0a0a0a` background, flat `#141414` panels, cyan `#06b6d4` actions, Inter typography, one-pixel dividers, six-pixel corners, and restrained motion. Alternate themes remain optional customization rather than changing the OSL default.

## Run

```bash
cd /home/liamw/discord-privacy-client/docs/prototypes/osl-chats-lab
python3 -m http.server 4173 --bind 127.0.0.1
```

Open `http://127.0.0.1:4173`.

## Prototype interactions

- Switch between DMs and Circles.
- Search conversations and messages.
- Send local messages, reactions and replies; scrub your own messages instantly.
- Preview per-message expiry, view-once/open-count, forwarding and timer rules.
- Open Circle channels and preview voice/video rooms.
- Draw on an encrypted-canvas concept and display it in a chat.
- Create a Circle locally.
- Inspect devices and conversation security.
- Generate local JSON, CSV, HTML, or portable-archive downloads.
- Create developer projects, toggle fake bots, inspect events, and run user automations.
- Change themes, density, message shape, typography, profile effects, and animated avatar URLs/uploads.
- Reset all demo state from Settings.

All state stays in browser `localStorage` under `osl-chats-lab-v4`.

## Fair storage model represented here

- **Free:** the complete messenger, full local history, direct/LAN transfers, user-owned storage connectors, temporary encrypted relay, calls, Circles, canvases, bots, customization and every export format.
- **Pro:** durable OSL-operated encrypted file hosting, redundant copies, long retention, restoration when no peer device is online, and priority relay.
- Temporary relay ciphertext is deleted after one device per recipient acknowledges it by default, or at a user-selected expiry. Requiring every active device is optional because a retired device must not make relay storage permanent.

The honest product term is **local-first with zero-retention relay**, not “100% local”: while delivery is pending, OSL infrastructure may transiently handle encrypted ciphertext and minimal routing metadata.
