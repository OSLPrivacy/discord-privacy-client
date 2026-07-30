# Release lane - 2026-07-26

This report records release qualification evidence only. It does not edit the
checklist and does not convert local source tests into live release evidence.

## Discord Release Qualification {#discord_release_qualification}

`discord_release_qualification: test-proven-only`

The current Discord release candidate has two local gates:

- `cd apps/osl-hub-ui && npm test -- overlay-send-gesture.test.ts discord-qa-send-stage.ts`
- `python3 scripts/qa/c4-shipping-evidence-test.py`

The first gate qualifies the Double Enter handoff that the disposable Discord QA
overlay uses before calling native. The accepted behavior is narrow: first
trusted Enter is consumed, the second Enter must be a distinct trusted press
after key-up, and intervening draft input or an invalid second Enter attempt
cancels the armed handoff. Expiry, cancellation, refusal, or delivery uncertainty
must leave the draft available; no path auto-retries or treats missing consent as
send authority.

The second gate qualifies the production evidence contract for C4. Admissible
Discord release evidence is a bounded `osl-c4-shipping-evidence-v1` bundle from a
shipping build only:

- frontend command `npm --prefix apps/osl-hub-ui run build`
- desktop build command using `osl-cargo` with cargo features exactly `["desktop"]`
- `qaShell` is `false`
- executable bytes contain no `discord-qa-shell`, `send_native_discord_qa_atomic_text`,
  `discord-qa-send-stage-receipt.json`, or `osl-discord-qa-send-stage.txt` marker
- command receipt source `production-overlay-ui`
- receipt authority `shipping-renderer-success-gate`
- UI control automation id `prepare-protected`
- backend command `send_native_discord_overlay_carrier`
- terminal receipt `status: "sent"`, `placed: true`, and `enterSent: true`
- `preEnterReadback` authority `native-pre-enter-exact-readback`, relation
  `rawExact`, `readCount: 1`, and `exact: true`
- `postEnterComposer` has `readCount: 1`, `utf8Bytes: 0`, the empty SHA-256, and
  `empty: true`
- before and after conversation snapshots bind the same target, each match one
  named conversation and one transcript, and the row count increases by exactly
  one
- `newRows` contains exactly one row whose target binding matches the snapshots,
  whose carrier hash matches the pre-Enter readback, and whose match count is
  exactly one
- the screenshot is a PNG inside the evidence bundle, hashes exactly, binds the
  same target, and shows one matching new row

QA-shell trails, QA atomic command receipts, append-only send-stage files, stale
screenshots, fabricated row deltas, or source-level success are not Discord
release qualification evidence. They are useful diagnostics only.

This status does not claim a released build, a production deployment, or a live
Discord send. Moving C4 beyond `test-proven-only` still requires one
owner-approved run against the exact shipping executable and expected
conversation, followed by the verifier accepting that bundle.
