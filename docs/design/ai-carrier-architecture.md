# AI carrier architecture

## Free tier today

<!-- current-free-cover-record
{
  "currentCarrier": "fixed beacon string",
  "currentValue": "🔒 OSL private message",
  "observerSignal": "an exact-match classifier identifies it",
  "productionStealth": "off",
  "requiredFreeCarrier": "word-bank carrier only"
}
-->

The current free-cover implementation returns the fixed string `🔒 OSL private
message`. It carries no message-dependent variation, so an observer can label it
with a single exact-match rule. This is a beacon, not cover traffic.

That helper currently has no production call site. This does not make the
design safe: it is still the implemented free-cover path, ready to create a
perfect classifier signal if wired into sending.

The shipping path is not a stealth mechanism today. It coerces Mode 1 to Mode
0 and emits a visible `DPC0::` capsule. Product and public copy must therefore
describe stealth as **off**, not merely degraded.

## Required free-tier floor

Owner decision D49 requires **word-bank carrier only** for Free. The word-bank
path must always ship and work with no model pack, network access, credits, or
cloud consent; encryption must never depend on an optional component.

T13-B2 replaces the fixed free-cover constant with the existing codec's
word-bank output. It must not introduce a second word list. Until that change
lands, the fixed beacon is an explicitly recorded defect rather than a
stealth claim.
