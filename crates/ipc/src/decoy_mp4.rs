//! Decoy containers.
//!
//! Two unrelated decoys live here, and conflating them is the mistake this
//! header exists to prevent:
//!
//! 1. **The legacy wire decoy** ([`decoy_mp4`]) — a 16x16 MP4 that
//!    `seal_attachment_v3` prepends to OSL ciphertext so the *sealed object*
//!    is a structurally-valid MP4. Ciphertext lives in the cipher store, not
//!    on any platform CDN, so this decoy's only remaining job is keeping the
//!    V3 wire format stable for objects already sealed. It is **not** what
//!    OSL places on Discord and its bytes never traverse Discord.
//! 2. **The layout decoy** ([`layout_decoy_png`]) — a payload-free image
//!    whose only purpose is to make Discord allocate and lay out a media row.
//!    It carries no ciphertext, no key material, and nothing derived from the
//!    plaintext beyond a 4-way aspect bucket. See the section at the bottom of
//!    this file.
//!
//! ## Phase 8e: minimal decoy MP4 container
//!
//! Built dynamically per the spec's Option B (ffmpeg-free fallback)
//! and cached via [`OnceLock`]. Produces a structurally-valid ISO/IEC
//! 14496-12 file declaring a 16×16 H.264 baseline video track. The
//! avcC sample descriptor embeds real SPS + PPS NAL units; the
//! sample table is zero-length so the actual frame is empty. Discord
//! treats the file as `video/mp4` (not transcoded) and renders a
//! video-card preview surface — the preview frame itself will fail
//! to load (no samples) but the visual category is "media" rather
//! than "generic binary file".
//!
//! ## What's *not* in this implementation
//!
//! - **No decoded frame.** A truly-playable hand-crafted single-frame
//!   I_PCM MP4 was scoped out because precise H.264 bit-packing is
//!   error-prone without a video-toolchain to validate against. The
//!   container is valid; the bitstream is empty. Phase-8e+ work item:
//!   swap this decoy for an ffmpeg-baked `decoy.mp4` asset when the
//!   project gains a video-encode dev dep or pre-bake step.
//! - **No `mdat` content.** mdat is an empty 8-byte box header. With
//!   zero samples declared in `stsz`/`stco`, no MP4 parser ever needs
//!   to look inside it.
//!
//! ## Wire role
//!
//! `seal_attachment_v3` (in [`crate::attachment_wire`]) appends a
//! `free` box carrying the OSL payload AFTER the decoy bytes. Free
//! boxes are ignorable per the ISO spec, so MP4 parsers walk past
//! them without complaining.
//!
//! ## Correction: the "Discord preserves trailing bytes" claim is retired
//!
//! An earlier revision of this comment asserted that Discord's CDN
//! "preserves the trailing bytes verbatim (octet-stream-style; no
//! transcoding)". That claim was never verified against Discord and it is
//! **no longer relied upon anywhere**. The sealed object is uploaded to
//! OSL's own cipher store, so this `free` box only ever round-trips through
//! storage OSL controls. Nothing in OSL requires Discord to preserve any
//! byte of any file. If a future change wants to put ciphertext on a
//! platform CDN, that claim must be re-established first — it is not
//! established here, and the deletion-control argument (a platform CDN blob
//! is outside the cipher store's TTL and outside burn) rules it out anyway.

use std::sync::OnceLock;

static DECOY_MP4: OnceLock<Vec<u8>> = OnceLock::new();

/// Public accessor — cached after first call.
pub fn decoy_mp4() -> &'static [u8] {
    DECOY_MP4.get_or_init(build_decoy_mp4)
}

/// Minimal SPS NAL unit for a 16×16 baseline level-1.0 H.264 stream.
///
/// Byte layout:
///
/// - `0x67`: NAL header (forbidden_zero_bit=0, nal_ref_idc=3,
///   nal_unit_type=7 = SPS).
/// - `0x42`: profile_idc = 66 (baseline).
/// - `0xC0`: constraint_set0_flag + constraint_set1_flag, reserved 0.
/// - `0x0A`: level_idc = 10 (level 1.0).
/// - `0xF4`: bit-packed `seq_parameter_set_id=ue(0)`,
///   `log2_max_frame_num_minus4=ue(0)`, `pic_order_cnt_type=ue(0)`,
///   `log2_max_pic_order_cnt_lsb_minus4=ue(0)`, `num_ref_frames=ue(1)`,
///   `gaps_in_frame_num_value_allowed_flag=0`.
/// - `0xE2`: bit-packed `pic_width_in_mbs_minus1=ue(0)` (16-px wide),
///   `pic_height_in_map_units_minus1=ue(0)` (16-px tall),
///   `frame_mbs_only_flag=1`, `direct_8x8_inference_flag=0`,
///   `frame_cropping_flag=0`, `vui_parameters_present_flag=0`,
///   `rbsp_trailing_bits=10000000` aligning to byte.
const SPS: &[u8] = &[0x67, 0x42, 0xC0, 0x0A, 0xF4, 0xE2];

