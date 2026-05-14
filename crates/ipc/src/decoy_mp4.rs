//! Phase 8e: minimal decoy MP4 container.
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
//! them without complaining and Discord's CDN preserves the trailing
//! bytes verbatim (octet-stream-style; no transcoding).

use std::sync::OnceLock;

static DECOY_MP4: OnceLock<Vec<u8>> = OnceLock::new();

/// Public accessor — cached after first call.
pub fn decoy_mp4() -> &'static [u8] {
    DECOY_MP4.get_or_init(build_decoy_mp4)
}

/// Minimal SPS NAL unit for a 16×16 baseline level-1.0 H.264 stream.
///
/// Byte layout:
/// - `0x67`: NAL header (forbidden_zero_bit=0, nal_ref_idc=3,
///           nal_unit_type=7 = SPS).
/// - `0x42`: profile_idc = 66 (baseline).
/// - `0xC0`: constraint_set0_flag + constraint_set1_flag, reserved 0.
/// - `0x0A`: level_idc = 10 (level 1.0).
/// - `0xF4`: bit-packed `seq_parameter_set_id=ue(0)`,
///           `log2_max_frame_num_minus4=ue(0)`,
///           `pic_order_cnt_type=ue(0)`,
///           `log2_max_pic_order_cnt_lsb_minus4=ue(0)`,
///           `num_ref_frames=ue(1)`,
///           `gaps_in_frame_num_value_allowed_flag=0`.
/// - `0xE2`: bit-packed `pic_width_in_mbs_minus1=ue(0)` (16-px wide),
///           `pic_height_in_map_units_minus1=ue(0)` (16-px tall),
///           `frame_mbs_only_flag=1`,
///           `direct_8x8_inference_flag=0`,
///           `frame_cropping_flag=0`,
///           `vui_parameters_present_flag=0`,
///           `rbsp_trailing_bits=10000000` aligning to byte.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoy_starts_with_ftyp() {
        let d = decoy_mp4();
        assert!(d.len() >= 28);
        // size BE u32 at offset 0
        let size = u32::from_be_bytes([d[0], d[1], d[2], d[3]]) as usize;
        assert!(size >= 24 && size <= 64);
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
}
