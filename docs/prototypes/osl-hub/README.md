# OSL Privacy prototype

This is a clickable, simulated-only interface prototype for reviewing OSL Privacy, Secure Composer, account switching, privacy scan, and cleanup flows. It does not provide real encryption, scanning, sending, deletion, or platform integration.

## Run locally

From the repository root:

```bash
python3 -m http.server 4173 --bind 127.0.0.1 --directory docs/prototypes/osl-hub
```

Then open <http://127.0.0.1:4173/>.

## Interaction tour

- Explore Home, Inbox, People, Secure Composer, Privacy, Connections, Activity, and Settings.
- Switch between services and multiple accounts; drafts and simulated state are scoped to the selected account.
- For each supported conversation, compare the persistent **Native** and **OSL Protected** modes. Native represents continuing in the underlying app or site with its full feature set; OSL Protected represents only the capabilities OSL can safely provide.
- Try Secure Composer options, hand a simulated protected payload to the underlying composer, and finish with the separate native Send action.
- Change a simulated recipient or add an unsupported feature to see the protected send stop and explain the deliberate Native/assist fallback. The prototype never switches or downgrades modes silently.
- Simulate a layout change, low-confidence recovery, or a platform challenge to review safe fallback states.
- Run the local sensitive-history demo and inspect its calm, guided findings.
- Preview the Free and Pro experiences. No purchase or real account is involved.

The launch mockup covers Discord, Telegram, Instagram, Snapchat, email providers, X, Slack, Teams, and Facebook Messenger. It demonstrates multiple accounts per service.

Full service functionality comes from companioning each native app or site, not from recreating every platform feature inside OSL. True OSL E2EE exists only among OSL-capable, verified endpoints. Unsupported recipients and features fall back only after a plain explanation and explicit user choice; ordinary external platform messages and email are never presented as OSL E2EE.

## Tier model shown

- **Free:** core protected text features and all connected accounts, with media disabled.
- **Pro:** protected media plus guided history-cleanup workflows.

There is no Business tier in this prototype.

## Production caveats

- All content and outcomes are simulated. Nothing is encrypted, scanned, sent, deleted, or synchronized.
- The prototype accepts no platform credentials and performs no platform automation or network requests.
- Assist-only integrations require the user to perform the final native Send or Delete action.
- Mode and encryption claims are scoped to the active conversation. Production must recheck recipient, endpoint and feature support before every protected handoff and must never silently switch to Native or weaken protection.
- Earlier UI concepts allowed timers from 1 hour to 7 days, while the current backend accepts 24 hours, 72 hours, or 7 days. This prototype uses only compatible simulated choices; production support must be aligned before exposing additional timers.

## Safety boundary

OSL will not implement fingerprint spoofing, anti-detection behavior, simulated typing speed or keystrokes, ban bypasses, CAPTCHA bypasses, or other enforcement evasion. Self-healing means safely detecting layout changes, pausing at low confidence, and falling back to user-assisted operation—not hiding automation.

Launch and test the app and service companions locally with test accounts in dedicated browser profiles. Do not use VMs for normal development or integration testing; reserve separate local devices or isolated environments for P2P and multi-endpoint tests that genuinely require them. Never paste passwords, session cookies, recovery codes, or account tokens into chat.