/// Minimal PPS NAL unit. NAL header `0x68` (nal_unit_type=8 = PPS),
/// then bit-packed defaults: `pic_parameter_set_id=ue(0)`,
/// `seq_parameter_set_id=ue(0)`, CAVLC, no slice groups, default
/// reference indices, no weighted prediction, qp deltas all zero,
/// flags off, `rbsp_trailing_bits` aligning to byte.
const PPS: &[u8] = &[0x68, 0xCE, 0x38, 0x80];

fn build_decoy_mp4() -> Vec<u8> {
    let mut out = Vec::with_capacity(600);
    write_ftyp(&mut out);
    write_moov(&mut out);
    write_mdat(&mut out);
    out
}

fn write_box(out: &mut Vec<u8>, box_type: &[u8; 4], body: impl FnOnce(&mut Vec<u8>)) {
    let start = out.len();
    out.extend_from_slice(&[0, 0, 0, 0]); // size placeholder
    out.extend_from_slice(box_type);
    body(out);
    let size = (out.len() - start) as u32;
    out[start..start + 4].copy_from_slice(&size.to_be_bytes());
}

fn write_ftyp(out: &mut Vec<u8>) {
    write_box(out, b"ftyp", |o| {
        o.extend_from_slice(b"isom"); // major_brand
        o.extend_from_slice(&0x200u32.to_be_bytes()); // minor_version
        o.extend_from_slice(b"isom"); // compatible_brands[0]
        o.extend_from_slice(b"avc1");
        o.extend_from_slice(b"mp41");
    });
}

fn write_moov(out: &mut Vec<u8>) {
    write_box(out, b"moov", |o| {
        write_mvhd(o);
        write_trak(o);
    });
}

fn write_mvhd(out: &mut Vec<u8>) {
    write_box(out, b"mvhd", |o| {
        o.extend_from_slice(&[0, 0, 0, 0]); // version=0, flags=0
        o.extend_from_slice(&0u32.to_be_bytes()); // creation_time
        o.extend_from_slice(&0u32.to_be_bytes()); // modification_time
        o.extend_from_slice(&1000u32.to_be_bytes()); // timescale = 1000
        o.extend_from_slice(&1000u32.to_be_bytes()); // duration = 1 second
        o.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // rate = 1.0
        o.extend_from_slice(&0x0100u16.to_be_bytes()); // volume = 1.0
        o.extend_from_slice(&0u16.to_be_bytes()); // reserved
        o.extend_from_slice(&[0; 8]); // reserved
        write_unity_matrix(o);
        o.extend_from_slice(&[0; 24]); // pre_defined
        o.extend_from_slice(&2u32.to_be_bytes()); // next_track_ID
    });
}

fn write_trak(out: &mut Vec<u8>) {
    write_box(out, b"trak", |o| {
        write_tkhd(o);
        write_mdia(o);
    });
}

fn write_tkhd(out: &mut Vec<u8>) {
    write_box(out, b"tkhd", |o| {
        // version=0, flags=0x000007 (enabled+in_movie+in_preview).
        o.extend_from_slice(&[0, 0, 0, 0x07]);
        o.extend_from_slice(&0u32.to_be_bytes()); // creation_time
        o.extend_from_slice(&0u32.to_be_bytes()); // modification_time
        o.extend_from_slice(&1u32.to_be_bytes()); // track_ID = 1
        o.extend_from_slice(&[0; 4]); // reserved
        o.extend_from_slice(&1000u32.to_be_bytes()); // duration
        o.extend_from_slice(&[0; 8]); // reserved
        o.extend_from_slice(&0u16.to_be_bytes()); // layer
        o.extend_from_slice(&0u16.to_be_bytes()); // alternate_group
        o.extend_from_slice(&0u16.to_be_bytes()); // volume (video=0)
        o.extend_from_slice(&0u16.to_be_bytes()); // reserved
        write_unity_matrix(o);
        // width / height as 16.16 fixed-point: 16x16.
        o.extend_from_slice(&0x0010_0000u32.to_be_bytes());
        o.extend_from_slice(&0x0010_0000u32.to_be_bytes());
    });
}

