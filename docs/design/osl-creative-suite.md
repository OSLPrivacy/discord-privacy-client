# OSL Notes creative suite

## Non-negotiable product rules

OSL Notes is the local encrypted workspace for writing, data, visual design, media, and project files. No editor requires an account, subscription, cloud conversion endpoint, telemetry endpoint, or hosted AI service. Opening, autosaving, indexing, previewing, rendering, transcoding, importing, exporting, recovery, and collaboration history must work locally.

Project files, source media, previews, proxies, thumbnails, fonts added by the user, render caches, undo history, brushes, presets, color profiles, transcripts, OCR, and extension settings are authenticated-encrypted at rest under the active OSL identity. Temporary plaintext is memory-only where possible and placed in an identity-owned encrypted scratch area when a codec requires seekable temporary storage. Locks clear keys and derived previews from memory.

Public application code, extension manifests, public presets, and package signatures remain inspectable. Encryption is for user data, not for hiding executable code.

## Workspace families

### Notes and knowledge

Markdown, backlinks, block references, tasks, databases, graph views, clipping, offline search, templates, version history, and portable vault export.

### Documents and publishing

Paginated and continuous editing, styles, sections, headers/footers, columns, tables, footnotes, citations, change tracking, comments, mail merge, accessibility inspection, print/PDF production, master pages, preflight, bleed, and color-managed output.

### Spreadsheets and data

Typed cells, formulas, named ranges, sorting/filtering, frozen panes, pivot tables, charts, conditional formatting, validation, tables, query transforms, scripting through bounded data capabilities, and deterministic recalculation without remote functions.

### Drawings and vector design

Pen and node editing, shape builder, boolean operations, gradients, patterns, symbols/components, constraints, connectors, diagrams, typography on paths, artboards, SVG/PDF import/export, and nondestructive effects.

### Presentations

Master slides, layouts, themes, speaker notes, charts, media, transitions, animations, presenter view, rehearsal timing, recording, accessible reading order, PPTX/PDF/video import-export, and local multiplayer presentation over user-approved LAN sessions.

### Photo and raster composition

Layers, groups, masks, adjustment layers, blend modes, smart/nondestructive objects, selections, paths, clone/heal, content-aware tools, configurable brushes, RAW development, HDR, wide-gamut color, CMYK proofing, typography, batch actions, and local-only ML filters with visible model provenance and an off switch.

### Video, motion, and compositing

Multitrack timelines, nested sequences, proxies, multicam, trimming, transitions, keyframes, masks, tracking, stabilization, scopes, color grading, captions, motion graphics, chroma key, compositing graphs, hardware-aware render queues, and deterministic background exports.

### Audio and music

Waveform and spectral editing, multitrack recording, routing, automation, nondestructive effects, plugin chains, noise restoration, MIDI, score/notation views, loudness metering, markers, transcripts, and stems.

### 3D and interactive work

Mesh and curve modeling, sculpting, materials, UVs, rigging, animation, simulation, cameras/lights, compositing, local rendering, game-ready export, and a node system. Large caches use encrypted chunk storage with explicit quotas.

### Interface and prototype design

Components, variants, constraints, responsive frames, tokens, interactive prototypes, variables, accessibility annotations, developer handoff, design-system libraries, and local co-editing. Export must never be tied to a hosted design account.

## Import and export policy

The first implementation accepts bounded Markdown, text, CSV, TSV, inert HTML/RTF text, safe SVG primitives, and OSL interchange locally. The importer rejects scripts, remote references, external resources, oversized structures, and malformed project data. Lossy HTML, RTF, and SVG conversions preserve the byte-exact source in the encrypted asset vault and create a separate encrypted limitations receipt; Markdown, plain text, delimited data, and native OSL interchange remain direct portable imports.

The first native Office adapters now decode XLS/XLSX/XLSB/ODS worksheets, DOCX/ODT text, and PPTX/ODP slide text from bounded encrypted originals. Imports retain the immutable source and produce explicit loss receipts for layouts, charts, macros, media, comments, tracked changes, animations, and formulas outside the current deterministic subset. These are local in-process parsers with byte/structure limits, not an OS process sandbox; adversarial format fuzzing and subprocess isolation remain required before broad codec coverage.

Additional format adapters are staged behind regression corpora and the decoding broker:

- DOCX, ODT, RTF, EPUB, HTML, PDF;
- XLSX, ODS, legacy CSV dialects, Parquet, SQLite extracts;
- PPTX, ODP, Keynote export packages;
- PSD/PSB, TIFF, PNG, JPEG, WebP, AVIF, HEIF, OpenEXR, common RAW formats;
- AI/PDF/EPS/SVG, Affinity interchange where documented, Krita and GIMP projects;
- MP4/MOV/MKV/WebM, image sequences, project interchange such as OTIO/EDL/XML;
- WAV/FLAC/AIFF/MP3/Opus/MIDI and common plugin/preset interchange;
- glTF/GLB, OBJ, FBX through audited adapters, USD/USDZ, STL, PLY, Alembic.

Unsupported proprietary features are reported precisely. Imports preserve originals as encrypted immutable source assets and create derived project state; they never destructively rewrite the selected file. Exports require an explicit destination chosen by the user and never use a web conversion service.

## Encrypted local asset vault

Media work requires a storage layer separate from the bounded note document:

The first vault layer is implemented: user-selected binary files stream through bounded 256 KiB chunks into independently authenticated-encrypted files, with an encrypted manifest, sequential-write enforcement, resumable offsets, per-chunk SHA-256 integrity receipts, an 8 GiB per-asset ceiling, a 64 GiB initial vault quota, atomic recoverable writes, exact incomplete-upload quarantine/cancellation, and identity-slot cleanup. Imported binary formats receive a searchable encrypted project card while their original source bytes remain immutable in the vault. Completed chunks can be authenticated and decrypted into bounded memory-only previews; the CSP permits local `blob:` image/media previews while continuing to deny remote media, frames, object embeds, workers, and network connections. Encrypted reference links now keep shared sources until their final project is permanently deleted, and Office projects can discover/export their linked original through bounded in-memory export. Durable encrypted derived caches, streaming export for sources above the memory ceiling, larger configurable quotas, and additional editor-specific decoders remain required before the vault phase is complete.

Photo, video, audio, and 3D sources now receive distinct, versioned workspace types. Images have bounded nondestructive source/adjustment layers, ordering, visibility, opacity, blend modes, masks, and per-layer brightness/contrast/saturation/blur/rotation. Private PNG export re-renders decoded pixels into a fresh local canvas and therefore omits source EXIF, GPS, XMP, filenames, and container metadata; exact-original export remains a separate, explicitly labeled action that preserves the selected file byte-for-byte.

Audio and video projects now persist a bounded multitrack timeline with source clips, motion tracks, playhead, trim/source-in, duration, speed, volume, fades, mute, and track locks. Local media metadata supplies duration without a network decoder. 3D projects now persist a bounded scene graph with source meshes, cameras, lights, empties, transforms, visibility, locking, and material names. These are editing foundations; proxy generation, waveform/scope caches, frame-accurate rendering, mesh decoding, and production codec/export pipelines remain separate audited milestones.

Drawings now migrate into a bounded vector model with pen paths, independent fill/stroke, gradients, opacity, rotation, object naming, locking, visibility, layer order, multiple artboards, reusable source components and instances, responsive artboard constraints, and metadata-free standalone SVG output. The same authenticated-encrypted model now supports a bounded infinite viewport, zoom and pan, pointer-drawn ink/highlighting, and annotation surfaces linked to encrypted image/PDF attachments. Images decode locally from the encrypted vault. PDFs remain labelled page placeholders until a verified local decoder is present, and OCR remains unavailable rather than guessing or uploading text. Component source styling propagates locally to its instances without granting code execution or network access. The same model holds local prototype start points and bounded click/hover/key interactions with instant/dissolve/slide transitions. Documents now use a structured publishing model with page size, orientation, margins, columns, headers/footers, paragraphs, headings, quotes, checklists, tables, page breaks, alignment, paginated reading view, encrypted block comments, and bounded tracked text edits with accept/reject controls. Spreadsheets migrate to typed cell formats, filters, deterministic sorting, frozen rows/columns, bounded formulas, evaluated CSV, per-cell number/date/list validation, conditional rules, and accessible local bar/line/pie chart foundations. Presentations migrate to layouts, speaker notes, per-slide timing/transitions, duplication, master footer/slide-number settings, bounded title/body animation sequences, explicit accessible reading order, and an offline keyboard-operable presenter view with private notes.

