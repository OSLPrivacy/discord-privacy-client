# Receiving Screen Sentences

Task 3903 inventory, checked against the receive-path source.

- Sentence: "OSL: invalid first-party chat history row"
  Cause: OSL Chat receive has authenticated a peer message but the durable history row assembled from it is structurally invalid before persistence.

- Sentence: "OSL Chat history is unavailable"
  Cause: OSL Chat receive or history loading cannot lock or access the local encrypted message-history store.

- Sentence: "OSL: first-party chat history: {error}"
  Cause: OSL Chat receive reached the local store but the store refused the inbound history write; the store error is appended after the fixed prefix.

- Sentence: "Turn on decrypted text for this conversation before opening it"
  Cause: Direct peer text opening or OSL Chat history loading is requested while decrypted display is disabled for the active conversation.

- Sentence: "OSL Privacy account storage is unavailable"
  Cause: The receive/open path needs the active account directory for pointer resolution or protected local state, and the account storage directory cannot be resolved.

- Sentence: "This encrypted message could not be opened"
  Cause: Generic non-disclosing receive refusal for a protected text row that must not reveal which check failed.

- Sentence: "OSL could not reach the protected message store. Try again shortly"
  Cause: A cover pointer decoded for this conversation, but the protected message store was unavailable or returned a retryable transport failure.

- Sentence: "This view-once message is unavailable or expired"
  Cause: A native Discord view-once reveal is requested for an invalid id, an already consumed message, or a drain that does not produce exactly one consumable view-once row.

- Sentence: "OSL could not receive protected messages"
  Cause: The protected receive drain cannot derive the scope, relay scope, or sender-filtered control-inbox page needed for this conversation.

- Sentence: "OSL retained private rows because the sender identity is untrusted"
  Cause: The sender-filtered control inbox retained only quarantined untrusted private rows and delivered no usable rows.

- Sentence: "OSL retained private rows while the sender identity is temporarily unavailable"
  Cause: The sender-filtered control inbox retained retryable rows because sender identity validation is temporarily unavailable and delivered no usable rows.

- Sentence: "OSL retained private rows for a terminal sender identity"
  Cause: The sender-filtered control inbox retained terminally refused private rows and delivered no usable rows.

- Sentence: "OSL retained private rows for a disabled sender identity"
  Cause: The sender-filtered control inbox retained disabled-sender private rows and delivered no usable rows.

- Sentence: "OSL privacy receipt was invalid"
  Cause: The receive drain found a privacy receipt row, but the signed receipt or its binding failed validation.

- Sentence: "OSL receipt state is unavailable"
  Cause: The receive drain cannot lock the in-memory receipt state while admitting a valid privacy receipt.

- Sentence: "Decryption display is off for this conversation"
  Cause: A protected local open path reaches its display-policy gate while decrypted display is disabled.

- Sentence: "Only the trusted native Discord overlay may receive text"
  Cause: The native Discord text receive command is called by any webview other than the bundled native Discord overlay.

- Sentence: "OSL native overlay worker failed: {error}"
  Cause: The blocking native Discord receive or reveal worker is interrupted or returns a fixed receive-path error; the fixed prefix is shown with the worker error.

- Sentence: "Only the trusted native Discord overlay may reveal view-once text"
  Cause: The native Discord view-once reveal command is called by any webview other than the bundled native Discord overlay.

- Sentence: "Only the trusted OSL window may receive OSL Chats"
  Cause: The OSL Chat receive command is called by any webview other than the trusted main OSL window.

- Sentence: "Windows capture resistance is required to receive OSL Chats"
  Cause: The OSL Chat receive command cannot apply the required capture-resistance setting before opening received text.

- Sentence: "OSL Chat worker failed: {error}"
  Cause: The blocking OSL Chat receive worker is interrupted or returns a fixed receive-path error; the fixed prefix is shown with the worker error.

- Sentence: "Only the trusted OSL window may read OSL Chat history"
  Cause: The OSL Chat history command is called by any webview other than the trusted main OSL window.

- Sentence: "Windows capture resistance is required to read OSL Chat history"
  Cause: The OSL Chat history command cannot apply the required capture-resistance setting before showing stored received text.

- Sentence: "OSL Chat history worker failed: {error}"
  Cause: The blocking OSL Chat history worker is interrupted or returns a fixed receive-path error; the fixed prefix is shown with the worker error.

- Sentence: "OSL Chat reaction storage is unavailable"
  Cause: OSL Chat history loading cannot resolve or read the local encrypted reaction ledger used to decorate received chat history.

- Sentence: "Only the trusted OSL window may react to OSL Chats"
  Cause: The OSL Chat reaction command is called by any webview other than the trusted main OSL window.

- Sentence: "OSL Chat reaction state is unavailable"
  Cause: The reaction command cannot lock or access the local reaction state for the active OSL Chat row.

- Sentence: "OSL Chat reaction worker failed: {error}"
  Cause: The blocking OSL Chat reaction worker is interrupted or returns a fixed receive-path error; the fixed prefix is shown with the worker error.

- Sentence: "Only the trusted OSL window may remove OSL Chat reactions"
  Cause: The OSL Chat reaction-removal command is called by any webview other than the trusted main OSL window.

- Sentence: "Only the trusted OSL window may search OSL Chat history"
  Cause: The OSL Chat history search command is called by any webview other than the trusted main OSL window.

- Sentence: "Windows capture resistance is required to search OSL Chat history"
  Cause: The OSL Chat history search command cannot apply the required capture-resistance setting before showing matching received text.

- Sentence: "OSL Chat history search query is invalid"
  Cause: The OSL Chat history search command received an empty, malformed, or otherwise invalid query before reading stored rows.

- Sentence: "OSL Chat history search worker failed: {error}"
  Cause: The blocking OSL Chat history search worker is interrupted or returns a fixed receive-path error; the fixed prefix is shown with the worker error.

- Sentence: "Only the trusted OSL window may open OSL Chat history results"
  Cause: The OSL Chat history-result open command is called by any webview other than the trusted main OSL window.

- Sentence: "Windows capture resistance is required to open OSL Chat history results"
  Cause: The OSL Chat history-result open command cannot apply the required capture-resistance setting before showing stored received text.

- Sentence: "OSL Chat history result is no longer in this conversation"
  Cause: The selected OSL Chat history result no longer belongs to the active conversation when the open command resolves it.

- Sentence: "OSL Chat history result worker failed: {error}"
  Cause: The blocking OSL Chat history-result worker is interrupted or returns a fixed receive-path error; the fixed prefix is shown with the worker error.

- Sentence: "Only the trusted OSL window may burn OSL Chat history"
  Cause: The OSL Chat history burn command is called by any webview other than the trusted main OSL window.

- Sentence: "OSL Chat burn worker failed: {error}"
  Cause: The blocking OSL Chat history burn worker is interrupted or returns a fixed receive-path error; the fixed prefix is shown with the worker error.

- Sentence: "OSL Chat visible-record query failed: {error}"
  Cause: The OSL Chat history burn path cannot query the visible stored records for the active conversation.

- Sentence: "OSL Chat refused unprotected mode"
  Cause: The OSL Chat receive path is asked to open or process protected chat text while the required protected mode is unavailable.

## Shared Refusal Rule

The sentence "This encrypted message could not be opened" is deliberately shared by these three `PeerProsePointerFailure` causes:

- `NotAToken`
- `PointerBlobGone`
- `Rejected`

That is a rule, not a bug. The receiving path must not tell an observer whether the row was ordinary chat, content gone/burned/expired, or a failed authentication/decryption/binding check.