fn write_unity_matrix(out: &mut Vec<u8>) {
    // 3x3 affine, identity:
    //   1.0  0    0
    //   0    1.0  0
    //   0    0    1.0
    // Top-left 2x2 is 16.16 fixed; right column is 2.30 fixed.
    let m: [u32; 9] = [0x0001_0000, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000];
    for v in m {
        out.extend_from_slice(&v.to_be_bytes());
    }
}

fn write_mdia(out: &mut Vec<u8>) {
    write_box(out, b"mdia", |o| {
        write_mdhd(o);
        write_hdlr(o);
        write_minf(o);
    });
}

fn write_mdhd(out: &mut Vec<u8>) {
    write_box(out, b"mdhd", |o| {
        o.extend_from_slice(&[0, 0, 0, 0]); // version=0, flags=0
        o.extend_from_slice(&0u32.to_be_bytes()); // creation_time
        o.extend_from_slice(&0u32.to_be_bytes()); // modification_time
        o.extend_from_slice(&1000u32.to_be_bytes()); // timescale
        o.extend_from_slice(&1000u32.to_be_bytes()); // duration
                                                     // language = "und" packed 5-5-5 (each char - 0x60).
                                                     // 'u'=21, 'n'=14, 'd'=4 → 0b0_10101_01110_00100 = 0x55C4
        o.extend_from_slice(&0x55C4u16.to_be_bytes());
        o.extend_from_slice(&0u16.to_be_bytes()); // pre_defined
    });
}

fn write_hdlr(out: &mut Vec<u8>) {
    write_box(out, b"hdlr", |o| {
        o.extend_from_slice(&[0, 0, 0, 0]); // version=0, flags=0
        o.extend_from_slice(&0u32.to_be_bytes()); // pre_defined
        o.extend_from_slice(b"vide"); // handler_type
        o.extend_from_slice(&[0; 12]); // reserved
                                       // Null-terminated name string. Empty.
        o.push(0);
    });
}

fn write_minf(out: &mut Vec<u8>) {
    write_box(out, b"minf", |o| {
        write_vmhd(o);
        write_dinf(o);
        write_stbl(o);
    });
}

fn write_vmhd(out: &mut Vec<u8>) {
    write_box(out, b"vmhd", |o| {
        // version=0, flags=1 (no_lean_ahead).
        o.extend_from_slice(&[0, 0, 0, 1]);
        o.extend_from_slice(&0u16.to_be_bytes()); // graphicsmode = copy
        o.extend_from_slice(&[0; 6]); // opcolor RGB (0,0,0)
    });
}

fn write_dinf(out: &mut Vec<u8>) {
    write_box(out, b"dinf", |o| {
        write_box(o, b"dref", |o| {
            o.extend_from_slice(&[0, 0, 0, 0]); // version=0, flags=0
            o.extend_from_slice(&1u32.to_be_bytes()); // entry_count
                                                      // url box: 12 bytes (header + flags=self-contained).
            write_box(o, b"url ", |o| {
                o.extend_from_slice(&[0, 0, 0, 1]); // version=0, flags=1
            });
        });
    });
}

fn write_stbl(out: &mut Vec<u8>) {
    write_box(out, b"stbl", |o| {
        write_stsd(o);
        write_empty_full_box(o, b"stts");
        write_empty_full_box(o, b"stsc");
        // stsz needs a sample_size field before entry_count.
        write_box(o, b"stsz", |o| {
            o.extend_from_slice(&[0, 0, 0, 0]); // version=0, flags=0
            o.extend_from_slice(&0u32.to_be_bytes()); // sample_size = 0 (variable)
            o.extend_from_slice(&0u32.to_be_bytes()); // sample_count = 0
        });
        write_empty_full_box(o, b"stco");
    });
}

fn write_empty_full_box(out: &mut Vec<u8>, box_type: &[u8; 4]) {
    write_box(out, box_type, |o| {
        o.extend_from_slice(&[0, 0, 0, 0]); // version=0, flags=0
        o.extend_from_slice(&0u32.to_be_bytes()); // entry_count = 0
    });
}

fn write_stsd(out: &mut Vec<u8>) {
    write_box(out, b"stsd", |o| {
        o.extend_from_slice(&[0, 0, 0, 0]); // version=0, flags=0
        o.extend_from_slice(&1u32.to_be_bytes()); // entry_count = 1
        write_avc1(o);
    });
}