Portable OSL exports are privacy-minimized by default: folder organization, tags, timestamps, device details, and internal identity data are omitted, and the importer accepts this stripped interchange without recreating private organization. Text, CSV, SVG, presentation HTML, and private PNG exports are generated locally and contain no telemetry or remote references.

1. Each imported source is streamed into encrypted, authenticated chunks with opaque per-identity identifiers.
2. Names, MIME types, dimensions, duration, codecs, color profiles, checksums, and relationships live only in encrypted project metadata.
3. Derived previews, waveforms, proxies, thumbnails, OCR, transcripts, and render caches are encrypted and revocable.
4. The vault maintains encrypted reference counts and an orphan quarantine before deletion.
5. Autosave uses append-only encrypted project operations plus periodic authenticated snapshots.
6. Scratch storage has hard quotas, crash recovery, secure cleanup, and no global temp-directory fallback.
7. Search indexes are rebuilt after unlock or stored as encrypted pages; no plaintext media index is durable.

## Modification and plugin surfaces

The SDK supports themes, templates, commands, importers, exporters, brushes, filter packs, codecs, render nodes, generators, panels, and workflow packs. A package receives only versioned opaque handles through a deny-by-default broker.

Creative capabilities include selected-asset reads, derived-asset creation, bounded render filters, import decoding, and export encoding. Packages never get ambient filesystem paths, sockets, browser sessions, identity keys, shell commands, process spawning, arbitrary native libraries, or cross-project data. Codec and native acceleration packs run in isolated workers with file-format fuzzing, memory/time budgets, crash quarantine, and reproducible signed builds.

Users can sideload signed packages, inspect manifests and sources, pin versions, export their setup, disable all modifications at startup, see performance impact, revoke grants per identity, and roll back updates. The free registry protocol permits mirrors and community curation without making one company the gatekeeper.

An implemented-but-unwired `.oslmod` prototype exists in isolated source: it parses an encrypted ZIP package with a strict manifest and a WebAssembly module exporting `osl_run(i64) -> i64`. Its native Wasmi path denies WASI and host imports, limits linear memory to 32 MiB and one memory/table/instance, and applies a deterministic two-million-fuel cutoff. The current desktop build does not declare the module, register its inspect/run commands, or call its UI adapter, so it exposes no executable plugin surface. Selected-note/asset brokers, signatures, audit logs, safe mode, rollback, and crash quarantine remain integration requirements.

## Sharing and collaboration

Direct encrypted LAN-room source exists, but LAN collaboration is not available in the current desktop build. The implemented-but-unwired prototype uses explicit local-network endpoints with a random 256-bit room secret, rejects public/global IP addresses, and authenticates sequence-bound frames; those source properties are not shipping product behavior. Conflict-copy handling remains part of the integration boundary.

Hosted relay, internet sync, and always-available rooms are optional Pro services. If LAN collaboration is wired, the free application must continue to open, edit, export, share by file, and collaborate over LAN without them. Enabling Pro does not opt a workspace into cloud storage; the user must explicitly create or move each hosted room, can return it to LAN/local-only mode, and can export all hosted state into the same portable encrypted formats.

## Research-led feature intake

Feature requests are maintained as a public, exportable local backlog with duplicate linking, evidence, accessibility impact, privacy impact, format portability, and performance cost. Popularity informs prioritization but cannot override encryption, accessibility, data ownership, or safe extension boundaries. Shipping decisions and rejected requests receive a concrete rationale instead of disappearing into a vote counter.

Current community research reinforces five delivery priorities: familiar workflows over novelty, dependable round-trip format compatibility, a simple mode that does not hide professional depth, responsive performance on ordinary hardware, and user control over accounts, subscriptions, telemetry, and AI. Nondestructive editing and reusable procedural graphs should be shared foundations instead of separate implementations in each creative tool. AI-assisted features, when added, must be local, optional, clearly labeled, replaceable, and absent from deterministic exports unless the user invokes them.

## Release order

1. Finish dependable Docs/Sheets/Drawings/Slides editing, undo/version history, and portable imports with round-trip regression corpora.
2. Land the encrypted chunked asset vault, undo log, render worker, and crash recovery.
3. Ship raster/vector editors and the audited image pipeline.
4. Ship audio/video timelines on the same asset/render foundation.
5. Add publishing, prototyping, 3D, and local collaboration.
6. Enable executable creative extensions only after sandbox, permissions, safe mode, audit logs, quotas, signing, rollback, and revocation are independently verified.
