# OSL Privacy

[![Rust Test](https://github.com/OSLPrivacy/discord-privacy-client/actions/workflows/rust-test.yml/badge.svg)](https://github.com/OSLPrivacy/discord-privacy-client/actions/workflows/rust-test.yml)

OSL Privacy is a pre-release desktop privacy client for Discord-centered
workflows. The repository contains the original Discord-specific client and the
newer multi-service hub under `apps/osl-hub*`. The public claim boundary is the
one in [`docs/design/osl-public-claim-allowlist.md`](docs/design/osl-public-claim-allowlist.md):
message contents use a hybrid X25519 plus ML-KEM-768 encryption path in source
and QA builds, but no named release build in this checkout proves the full
Discord send/receive path end to end.

Treat this as beta software. Keep a backup plan and do not rely on it for
anything where a failure would be serious. Features that are source-present but
not proven on a named release build are described as planned, not available.

## What it protects and what it does not

On the proved encrypted-message path, Discord receives an encrypted block, not
your message, and it never receives the decryption key. Discord still sees who
you talk to, when, and how often. OSL does not hide that you use Discord, and it
does not protect against targeted investigation. If your threat model includes a
targeted investigation of you specifically, use Signal, Briar, or Cwtch instead.

The full write up is in [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md).

## How the encryption works

Every install generates a long-term identity on first launch. The identity holds
X25519, Ed25519 and ML-KEM-768 public keys that may be published so other OSL
users can find them. Message contents are encrypted with a hybrid scheme
combining X25519 and ML-KEM-768; breaking confidentiality requires breaking
both.

This protects contents against future quantum decryption of recorded traffic.
It does not make identity verification post-quantum. Forward secrecy,
post-compromise security, group sender keys, view-once messages, timed deletion
and attachment transport are planned or partially implemented, but they are not
release-proven capabilities today.

## Using it

### The lock button

Every supported conversation header gets a lock icon. It has three states:

- Open (grey): nobody in this conversation is set up, messages go out as plain
  text.
- Partial (yellow): some people are set up, so OSL cannot promise full
  protection for the conversation.
- Full (green): everyone is set up, the whole conversation is encrypted.

Click the lock to change the encryption setting for that conversation. In a
server you can encrypt a single channel or turn on a server wide setting that
covers every channel you are in.

### Burn

Burn deletes OSL's own copies. It is **not** cryptographic erasure and it does not
destroy keys — what that means in practice is spelled out below. There are two
kinds and they are different.

Scope burn shreds OSL's local copies for one conversation. The stored ciphertext
and nonce are overwritten in place, the rows are marked burned so a later sync
cannot write them back, cached attachments go with them, and OSL's server-side
state for that scope is deleted. A peer-notification path is implemented, but it
has not been proved end to end and is not available as a working peer action
today. Do not rely on Burn to remove another member's copy. Use scope burn when
you no longer trust the people in one channel with your past messages.
Everything else you have is untouched.

Account burn shreds everything on your machine — every conversation and every
saved message — and generates a fresh identity so you can start over. Use this as
a panic button. It cannot be undone.

**What burn does not do.** It does not destroy anyone's ability to decrypt. OSL
seals each message to the recipient's long-term keys, so the carrier that Discord
still holds stays readable to anyone who has that key material, and burning your
copy does not change that. It does not delete anything from Discord: the carrier
messages stay in the channel and Discord's own copies stay on Discord's servers.
It does not touch provider retention, exports, backups or screenshots, it cannot
reach a recipient who already read the plaintext, and it cannot reach a client
that ignores the notice.

Burn cleans up. It does not un-send. Making burn genuinely cryptographic would
require sealing every message under a per-message key that lives off your device,
which is designed but deliberately not built. See
[`docs/design/burn-contract.md`](docs/design/burn-contract.md).

### Deleting messages from Discord

This is a separate action from burn, and a separate promise. Burn makes your own
messages unreadable; deleting them from Discord means asking Discord to remove
them.

Guided deletion is a Pro feature of the newer hub shell under `apps/osl-hub*`. It
works the way you would by hand, and for the same reason: there is no supported
API for deleting your own messages with your own account token, and using the
private one is self-botting and puts your account at risk. So OSL drives
Discord's own interface through Windows accessibility — it focuses one of your
message rows, opens that row's own menu, chooses Discord's own delete item and
confirms in Discord's own dialog. It never moves your pointer and it only ever
touches messages you wrote; another person's message has no delete item and is
reported as unsupported.

Every run is `Scan → Preview → Confirm → Execute → Verify → Receipt`. You see
exactly which rows will be attempted before anything happens, and the plan is
bound to that exact list, so changing the selection means confirming again. After
each row OSL re-reads the transcript, and a row is only reported as
`Deleted from Discord` when that re-read proves the row is gone. Anything else is
reported as `Sent request - not verified`, `Held` or `Unsupported`, and says
which. A request OSL could not verify is never displayed as a deletion.

Three separate facts, never merged into one claim:

- **Removed from Discord** — OSL used Discord's own delete control and proved by
  re-reading the conversation that the row is no longer there.
- **OSL content expired** — the encrypted content became undecryptable because
  its keys are gone. That is burn, above.
- **Removed locally** — OSL deleted its own cached copy on this device.

Status: the scan, the preview, the confirmation and the verifying re-read are
built and tested. The step that posts the keystrokes into Discord's own menu is
not enabled yet, so today every row comes back `Held` and nothing is deleted —
which is the point of a fail-closed design. It will not be turned on until it has
been proven against a real conversation.

### Settings

Open settings from the gear in the OSL toolbar. You can manage your identity, set
a password and recovery phrase, pick which servers and channels encrypt by
default, choose an update channel, and review keyboard shortcuts. Burn controls
live here too.

## Requirements

Windows 10 or newer. macOS and Linux are not supported and onboarding will exit
on them. You need a normal Discord account; OSL signs in through the real Discord
login.

## Discord Terms of Service

Using OSL may break Discord's Terms of Service and could get your account
suspended. You take that risk on yourself.

## Repository layout

```
src-tauri/            Tauri app shell (Rust) and the injected boot.js
crates/
  crypto/             PQXDH hybrid handshake, Double Ratchet, sender keys, AEAD
  stego/              Cover text generation (bigram language model + templates)
  keystore/           Identity, key sealing, keyserver client, control inbox
  store/              Encrypted local message store (SQLite)
  ipc/                Tauri command surface and message pipeline
keyserver-cf/         Keyserver on Cloudflare Workers (D1): identity, prekeys,
                      licenses, control inbox
cipher-store-cf/      Encrypted blob holding store on Cloudflare Workers
docs/
  THREAT_MODEL.md
  design/             Per feature design docs
```

## Building

The app is a Tauri 2 project. From `src-tauri/` run `cargo tauri dev` for a
development build. The two Cloudflare Workers in `keyserver-cf/` and
`cipher-store-cf/` deploy with `npm run deploy` and apply their database
migrations with `wrangler d1 migrations apply`.

## License

Apache-2.0. See [`LICENSE`](LICENSE).