fn write_avc1(out: &mut Vec<u8>) {
    write_box(out, b"avc1", |o| {
        // VisualSampleEntry header (78 bytes total including the 6
        // SampleEntry-reserved bytes that lead it).
        o.extend_from_slice(&[0; 6]); // reserved
        o.extend_from_slice(&1u16.to_be_bytes()); // data_reference_index
        o.extend_from_slice(&0u16.to_be_bytes()); // pre_defined
        o.extend_from_slice(&0u16.to_be_bytes()); // reserved
        o.extend_from_slice(&[0; 12]); // pre_defined
        o.extend_from_slice(&16u16.to_be_bytes()); // width
        o.extend_from_slice(&16u16.to_be_bytes()); // height
        o.extend_from_slice(&0x0048_0000u32.to_be_bytes()); // horizresolution = 72
        o.extend_from_slice(&0x0048_0000u32.to_be_bytes()); // vertresolution = 72
        o.extend_from_slice(&[0; 4]); // reserved
        o.extend_from_slice(&1u16.to_be_bytes()); // frame_count
        o.extend_from_slice(&[0; 32]); // compressorname (empty padded)
        o.extend_from_slice(&0x0018u16.to_be_bytes()); // depth
        o.extend_from_slice(&0xFFFFu16.to_be_bytes()); // pre_defined = -1
        write_avcc(o);
    });
}

fn write_avcc(out: &mut Vec<u8>) {
    write_box(out, b"avcC", |o| {
        o.push(0x01); // configurationVersion
        o.push(SPS[1]); // AVCProfileIndication (profile_idc)
        o.push(SPS[2]); // profile_compatibility
        o.push(SPS[3]); // AVCLevelIndication (level_idc)
        o.push(0xFF); // 6 bits reserved (111111) | lengthSizeMinusOne=3 (11)
        o.push(0xE1); // 3 bits reserved (111) | numOfSequenceParameterSets=1 (00001)
        o.extend_from_slice(&(SPS.len() as u16).to_be_bytes());
        o.extend_from_slice(SPS);
        o.push(0x01); // numOfPictureParameterSets
        o.extend_from_slice(&(PPS.len() as u16).to_be_bytes());
        o.extend_from_slice(PPS);
    });
}

fn write_mdat(out: &mut Vec<u8>) {
    write_box(out, b"mdat", |_| {});
}

/// Walk the top-level boxes of a (presumed-valid) MP4 file and return
/// each `(type, range)` pair. Used by [`crate::attachment_wire`] to
/// place a `free` box after the last top-level box so the decoy
/// remains a parser-clean container. Tolerates trailing garbage by
/// stopping at the first malformed box header.
pub fn iter_top_level_boxes(file: &[u8]) -> Vec<([u8; 4], std::ops::Range<usize>)> {
    let mut out = Vec::new();
    let mut p = 0;
    while p + 8 <= file.len() {
        let size = u32::from_be_bytes([file[p], file[p + 1], file[p + 2], file[p + 3]]) as usize;
        if size < 8 || p + size > file.len() {
            break;
        }
        let mut box_type = [0u8; 4];
        box_type.copy_from_slice(&file[p + 4..p + 8]);
        out.push((box_type, p..p + size));
        p += size;
    }
    out
}

// ---------------------------------------------------------------------------
// Layout decoy
// ---------------------------------------------------------------------------
//
// The problem this solves: Discord renders a cover message as a text row of a
// height Discord alone decides. A decrypted image is far taller than that row,
// so painting it over the row occludes the row's neighbours and is wrong at
// every scroll offset. Something has to *allocate* the vertical space, and the
// only thing that can allocate space inside Discord is Discord.
//
// So OSL gives Discord a file to lay out. That file is payload-free: the
// ciphertext stays in the cipher store, where OSL keeps deletion control and
// burn still means something. The decoy exists only so Discord computes a
// media rect, which OSL then *measures* and paints over.
//
// Three properties make the decoy safe to hand to a platform:
//
// * **It cannot leak the image.** [`layout_decoy_png`] takes a
//   [`LayoutDecoyBucket`] and nothing else. There is no parameter through
//   which plaintext, dimensions, filename or key material could reach it.
//   That is a type-level guarantee, not a review convention.
// * **It leaks ~2 bits.** The bucket is one of four, chosen from the image's
//   aspect ratio alone by [`LayoutDecoyBucket::for_dimensions`], which clamps
//   rather than extends: a 20:1 panorama and a 3:1 banner both land in `Wide`.
// * **Its bytes differ every send.** The pixel field is random, so the decoy
//   has no fixed hash or fixed length-for-bucket to fingerprint on.
//
// The decoy's own dimensions are deliberately *not* treated as the geometry
// OSL paints into. If Discord clamps, scales, re-encodes or re-boxes it, that
// changes nothing: the paint rect is measured live from Discord's own
// accessibility tree every time it is used.

