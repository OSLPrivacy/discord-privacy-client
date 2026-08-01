# Sensitive-content warning contract

## Purpose

This feature warns about an exposure consequence, not about whether a person should say
something. It exists to give a person one clear choice before text leaves their device without
OSL encryption: send it anyway, protect it with OSL, or go back to editing.

The warning must use neutral consequence language. Its user-facing meaning is:

> This message will leave your device in the clear. The platform can keep a copy.

It must not describe content as acceptable, unacceptable, abusive, suspicious, prohibited, or
otherwise morally or politically judged.

## Definitions

- **Protected send** means an OSL-encrypted send. Its plaintext is not an unencrypted native
  message sent to the platform.
- **Unencrypted send** means sending the draft as ordinary native-platform text without OSL
  encryption.
- **Draft** means the current unsent message. A new or materially edited draft is a new draft for
  this contract.
- **Finding** means a local detector result from a category defined in the later categories
  contract. A finding is a reason to offer the warning, never a verdict about the user or text.

## Required behavior

1. Evaluate the draft only at the attempted send action, immediately before an **unencrypted**
   send. A protected send never opens this warning, even if there is a finding.
2. With an enabled warning, an unencrypted send with a finding opens the consequence warning.
   An unencrypted send without a finding proceeds normally.
3. The warning offers three equally available choices: **send unencrypted**, **protect with OSL**,
   and **keep editing**. Choosing **send unencrypted** must allow that same send to proceed.
   The warning is therefore not a send failure, a hold, or a requirement to change the text.
4. A user may dismiss the warning once for a draft, mute a category, or disable the feature
   globally. Those choices must be honored without repeated prompts. Turning it off may make the
   user less informed; it must not make sending impossible.
5. The classifier runs locally only. It has no network path and must not retain or log plaintext,
   findings, category decisions, or a history of what triggered warnings. Preference state may
   record only the user's mute/off choices, never the matched content or finding.
6. This feature is not moderation: it does not block, report, rank, punish, shadow-limit, score,
   or label a user or their content. It makes no safety, legality, civility, or policy decision.
7. Ordinary political discussion is not sensitive for this feature and must not trigger this
   warning. The same red line applies to contextual discussion, quotation, self-description, and
   dialect; a category that cannot meet it is excluded rather than softened.

## Non-goals and boundaries

This contract does not define detector implementation or a semantic safety judgement. It also does
not claim that an unencrypted platform will definitely retain a message. The warning states the
meaningful consequence: the plaintext leaves the device and the platform can retain it.

## Categories and detector boundary

The warning is about accidental exposure, so its categories are grouped by the detector that can
actually produce them, rather than by a broad label such as “sensitive content.” A finding means
only that its stated detector matched. It does not establish intent, truth, legality, offensiveness,
or whether sending is wise.

### Structured text checks — included in v1

These are deterministic, local checks over the text draft. They do not use an AI model or a
general-purpose entropy scanner.

| Category | What detects it | Boundary |
|---|---|---|
| Credential | Secret assignment labels and known credential prefixes | This detects a credential-shaped value, not whether a person is authorised to use it. |
| Recovery material | Specific recovery or private-key phrases | A phrase is a warning signal, not proof that a recovery secret is present. |
| Payment card | A 13–19 digit run validated with Luhn | A Luhn-valid number can still be a test or non-card number. |
| Government identity | SSN shape plus a short label list | It does not validate identity documents or infer nationality. |
| Precise location | Location-introducing phrases together with digits | It does not geocode text or infer a location from context. |

### Lexical text checks — included in v1, with the same non-blocking boundary

These are local word or phrase lists, not semantic understanding and not ML. They can offer the
same exposure warning, but must never be described as judging a user or their message.

| Category | What detects it | Boundary |
|---|---|---|
| Profanity | Word list | Does not detect abuse, harassment, or hate. |
| Sexual content | Word and phrase lists | Does not classify consent, legality, or explicit imagery. |
| Sensitive health | Phrase list | Does not diagnose or infer health status. |
| Controlled substances | Word and phrase lists | Does not determine possession, use, or legality. |
| Potentially unlawful conduct | Phrase list | Does not make a legal conclusion. |
| Work-sensitive information | Phrase list | Does not prove confidentiality or scan for every business secret. |

There is deliberately no general entropy scan over chat text: UUIDs, hashes, base64, and crypto
addresses would create too many unrelated findings. There is also no slur, toxicity, or political
classifier. Ordinary political discussion, quotation, self-description, and dialect remain outside
this warning as required by the contract.

### Attachments and imagery — not part of v1

V1 evaluates **text drafts only**. It does not inspect attachments before this warning. Explicit
imagery would require a local image classifier; the code has only an optional trait hook, with no
shipping implementation. OCR, video-frame extraction, and audio transcription are likewise
optional hooks, not v1 detectors. No current detector identifies imagery targeting a group; that
is not implied by the generic image-classifier hook and is out of scope rather than silently
treated as detected.

## Contract test vectors

The following vectors are executable checks for later policy implementations. `warning` means
show the non-blocking choice described above; `send` means continue without showing it.

```json
{
  "version": 1,
  "warning": {
    "trigger": "before-unencrypted-send",
    "localOnly": true,
    "retainsPlaintext": false,
    "logsFindings": false,
    "canBlock": false,
    "canJudge": false,
    "choices": ["send-unencrypted", "protect-with-osl", "keep-editing"]
  },
  "categories": {
    "shippingScope": { "text": true, "attachments": false },
    "detectorFamilies": {
      "structuredText": [
        "credential",
        "recovery-material",
        "payment-card",
        "government-identity",
        "precise-location"
      ],
      "lexicalText": [
        "profanity",
        "sexual-content",
        "sensitive-health",
        "controlled-substances",
        "potentially-unlawful-conduct",
        "work-sensitive-information"
      ],
      "attachmentOnly": ["explicit-imagery", "imagery-targeting-a-group"]
    },
    "unsupportedDetectors": ["general-entropy", "slur-or-toxicity-model"]
  },
  "cases": [
    {
      "name": "finding before an enabled unencrypted send warns",
      "protectedSend": false,
      "finding": "credential",
      "globalOff": false,
      "categoryMuted": false,
      "dismissedForDraft": false,
      "expect": "warning"
    },
    {
      "name": "encrypted sends are never warned",
      "protectedSend": true,
      "finding": "credential",
      "globalOff": false,
      "categoryMuted": false,
      "dismissedForDraft": false,
      "expect": "send"
    },
    {
      "name": "a user can disable the warning globally",
      "protectedSend": false,
      "finding": "credential",
      "globalOff": true,
      "categoryMuted": false,
      "dismissedForDraft": false,
      "expect": "send"
    },
    {
      "name": "a muted category does not nag",
      "protectedSend": false,
      "finding": "credential",
      "globalOff": false,
      "categoryMuted": true,
      "dismissedForDraft": false,
      "expect": "send"
    },
    {
      "name": "a dismissed draft does not warn again",
      "protectedSend": false,
      "finding": "credential",
      "globalOff": false,
      "categoryMuted": false,
      "dismissedForDraft": true,
      "expect": "send"
    },
    {
      "name": "an unflagged unencrypted send proceeds",
      "protectedSend": false,
      "finding": null,
      "globalOff": false,
      "categoryMuted": false,
      "dismissedForDraft": false,
      "expect": "send"
    }
  ]
}
```
