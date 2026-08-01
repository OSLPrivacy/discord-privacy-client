# Scrub provider-route evidence — 2026-07-31

This is the evidence ledger requested by T12-E2. It transcribes the provider
routes in T12 §4 and preserves the operative provider/API clause verbatim,
with the retrieval date. It is not a claim that any route is permitted for
OSL: owner decisions D55–D57 control the product route. In particular, paid
deletion APIs and Google CASA are out; rank-4 UI automation requires the
separate explicit, per-service consent gate.

## IMAP

- **§4 route:** IMAP `STORE +FLAGS \\Deleted` followed by `EXPUNGE` (or
  `UID EXPUNGE` where UIDPLUS is available).
- **Verbatim clause:** “The EXPUNGE command permanently removes all messages
  that have the \\Deleted flag set from the currently selected mailbox.”
- **Primary source:** [RFC 3501 §6.4.3](https://datatracker.ietf.org/doc/html/rfc3501#section-6.4.3)
- **Fetched:** 2026-07-31

## Gmail

- **§4 route:** `users.messages.trash` would require `gmail.modify`; D57
  rejects this API/CASA route. IMAP by app password, where available, or the
  separately consented UI route remains the plan.
- **Verbatim clause:** “Moves the specified message to the trash.”
- **Primary source:** [Gmail `users.messages.trash`](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages/trash)
- **Fetched:** 2026-07-31

The same API reference lists `https://www.googleapis.com/auth/gmail.modify`
as an authorization scope. Google’s scopes page classifies it as restricted:
“Restricted: These scopes provide wide access to Google user data and require
restricted scope OAuth App Verification.” [Source](https://developers.google.com/workspace/gmail/api/auth/scopes)
(fetched 2026-07-31).

## Reddit

- **§4 route:** `POST /api/del`, with the `edit` OAuth scope, is the retained
  free sanctioned API candidate; later work must read back because the route’s
  response is not sufficient deletion evidence.
- **Verbatim clause:** “/api/del”
- **Primary source:** [Reddit API documentation](https://www.reddit.com/dev/api/#POST_api_del)
- **Fetched:** 2026-07-31

The official endpoint index places `/api/del` under “links & comments”. That
is the complete provider text exposed by the public API index at retrieval;
it does not establish ownership or successful deletion.

## X (Twitter)

- **§4 route:** `DELETE /2/tweets/{id}` exists, but D56 prohibits the paid
  API route; use the free archive only as a local ID source and delete through
  the consented UI route.
- **Verbatim clause:** “Deletes a specific Post by its ID, if owned by the
  authenticated user.”
- **Primary source:** [X: Delete Post](https://docs.x.com/x-api/posts/delete-post)
- **Fetched:** 2026-07-31

The pricing evidence is also explicit: “Interaction: Delete | $0.010 per
request.” [X API pricing](https://docs.x.com/x-api/getting-started/pricing)
(fetched 2026-07-31).

## Instagram own media

- **§4 route:** the Graph API media route was considered for own posts/reels
  and own-media comments, but D55–D57 select UI automation plus erasure rather
  than a Meta app-review route.
- **Verbatim clause:** “You can delete posts that you've shared on Instagram
  at any time.”
- **Primary source:** [Instagram Help: delete posts](https://www.facebook.com/help/289302621183285)
- **Fetched:** 2026-07-31

This is direct provider evidence for the user-visible deletion route. It does
not represent an API capability for DMs or comments on another person’s post.

## Instagram DMs and comments on others’ posts

- **§4 route:** no deletion API is used; offer an erasure request first, then
  the consented UI route only when the UI reaches the item, otherwise a manual
  jump.
- **Verbatim clause:** “Depending on the type of content you're reviewing,
  you may have different options.”
- **Primary source:** [Facebook Help: Activity Log](https://www.facebook.com/help/269672196396014)
- **Fetched:** 2026-07-31

The clause is the provider’s limitation evidence: the UI cannot be assumed to
offer deletion for every content type. It replaces any unsupported claim that
a Graph API can delete Instagram DMs or another account’s content.

## Discord

- **§4 route:** the bot API is not a user-account deletion route. Under D55,
  OSL keeps the separately consented, human-rate UI route; this evidence is
  the basis for the Stage K account-termination warning.
- **Verbatim clause:** “Automating normal user accounts (generally called
  \"self-bots\") outside of the OAuth2/bot API is forbidden, and can result in
  an account termination if found.”
- **Primary source:** [Discord: Automated User Accounts (Self-Bots)](https://support.discord.com/hc/en-us/articles/115002192352-Automated-User-Accounts-Self-Bots)
- **Fetched:** 2026-07-31

## Facebook / Instagram Activity UI

- **§4 route:** UI-only review/delete, with manual jump as the fallback. Do
  not infer that hiding, archiving, or recycling is a verified deletion.
- **Verbatim clause:** “Delete: When you delete something from activity log,
  it will be deleted from Facebook and can't be restored.”
- **Primary source:** [Facebook Help: Activity Log](https://www.facebook.com/help/269672196396014)
- **Fetched:** 2026-07-31

The same page distinguishes archive and recycle-bin outcomes, so a later
adapter must verify the outcome on an independent surface rather than treating
row disappearance as deletion.

## Unlisted providers

For a provider not named above, §4’s route is an erasure request or manual
jump. No provider-specific automation claim is made without a fresh dated
primary-source entry in this ledger.