/// Aspect bucket for the layout decoy. Four values, so a Discord row discloses
/// about two bits of the protected image's shape and nothing else — not its
/// dimensions, not its size, not its content.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutDecoyBucket {
    /// Wider than about 16:9.
    Wide,
    /// Between about 16:9 and about 6:5.
    Landscape,
    /// Between about 6:5 and about 5:6.
    Square,
    /// Taller than about 5:6, including everything more extreme.
    Portrait,
}

/// Decoy pixel dimensions per bucket.
///
/// Chosen at or below the largest media box Discord has historically laid out
/// so the common case needs no downscale. Nothing depends on that being right:
/// a clamp on Discord's side changes the rect OSL measures, and the measured
/// rect is the only geometry that is ever painted into.
const WIDE_SIZE: (u32, u32) = (550, 232);
const LANDSCAPE_SIZE: (u32, u32) = (466, 350);
const SQUARE_SIZE: (u32, u32) = (350, 350);
const PORTRAIT_SIZE: (u32, u32) = (262, 350);

/// Largest image edge the bucket chooser will accept. Beyond this the caller is
/// not describing a real picture and gets no bucket at all.
const MAX_SOURCE_EDGE: u32 = 1 << 16;

impl LayoutDecoyBucket {
    /// Bucket for a decoded image's pixel dimensions.
    ///
    /// Returns `None` for degenerate input so a caller can fail closed rather
    /// than place a row for an image it could not measure. Every non-degenerate
    /// ratio maps into one of the four buckets by clamping, so no extra bit ever
    /// escapes through an "unbucketable" case.
    pub fn for_dimensions(width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 || width > MAX_SOURCE_EDGE || height > MAX_SOURCE_EDGE {
            return None;
        }
        // Integer comparison of width/height against the boundary ratios, so
        // there is no float rounding at a bucket edge.
        let (w, h) = (u64::from(width), u64::from(height));
        Some(if w * 9 > h * 16 {
            Self::Wide
        } else if w * 5 > h * 6 {
            Self::Landscape
        } else if w * 6 >= h * 5 {
            Self::Square
        } else {
            Self::Portrait
        })
    }

    /// Pixel dimensions of the decoy this bucket produces.
    pub const fn dimensions(self) -> (u32, u32) {
        match self {
            Self::Wide => WIDE_SIZE,
            Self::Landscape => LANDSCAPE_SIZE,
            Self::Square => SQUARE_SIZE,
            Self::Portrait => PORTRAIT_SIZE,
        }
    }

    /// Stable, non-secret label for the operator-facing payload preview.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Wide => "wide",
            Self::Landscape => "landscape",
            Self::Square => "square",
            Self::Portrait => "portrait",
        }
    }
}

/// Palette entries for the decoy: two greys one step apart.
///
/// One step is below any practical perceptual threshold, so a reader who
/// reveals the spoiler sees flat grey rather than static, while the pixel
/// field underneath is still one random bit per pixel and therefore
/// incompressible and different on every send.
const DECOY_PALETTE: [[u8; 3]; 2] = [[0x8A, 0x8A, 0x8A], [0x8B, 0x8B, 0x8B]];

