# CONTRACT — entitlement (track T16)

**Owner:** T16. **Status:** FROZEN 2026-07-31 (credits boundary revision).
**Consumer:** T13 credits metering.

This appendix freezes the boundary T13 must use. It does not authorise a credit
checkout, change the prepaid Pro entitlement contract, or alter the encryption
floor.

## 6. Credits boundary

Credits are a separate one-time purchase, never a renewal or a Pro grant. A
credit balance is not an entitlement: it neither grants Pro nor changes the
entitlement state machine. Conversely, Pro is not a credit balance and creates
no credits.

Encryption never consults either a credit balance or entitlement state. A zero
credit balance degrades optional cloud AI to the word-bank fallback only; it
does not degrade encryption, text send/receive, decrypting/opening received
attachments, or any other Free-floor path.

### Credits boundary vector

```json
{
  "credits": {
    "purchase": "separate-one-time",
    "balanceGrantsPro": false,
    "proCreatesBalance": false
  },
  "encryption": {
    "dependsOnEntitlement": false,
    "dependsOnCredits": false
  },
  "zeroBalance": {
    "degrades": "optional-cloud-ai-only",
    "fallback": "word-bank"
  }
}
```

## Consumer rule

T13 owns the meter. T16 owns this boundary. T13 must call the entitlement
seam rather than derive a Pro tier from credits, and must retain the word-bank
fallback when cloud AI is unavailable.
