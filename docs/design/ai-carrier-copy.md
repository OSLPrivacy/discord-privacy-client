# AI carrier copy

Status: planned copy for a future interface. AI-generated carrier text and
processing credits are not available in the shipping app and credits are not
on sale. This page is a source for T7; it is not a product-availability claim.

## Cloud-processing consent

### Title

Use cloud processing for carrier text?

### Body

This is a planned option. Cloud processing is less private than local
processing because selected cover context reaches an OSL server. It is not
end-to-end private. OSL does not send the protected message itself to the
cloud model.

Choose local processing if you want the privacy-preferred option. You can
decline cloud processing and still use OSL.

### Consent action

I understand that cloud processing is less private than local processing.

Source: `AI-generated carrier text` in the public-claim allowlist; D46 in
the AI carrier architecture.

## In-flow cloud label

Cloud processing selected — less private than local processing; not
end-to-end private.

Source: `AI-generated carrier text` in the public-claim allowlist.

## Credits exhausted

Cloud processing credits are exhausted. Use local processing if available, or
continue with the free word-bank carrier. Encryption still works.

Source: `Processing credits` and `AI-generated carrier text` in the
public-claim allowlist; the architecture's required free-tier floor.

## Local model unavailable

Local AI processing is unavailable. Continue with the free word-bank carrier.
Encryption still works.

Source: `AI-generated carrier text` in the public-claim allowlist; the
architecture's fail-closed local-model condition and required free-tier floor.

## Sensitive-content warning

This message was detected as potentially sensitive. Sending it in an
unencrypted composer could expose it to that service. Review the message
before you send it.

Source: `Before-send exposure warning` in the public-claim allowlist and
master decision §7.12. This wording describes a possible consequence; it does
not judge the message, its author, or a topic.