/// Build the payload-free layout decoy for one bucket.
///
/// The signature is the security argument: there is no parameter through which
/// the protected image, its filename, its size or any key material could reach
/// this function, so the returned bytes cannot encode them. The only input is
/// the bucket, and the only variable content is CSPRNG output.
pub fn layout_decoy_png(bucket: LayoutDecoyBucket) -> Vec<u8> {
    let (width, height) = bucket.dimensions();
    // 1 bit per pixel, so one row is ceil(width / 8) bytes behind a PNG filter
    // byte. `width` is a compile-time constant per bucket and far below any
    // overflow boundary.
    let row_bytes = width.div_ceil(8) as usize;
    let stride = row_bytes + 1;
    let raw_len = stride * height as usize;

    let mut raw = Vec::with_capacity(raw_len);
    let noise = crypto::random::random_bytes(row_bytes * height as usize);
    for row in 0..height as usize {
        raw.push(0u8); // filter type 0 (None) — nothing to predict, nothing to gain
        raw.extend_from_slice(&noise[row * row_bytes..(row + 1) * row_bytes]);
    }
    debug_assert_eq!(raw.len(), raw_len);

    let mut png = Vec::with_capacity(raw_len + 128);
    png.extend_from_slice(&PNG_SIGNATURE);
    write_png_chunk(&mut png, b"IHDR", |o| {
        o.extend_from_slice(&width.to_be_bytes());
        o.extend_from_slice(&height.to_be_bytes());
        o.push(1); // bit depth
        o.push(3); // colour type 3 = palette
        o.push(0); // compression method (deflate)
        o.push(0); // filter method
        o.push(0); // interlace method (none)
    });
    write_png_chunk(&mut png, b"PLTE", |o| {
        for entry in DECOY_PALETTE {
            o.extend_from_slice(&entry);
        }
    });
    write_png_chunk(&mut png, b"IDAT", |o| write_stored_zlib(o, &raw));
    write_png_chunk(&mut png, b"IEND", |_| {});
    png
}

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

