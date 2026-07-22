# Scrub attachment capability contract

Scrub attachment parsing is local-only. The built-in scanner selects handlers
from bytes, expands archives only in memory, never executes attachment content,
and applies shared expansion, entry-count, recursion, and extracted-text caps.
This is a bounded memory-safe parser boundary; it is not an OS-level sandbox.

## Built in

- UTF-8 plain text, content-sniffed Markdown and CSV, and bounded RTF text
- PDF text layers through the pure-Rust `pdf-extract`/`lopdf` path
- DOCX and PPTX text through bounded OOXML ZIP/XML parsing
- XLSX cells through pure-Rust `calamine`
- recursive ZIP, TAR, and gzip processing with traversal and special-entry
  rejection
- explicit coverage records for encrypted, malformed, unsupported, over-limit,
  opaque, and model-dependent content

## Optional capability adapters

The Rust traits exist, but no production adapter is registered in this build.
Installing the following dependencies alone does **not** turn a flag on. A
future signed adapter/model manifest must verify executable and model hashes,
then register the matching trait implementation.

### Image explicit-content classifier

Proposed verified local pack:

- Python 3.11+
- `torch`, `transformers`, `Pillow`, and `safetensors`
- `Falconsai/nsfw_image_detection` at revision
  `ea798c4a93814025af5c7befb6cbf34757ecc7b4`
- accept only `model.safetensors` plus `config.json` and
  `preprocessor_config.json`; do not load pickle `.pt`/`.bin` weights
- expected `model.safetensors` SHA-256:
  `97b2ce64ec146884b37f98ee7944ca4891aa72f6827dc0cb10684a1cbecd5830`

This is a binary normal/NSFW signal, not a certainty or age classifier.

### OCR

- Tesseract 5.x executable
- `tessdata_best/eng.traineddata` for English, plus one explicit
  `.traineddata` file for every additional enabled language

The adapter must use fixed arguments, bounded image dimensions/time/output,
and feed successful OCR text back through the existing 11 text detectors.

### Video

- trusted `ffmpeg` and `ffprobe` 6.x or 7.x executables
- the verified image classifier pack above
- Tesseract 5.x and selected language data
- the speech-to-text pack below

The adapter must invoke fixed argument arrays without a shell, allow only local
file/pipe protocols, cap runtime/output/keyframes, and send keyframes through
the image classifier and OCR interfaces. Audio must be sent through STT.

### Speech to text

- `whisper-cli` built from `ggml-org/whisper.cpp`
- English default model `models/ggml-base.en.bin` (about 142 MiB), official
  SHA-1 `137c40403d78fd54d454da0f9bd998f78703390c`
- multilingual alternative `models/ggml-base.bin`, official SHA-1
  `465707469ff3a37a2b9b8d8f89f2f99de7299dac`

The adapter must pin the whisper.cpp source revision, verify the chosen model
hash, bound runtime/audio duration/output, and feed successful transcripts back
through the existing text detectors.
