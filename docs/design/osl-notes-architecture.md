# OSL Notes architecture and delivery map

## Product contract

OSL Notes is a free, local-first knowledge and creative workspace inside OSL. The default experience is useful without plugins or cloud accounts: fast capture, Markdown editing and preview, documents, spreadsheets, drawings, presentations, folders, tags, favorites, full-text search, wiki links, backlinks, local imports, and recoverable Trash. The larger encrypted media-editor program is specified in `osl-creative-suite.md`.

The current desktop build is deliberately honest. It does not claim OneNote/Obsidian/Office/Adobe parity. It does include encrypted notes and revision history, folders/tags/favorites/Trash, Markdown and wiki links/backlinks, encrypted note attachments, portable typed properties with table/board/calendar views, a bounded knowledge graph, a cross-note task dashboard, templates, local import/export, structured Docs/Sheets/Drawings/Slides editors, encrypted infinite canvases with pointer ink and highlights, encrypted image annotation surfaces, labelled PDF annotation placeholders, an encrypted source-asset vault, bounded creative workspace foundations, pure sandboxed command extensions, and encrypted same-LAN collaboration. OCR, decoded PDF page rendering, property formulas/queries, mobile clients, production media codecs/rendering, richer Office round trips, and hosted collaboration remain delivery tracks below.

## Encryption boundary

All user-derived state is private and must be authenticated-encrypted with the active identity's file-storage key before it reaches durable storage:

- note titles, bodies, folders, tags, favorites, links, backlinks, Trash, and history;
- attachment names, bytes, thumbnails, OCR text, and extracted metadata;
- canvas nodes, positions, embeds, handwriting, and transcriptions;
- search/index tokens and caches, unless rebuilt only in memory after unlock;
- template use, installed-extension state, permission grants, settings, private extension data, audit records, and sync metadata.

Notes stores bounded identity-scoped encrypted documents and revisions through OSL's authenticated at-rest primitive and recoverable atomic writer. Large imported sources use independently authenticated encrypted chunks and an encrypted manifest. The implementation refuses plaintext legacy files, requires an unlocked identity, and includes Notes artifacts in account cleanup.

Public extension packages, manifests, signatures, and documentation remain inspectable. They are not secrets. The user's installation state, configuration, grants, and content are encrypted. Future sharing and sync must be end-to-end encrypted with no plaintext fallback; server-side search is therefore excluded.

## Extension model

OSL Notes will not copy the ambient Node/Electron authority common in desktop note-app ecosystems. A package never inherits Tauri commands, filesystem access, process execution, secrets, network access, browser sessions, or another identity.

### Package types

1. Themes: allowlisted semantic design tokens only. No arbitrary remote imports, URLs, scripts, layout-breaking selectors, or hidden UI.
2. Templates: bounded Markdown plus typed placeholders. They can create a note only after a user invokes them.
3. Command packs: a declarative action graph over fixed host operations. No eval, shell, raw IPC, generic HTTP, or generic filesystem primitive.
4. Sandboxed extensions: the first executable surface is a pure WebAssembly command ABI with no WASI, imports, filesystem, process, browser-session, secret, or network authority. Broader selected-document capabilities remain disabled until their broker and approval receipts exist.

### Capability vocabulary in the first SDK contract

- `notes:read-selected`: read only notes explicitly handed to the extension.
- `notes:create`: create a note after a user-invoked action.
- `notes:update-selected`: propose an edit to the open note; host confirmation applies it.
- `ui:theme`: set allowlisted Notes tokens.
- `ui:command`: register commands in the host-owned menu.

Grants are per identity and per exact package version, encrypted, reviewable, revocable, and invalidated by a material manifest change. Every sensitive operation is mediated by the host, size/time bounded, and attributable in an encrypted audit log. A global kill switch starts Notes with every third-party extension disabled.

Executable `.oslmod` command installation is limited to a strict manifest and a pure `osl_run(i64) -> i64` WebAssembly export in Wasmi, with no host imports, a 32 MiB memory ceiling, and a deterministic fuel cutoff. Package signing, reproducible-build verification, permissioned document/asset brokers, safe mode, rollback, crash attribution, and emergency revocation are still required before granting broader extension capabilities. A permission dialog alone is not a sandbox.

## Data evolution

The encrypted repository is versioned and bounded to 5,000 notes, 256 KiB per body, and 64 MiB for note records. Encrypted source assets are chunked separately with explicit per-asset and vault quotas. Remaining storage work includes:

- append-only encrypted change records for crash safety and history;
- encrypted attachment blobs with content-derived IDs calculated before encryption but never exposed remotely;
- an in-memory search index rebuilt after unlock, or encrypted index pages with padded access patterns;
- schema migrations that authenticate the old format, write a new artifact, verify it, and retain recoverable rollback until the next successful launch.

## Delivery tracks

### Track A — dependable daily notes

- keyboard shortcuts for new/save/edit/preview/graph/tasks, a searchable command palette, encrypted pinning, and slash-delimited nested folder navigation are implemented; durable encrypted saved searches remain;
- note previews now include a bounded inert Markdown subset for portable tables, callouts, inline math notation, language-labelled code fences, ordered lists, highlighting, and strikethrough; adopting and auditing a complete CommonMark engine still remains;
- recurring Markdown tasks support exact date-only day/week/month/year rules, preserve completed occurrences, deterministically clamp month/year ends, and create at most one next occurrence after completion;
- import/export for Markdown folders, Obsidian vaults, OneNote exports where available, and standard archives;
- local version history now includes inspectable encrypted snapshots, conflict-free undo, and encrypted attachment linking/removal; image crop/compression and attachment-aware historical restoration remain;
- accessibility, reduced motion, screen-reader landmarks, large-vault profiling, and instant capture.

### Track B — knowledge tools

- aliases, block links, embeds, transclusion, graph and local graph;
- portable typed frontmatter properties with local tables, boards, and calendars are implemented; saved database queries, formulas, relations, rollups, and richer view configuration remain;
- mind maps, decoded PDF page rendering, OCR, and audio transcription (the encrypted infinite-canvas, ink/highlighter, image annotation, and PDF-placeholder foundations are implemented);
- portable vault diagnostics that identify extension dependencies and export plain formats.

### Track C — encrypted multi-device and collaboration

- device keys and explicit device approval; encrypted change replication and attachment chunks;
- local merge with transparent conflicts, durable deletion tombstones, key rotation, and remote-device revocation;
- shared notebooks using member-scoped keys, verified identities, granular roles, and auditable membership changes;
- free direct LAN rooms are implemented with encrypted, sequence-bound frames, private-address enforcement, explicit invitations, and recoverable conflict copies;
- optional hosted internet rooms remain Pro-only and opt-in. No file is uploaded merely because a user has Pro.

### Track D — developer platform

- documented manifest schema, typed SDK, local simulator, permission linter, test fixtures, package signer, reproducible build verifier;
- encrypted extension storage quotas, settings schemas, command/context APIs, theme inspector;
- extension performance profiler, crash quarantine, compatibility matrix, update rollback, and safe-mode recovery;
- free public registry protocol with mirrors; curated discovery is optional and does not gate sideloaded signed packages.

## Acceptance gates

No feature is “done” until it has format validation, bounded resource use, identity isolation, encrypted persistence tests, corrupt/tampered input tests, account-cleanup coverage, keyboard and screen-reader QA, crash recovery, and truthful UI copy. Networked features additionally require protocol threat modeling, key-rotation/revocation tests, metadata disclosure documentation, and an offline/no-account path. Exports default to regenerated, metadata-minimized files; byte-exact originals are a separate explicit action because they may retain source metadata.