fn write_png_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], body: impl FnOnce(&mut Vec<u8>)) {
    let mut payload = Vec::new();
    payload.extend_from_slice(chunk_type);
    body(&mut payload);
    // Length counts the data only, never the type or the CRC.
    let data_len = (payload.len() - 4) as u32;
    out.extend_from_slice(&data_len.to_be_bytes());
    let crc = crc32(&payload);
    out.extend_from_slice(&payload);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// A zlib stream of deflate *stored* blocks.
///
/// Deliberately uncompressed. The pixel field is one random bit per pixel, so
/// it is incompressible by construction and a real deflate would spend CPU to
/// grow it. Stored blocks also keep this file free of a compression dependency
/// and keep the decoy's length an exact function of its bucket plus framing.
fn write_stored_zlib(out: &mut Vec<u8>, raw: &[u8]) {
    // CMF=0x78 (deflate, 32 KiB window), FLG=0x01: no preset dictionary and
    // (0x78 << 8 | 0x01) % 31 == 0, which is the header check zlib requires.
    out.push(0x78);
    out.push(0x01);
    let mut offset = 0usize;
    // An empty input still needs one final (empty) stored block, or the stream
    // has no end and no decoder will accept it.
    loop {
        let remaining = raw.len() - offset;
        let take = remaining.min(u16::MAX as usize);
        let final_block = offset + take == raw.len();
        out.push(u8::from(final_block)); // BFINAL, BTYPE=00 (stored)
        out.extend_from_slice(&(take as u16).to_le_bytes());
        out.extend_from_slice(&(!(take as u16)).to_le_bytes());
        out.extend_from_slice(&raw[offset..offset + take]);
        offset += take;
        if final_block {
            break;
        }
    }
    out.extend_from_slice(&adler32(raw).to_be_bytes());
}

fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for byte in data {
        a = (a + u32::from(*byte)) % MOD;
        b = (b + a) % MOD;
    }
    (b << 16) | a
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoy_starts_with_ftyp() {
        let d = decoy_mp4();
        assert!(d.len() >= 28);
        // size BE u32 at offset 0
        let size = u32::from_be_bytes([d[0], d[1], d[2], d[3]]) as usize;
        assert!((24..=64).contains(&size));
        assert_eq!(&d[4..8], b"ftyp");
        assert_eq!(&d[8..12], b"isom");
    }

    #[test]
    fn decoy_box_structure_parses() {
        let d = decoy_mp4();
        let boxes = iter_top_level_boxes(d);
        let types: Vec<&[u8; 4]> = boxes.iter().map(|(t, _)| t).collect();
        assert_eq!(types, vec![b"ftyp", b"moov", b"mdat"]);
        // Combined sizes equal full length: no gaps.
        let total: usize = boxes.iter().map(|(_, r)| r.end - r.start).sum();
        assert_eq!(total, d.len());
    }

    #[test]
    fn decoy_contains_avcc_with_sps_pps() {
        let d = decoy_mp4();
        // Brute-force: avcC tag should be present somewhere in moov.
        let avcc_off = d
            .windows(4)
            .position(|w| w == b"avcC")
            .expect("avcC tag not found");
        // After "avcC", the avcC body starts. First byte = configurationVersion = 0x01.
        assert_eq!(d[avcc_off + 4], 0x01);
        // SPS bytes should appear after the standard avcC prefix.
        // No need for exact offset — find SPS by content scan.
        assert!(
            d.windows(SPS.len()).any(|w| w == SPS),
            "SPS bytes not found in decoy"
        );
        assert!(
            d.windows(PPS.len()).any(|w| w == PPS),
            "PPS bytes not found in decoy"
        );
    }

    #[test]
    fn decoy_is_reasonably_small() {
        let d = decoy_mp4();
        // Ballpark: ~600 bytes. Guard against accidental bloat.
        assert!(
            d.len() < 1024,
            "decoy unexpectedly large: {} bytes",
            d.len()
        );
    }

    #[test]
    fn decoy_cached_across_calls() {
        let a = decoy_mp4().as_ptr();
        let b = decoy_mp4().as_ptr();
        assert_eq!(a, b, "OnceLock should return the same slice on every call");
    }

    // ---------------- layout decoy ----------------

    const ALL_BUCKETS: [LayoutDecoyBucket; 4] = [
        LayoutDecoyBucket::Wide,
        LayoutDecoyBucket::Landscape,
        LayoutDecoyBucket::Square,
        LayoutDecoyBucket::Portrait,
    ];

    #[test]
    fn every_real_aspect_lands_in_exactly_one_of_four_buckets() {
        // Ordinary shapes.
        assert_eq!(
            LayoutDecoyBucket::for_dimensions(4000, 1000),
            Some(LayoutDecoyBucket::Wide)
        );
        assert_eq!(
            LayoutDecoyBucket::for_dimensions(4032, 3024),
            Some(LayoutDecoyBucket::Landscape)
        );
        assert_eq!(
            LayoutDecoyBucket::for_dimensions(1000, 1000),
            Some(LayoutDecoyBucket::Square)
        );
        assert_eq!(
            LayoutDecoyBucket::for_dimensions(1080, 1920),
            Some(LayoutDecoyBucket::Portrait)
        );
        // Extremes clamp into the same four, so an unusual picture never
        // discloses more than an ordinary one.
        assert_eq!(
            LayoutDecoyBucket::for_dimensions(60_000, 3),
            Some(LayoutDecoyBucket::Wide)
        );
        assert_eq!(
            LayoutDecoyBucket::for_dimensions(3, 60_000),
            Some(LayoutDecoyBucket::Portrait)
        );
        // Degenerate input gets no bucket, so the caller must fail closed
        // rather than place a row it could not size.
        assert_eq!(LayoutDecoyBucket::for_dimensions(0, 10), None);
        assert_eq!(LayoutDecoyBucket::for_dimensions(10, 0), None);
        assert_eq!(
            LayoutDecoyBucket::for_dimensions(MAX_SOURCE_EDGE + 1, 10),
            None
        );
    }

    #[test]
    fn bucket_boundaries_are_exact_and_monotonic() {
        // Walking one aspect step across each boundary changes the bucket once
        // and never skips one. 16:9 and 6:5 are the two boundary ratios.
        assert_eq!(
            LayoutDecoyBucket::for_dimensions(1600, 900),
            Some(LayoutDecoyBucket::Landscape),
            "exactly 16:9 is the top of Landscape, not the bottom of Wide"
        );
        assert_eq!(
            LayoutDecoyBucket::for_dimensions(1601, 900),
            Some(LayoutDecoyBucket::Wide)
        );
        assert_eq!(
            LayoutDecoyBucket::for_dimensions(1200, 1000),
            Some(LayoutDecoyBucket::Square)
        );
        assert_eq!(
            LayoutDecoyBucket::for_dimensions(1201, 1000),
            Some(LayoutDecoyBucket::Landscape)
        );
        assert_eq!(
            LayoutDecoyBucket::for_dimensions(1000, 1200),
            Some(LayoutDecoyBucket::Square),
            "the reciprocal boundary is inclusive on the Square side"
        );
        assert_eq!(
            LayoutDecoyBucket::for_dimensions(1000, 1201),
            Some(LayoutDecoyBucket::Portrait)
        );
    }

    /// The anti-leak property, stated as a test even though the real guarantee
    /// is the signature: `layout_decoy_png` has no parameter that could carry
    /// the protected image, so two completely different images of the same
    /// bucket produce decoys that are indistinguishable in every way except
    /// their random pixels.
    #[test]
    fn the_decoy_is_payload_free_and_never_repeats_its_bytes() {
        for bucket in ALL_BUCKETS {
            let first = layout_decoy_png(bucket);
            let second = layout_decoy_png(bucket);
            assert_eq!(
                first.len(),
                second.len(),
                "decoy length must depend on the bucket alone"
            );
            assert_ne!(
                first, second,
                "a fixed-byte decoy would give every send the same CDN hash"
            );
        }
    }

    #[test]
    fn decoy_declares_its_bucket_dimensions_and_stays_small() {
        for bucket in ALL_BUCKETS {
            let png = layout_decoy_png(bucket);
            let (width, height) = bucket.dimensions();
            assert_eq!(&png[..8], &PNG_SIGNATURE);
            // IHDR body starts at 8 (signature) + 4 (length) + 4 (type).
            assert_eq!(&png[12..16], b"IHDR");
            assert_eq!(
                u32::from_be_bytes([png[16], png[17], png[18], png[19]]),
                width
            );
            assert_eq!(
                u32::from_be_bytes([png[20], png[21], png[22], png[23]]),
                height
            );
            assert_eq!(png[24], 1, "bit depth");
            assert_eq!(png[25], 3, "colour type: palette");
            assert!(
                png.len() < 64 * 1024,
                "{} decoy is {} bytes; a layout decoy must stay a plausible small upload",
                bucket.label(),
                png.len()
            );
        }
    }

    /// Walk the file the way a decoder does: every chunk length and CRC, the
    /// chunk order, and the zlib stream inside IDAT. A decoy Discord refuses is
    /// worse than no decoy at all, because the operator would be left with a
    /// message whose row never gets its media rect.
    #[test]
    fn decoy_is_a_structurally_valid_png_a_decoder_would_accept() {
        for bucket in ALL_BUCKETS {
            let png = layout_decoy_png(bucket);
            let (width, height) = bucket.dimensions();
            let mut p = 8usize;
            let mut order = Vec::new();
            let mut idat = Vec::new();
            while p + 8 <= png.len() {
                let len = u32::from_be_bytes([png[p], png[p + 1], png[p + 2], png[p + 3]]) as usize;
                let chunk_type = &png[p + 4..p + 8];
                let body = &png[p + 8..p + 8 + len];
                let stored = u32::from_be_bytes([
                    png[p + 8 + len],
                    png[p + 9 + len],
                    png[p + 10 + len],
                    png[p + 11 + len],
                ]);
                assert_eq!(
                    stored,
                    crc32(&png[p + 4..p + 8 + len]),
                    "chunk CRC must verify"
                );
                order.push(std::str::from_utf8(chunk_type).unwrap().to_owned());
                if chunk_type == b"IDAT" {
                    idat.extend_from_slice(body);
                }
                p += 12 + len;
            }
            assert_eq!(p, png.len(), "no trailing bytes after IEND");
            assert_eq!(order, vec!["IHDR", "PLTE", "IDAT", "IEND"]);

            // zlib header, then stored deflate blocks, then adler32.
            assert_eq!(idat[0], 0x78);
            assert_eq!(idat[1], 0x01);
            assert_eq!(
                (u16::from(idat[0]) << 8 | u16::from(idat[1])) % 31,
                0,
                "zlib header check value"
            );
            let mut q = 2usize;
            let mut raw = Vec::new();
            loop {
                let header = idat[q];
                assert_eq!(header & 0b110, 0, "BTYPE must be 00 (stored)");
                let len = u16::from_le_bytes([idat[q + 1], idat[q + 2]]);
                let nlen = u16::from_le_bytes([idat[q + 3], idat[q + 4]]);
                assert_eq!(len, !nlen, "stored block LEN/NLEN must be complements");
                raw.extend_from_slice(&idat[q + 5..q + 5 + len as usize]);
                q += 5 + len as usize;
                if header & 1 == 1 {
                    break;
                }
            }
            assert_eq!(
                u32::from_be_bytes([idat[q], idat[q + 1], idat[q + 2], idat[q + 3]]),
                adler32(&raw),
                "adler32 trailer must match the inflated data"
            );
            assert_eq!(q + 4, idat.len());

            let row_bytes = width.div_ceil(8) as usize;
            assert_eq!(raw.len(), (row_bytes + 1) * height as usize);
            for row in 0..height as usize {
                assert_eq!(raw[row * (row_bytes + 1)], 0, "filter byte must be None");
            }
        }
    }

    #[test]
    fn crc32_and_adler32_match_their_published_vectors() {
        // Guards the hand-rolled checksums against a silent transcription slip,
        // which would produce a PNG every decoder rejects.
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(adler32(b"abc"), 0x024D_0127);
        assert_eq!(adler32(b""), 1);
    }
}
