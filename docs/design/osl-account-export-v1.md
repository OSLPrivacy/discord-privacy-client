# OSL portable account export format v1

Status: published, version 1 (`OSL-EXPORT-1`). The normative structural schema
is [`osl-account-export-v1.schema.json`](./osl-account-export-v1.schema.json).

An export consists of two independently saved files:

- `*.oslexport`, the encrypted archive;
- `*.key.json`, the person's separate key file.

OSL does not retain the key. Neither file depends on a running OSL service, a
device keystore, an account password, or an OSL-held secret after creation.

## Cryptography

The key file contains 32 random bytes as unpadded base64. The archive header
contains a fresh 32-byte salt. Readers derive a 32-byte archive key with
HKDF-SHA256 using info
`org.openstandardlibraries.account-export.archive-key.v1`.

Every export also has a fresh 16-byte nonce prefix. Block `i` uses the
XChaCha20-Poly1305-IETF nonce `prefix || uint64-big-endian(i)`. Associated data
is `SHA-256(exact header JSON) || uint64-big-endian(i) || kind-byte ||
uint32-big-endian(plaintext-length)`. Thus authentication binds the exact
header and the position, type, and length of every block. New key, salt,
archive id, and nonce prefix material are generated for every export.

Readers must validate the named KDF, AEAD, sizes, nonce construction, version,
key checksum, and archive-id binding before releasing plaintext. They must
authenticate all blocks and validate the full manifest in temporary storage
before releasing any plaintext. A wrong key, truncated frame, changed bit,
missing block, duplicated block, or reordered block is an integrity failure and
releases zero plaintext.

## Binary archive

All integers are unsigned big-endian.

| Field | Size |
|---|---:|
| magic `OSLXPORT` | 8 bytes |
| format version | 2 bytes |
| header JSON byte length | 4 bytes |
| UTF-8 header JSON | declared length |
| frame count | 8 bytes |
| frames | to EOF |

Each frame is `index:u64, kind:u8, plaintext_length:u32,
ciphertext_length:u32, ciphertext_and_16_byte_tag`. Indexes are exactly
contiguous from zero. Kind 0 is the one encrypted manifest; kind 1 is data.
Trailing bytes are forbidden.

The manifest lists all five production ownership classes—`identity_profile`,
`settings`, `friend_relationships`, `messages`, and `attachments`—including
zero counts. Every logical object has its complete byte count and SHA-256 and
an exact contiguous block set. Every data block has its class, object id,
offset, byte count, SHA-256, and final-block marker. Attachments additionally
carry owner, message, filename, and MIME metadata. Data blocks are at most
65,536 plaintext bytes. Production document and attachment enumeration uses
16-item pages and continues until an empty page; reaching 16 is never EOF.

## Successful save

The packaged client writes and synchronizes both native-dialog destinations,
closes them, reopens both final files, reads each to EOF, authenticates the
header, manifest, every data block and the final block, then compares the full
archive/key byte counts and authenticated-block set with the generated
manifest. Only that complete readback creates a success receipt.
