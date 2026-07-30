//! The carrier row must render with the same number of lines as the plaintext
//! it hides, so the eye's painted plaintext lands inside the Discord row rather
//! than overflowing into its neighbours.
//!
//! These tests assert on counts and structural facts only. No plaintext or
//! cover content is ever printed, logged, or hashed — a failure message that
//! quoted the cover would leak the carrier for that message.

use stego::{
    decode_mode1, decode_token, encode_mode1_shaped, encode_token_shaped, is_mode1,
    rendered_rows, rows_for_hard_lines, shape_cover, ConversationCipher, RowBudget, RowMatch,
    CHUNK_PAYLOAD_BYTES, MAX_SHAPED_ROWS, MODE1_MAX_RAW_LEN, TOKEN_ID_BYTES,
};

/// Discord's message column at 100% zoom on a default window is roughly 85
/// grapheme-ish units wide. 40 stands in for a zoomed or narrow column.
const WIDE_COLUMN: usize = 85;
const NARROW_COLUMN: usize = 40;

/// The hub's carrier planner refuses a cover taller than this, so a shaped
/// cover must never exceed it. Mirrors `MAX_TARGET_LINES` in
/// `apps/osl-hub/src/discord_carrier_geometry.rs`.
const PLANNER_MAX_TARGET_LINES: usize = 96;

/// `MAX_HARD_LINES` in the same module: the most hard lines a plaintext may
/// have before the planner falls back on structure grounds.
const PLANNER_MAX_HARD_LINES: usize = 64;

fn cipher() -> ConversationCipher {
    ConversationCipher::from_salt(b"dm:1234567890123456789")
}

fn pointer(seed: u8) -> [u8; TOKEN_ID_BYTES] {
    let mut id = [0u8; TOKEN_ID_BYTES];
    for (index, byte) in id.iter_mut().enumerate() {
        *byte = seed
            .wrapping_mul(37)
            .wrapping_add((index as u8).wrapping_mul(11));
    }
    id
}

/// Structural hygiene every shaped carrier must satisfy: Chromium rewrites
/// collapsing spaces as `\u{a0}` and Slate pads leaves with `\u{feff}`, so a
/// cover that contains a whitespace run, a blank line, or an edge separator can
/// mutate in transit and fail the readback comparator.
fn assert_survives_the_round_trip(text: &str) {
    assert!(!text.is_empty());
    assert!(!text.contains("  "));
    assert!(!text.contains(" \n"));
    assert!(!text.contains("\n "));
    assert!(!text.contains("\n\n"));
    assert!(!text.contains('\r'));
    assert!(!text.contains('\t'));
    assert!(!text.contains('\u{a0}'));
    assert!(!text.contains('\u{feff}'));
    assert!(!text.starts_with(char::is_whitespace));
    assert!(!text.ends_with(char::is_whitespace));
    assert!(!text.chars().any(|c| c.is_control() && c != '\n'));
}

// ==========================================================================
// The property the feature exists for
// ==========================================================================

#[test]
fn a_shaped_token_carrier_matches_the_plaintext_row_count_and_still_decodes() {
    let cipher = cipher();
    let mac_key = b"per-scope-mac-key";
    let mut exact = 0usize;
    let mut taller = 0usize;
    let mut shorter = 0usize;

    // Plaintext shapes a person actually sends: one short line, a few short
    // lines, a wrapped paragraph, and a blank line between two paragraphs.
    let shapes: [Vec<u32>; 6] = [
        vec![9],
        vec![9, 14],
        vec![9, 14, 7],
        vec![40, 0, 40],
        vec![220],
        vec![12, 12, 12, 12, 12, 12],
    ];

    for column in [NARROW_COLUMN, WIDE_COLUMN] {
        for shape in &shapes {
            let target = rows_for_hard_lines(shape, column);
            for seed in 0u8..8 {
                let id = pointer(seed);
                let shaped =
                    encode_token_shaped(&cipher, mac_key, &id, RowBudget::new(target, column));
                assert_survives_the_round_trip(shaped.text());
                assert_eq!(
                    shaped.rows,
                    rendered_rows(shaped.text(), column),
                    "the reported row count must be the rendered row count"
                );
                assert!(shaped.rows <= PLANNER_MAX_TARGET_LINES);
                // The payload must survive separator rewriting untouched.
                assert_eq!(
                    decode_token(&cipher, mac_key, shaped.text()),
                    Some(id),
                    "a shaped carrier must still decode to its pointer"
                );
                match shaped.outcome {
                    RowMatch::Exact => exact += 1,
                    RowMatch::Taller { .. } => taller += 1,
                    RowMatch::Shorter { .. } => shorter += 1,
                }
            }
        }
    }

    println!("token carrier outcomes: exact={exact} taller={taller} shorter={shorter}");
    assert!(exact > 0, "exact matching must be reachable in token mode");
    assert_eq!(
        shorter, 0,
        "no ordinary chat shape may leave the carrier shorter than the plaintext"
    );
}

#[test]
fn shaping_is_transparent_to_the_wordbank_payload_decoder_too() {
    let cipher = cipher();
    for payload in [
        vec![0u8],
        vec![0xff; 7],
        (0..32u8).collect::<Vec<_>>(),
        (0..MODE1_MAX_RAW_LEN as u8).collect::<Vec<_>>(),
    ] {
        for target in [1usize, 4, 12, 40] {
            let shaped =
                encode_mode1_shaped(&cipher, &payload, RowBudget::new(target, WIDE_COLUMN))
                    .expect("payload is within the wordbank cap");
            assert_survives_the_round_trip(shaped.text());
            assert!(is_mode1(shaped.text()), "the prefix must stay on line one");
            assert_eq!(
                decode_mode1(&cipher, shaped.text()).expect("shaped wordbank cover decodes"),
                payload,
                "separator rewriting must not disturb the wordbank round trip"
            );
        }
    }
}

// ==========================================================================
// Boundaries
// ==========================================================================

#[test]
fn a_one_line_plaintext_is_the_floor_case_and_is_reported_not_faked() {
    let cipher = cipher();
    let mac_key = b"per-scope-mac-key";
    let target = rows_for_hard_lines(&[9], WIDE_COLUMN);
    assert_eq!(target, 1);

    let mut floors = Vec::new();
    for seed in 0u8..32 {
        let shaped = encode_token_shaped(
            &cipher,
            mac_key,
            &pointer(seed),
            RowBudget::new(1, WIDE_COLUMN),
        );
        assert_survives_the_round_trip(shaped.text());
        assert_eq!(shaped.rows, shaped.min_rows);
        assert!(
            shaped.outcome.is_exact() || matches!(shaped.outcome, RowMatch::Taller { .. }),
            "a one-line target must never come back short"
        );
        assert_eq!(
            decode_token(&cipher, mac_key, shaped.text()),
            Some(pointer(seed))
        );
        floors.push(shaped.min_rows);
    }
    let worst = *floors.iter().max().unwrap();
    let best = *floors.iter().min().unwrap();
    println!("token floor at column {WIDE_COLUMN}: min={best} max={worst} rows for a 1-row target");
    assert!(worst <= 4, "the token floor must stay within a couple of rows");
}

#[test]
fn a_plaintext_at_the_planner_hard_line_limit_is_reported_honestly() {
    let cipher = cipher();
    let mac_key = b"per-scope-mac-key";
    let shape = vec![12u32; PLANNER_MAX_HARD_LINES];
    let target = rows_for_hard_lines(&shape, WIDE_COLUMN);
    assert_eq!(target, PLANNER_MAX_HARD_LINES);

    let shaped = encode_token_shaped(
        &cipher,
        mac_key,
        &pointer(5),
        RowBudget::new(target, WIDE_COLUMN),
    );
    assert_survives_the_round_trip(shaped.text());
    assert_eq!(decode_token(&cipher, mac_key, shaped.text()), Some(pointer(5)));
    assert!(shaped.rows <= PLANNER_MAX_TARGET_LINES);
    // A 12-byte pointer does not have 64 words in it, so the carrier cannot be
    // stretched that tall. It must say so rather than silently misalign.
    match shaped.outcome {
        RowMatch::Exact => {}
        RowMatch::Shorter { missing_rows } => {
            println!(
                "64-hard-line target: carrier reached {} of {} rows (max {}), short by {}",
                shaped.rows, target, shaped.max_rows, missing_rows
            );
            assert_eq!(shaped.rows, shaped.max_rows);
        }
        RowMatch::Taller { .. } => panic!("a 64-row target cannot overshoot"),
    }
    // Never papered over with filler.
    assert!(!shaped.text().contains("\n\n"));
}

#[test]
fn a_target_below_the_payload_floor_stays_taller_and_never_drops_payload() {
    let cipher = cipher();
    let mac_key = b"per-scope-mac-key";
    // A very narrow column pushes the floor well above a one-row plaintext.
    let shaped = encode_token_shaped(&cipher, mac_key, &pointer(11), RowBudget::new(1, 14));
    assert!(shaped.min_rows > 1, "the narrow column must force a floor");
    assert_eq!(shaped.rows, shaped.min_rows);
    assert_eq!(
        shaped.outcome,
        RowMatch::Taller {
            extra_rows: shaped.min_rows - 1
        }
    );
    assert!(!shaped.outcome.overflows_row());
    assert_eq!(
        decode_token(&cipher, mac_key, shaped.text()),
        Some(pointer(11)),
        "shrinking below the floor must never be attempted by dropping payload"
    );
}

#[test]
fn a_very_long_single_line_that_soft_wraps_is_measured_by_rendered_rows() {
    let cipher = cipher();
    let mac_key = b"per-scope-mac-key";
    // One hard line of 900 graphemes: no newline in the plaintext at all, but
    // it renders over many rows and the carrier has to match those rows.
    let target = rows_for_hard_lines(&[900], WIDE_COLUMN);
    assert_eq!(target, 900usize.div_ceil(WIDE_COLUMN));
    assert!(target > 1, "the point of the case is soft wrapping");

    let shaped = encode_token_shaped(
        &cipher,
        mac_key,
        &pointer(7),
        RowBudget::new(target, WIDE_COLUMN),
    );
    assert_survives_the_round_trip(shaped.text());
    assert_eq!(shaped.rows, rendered_rows(shaped.text(), WIDE_COLUMN));
    println!(
        "soft-wrapped 900-grapheme line: target={target} carrier={} outcome_exact={}",
        shaped.rows,
        shaped.outcome.is_exact()
    );
    assert_eq!(
        decode_token(&cipher, mac_key, shaped.text()),
        Some(pointer(7))
    );
}

#[test]
fn empty_ish_input_fails_closed_instead_of_producing_a_placeable_row() {
    // No structure measured at all: nothing to aim for, nothing to place.
    assert_eq!(rows_for_hard_lines(&[], WIDE_COLUMN), 0);
    // A single empty hard line still occupies one row.
    assert_eq!(rows_for_hard_lines(&[0], WIDE_COLUMN), 1);

    let shaped = shape_cover("", RowBudget::new(1, WIDE_COLUMN));
    assert!(shaped.text().is_empty());
    assert_eq!(shaped.rows, 0);
    assert_eq!(shaped.outcome, RowMatch::Shorter { missing_rows: 1 });
    assert!(shaped.outcome.overflows_row());

    let nothing_wanted = shape_cover("", RowBudget::new(0, WIDE_COLUMN));
    assert_eq!(nothing_wanted.outcome, RowMatch::Exact);
    assert_eq!(nothing_wanted.rows, 0);
}

#[test]
fn a_degenerate_column_measurement_cannot_panic_or_explode_the_row_count() {
    let cipher = cipher();
    let mac_key = b"per-scope-mac-key";
    for column in [0usize, 1, 2, usize::MAX] {
        let shaped = encode_token_shaped(
            &cipher,
            mac_key,
            &pointer(3),
            RowBudget::new(usize::MAX, column),
        );
        assert!(shaped.rows <= MAX_SHAPED_ROWS.max(shaped.min_rows));
        assert_eq!(
            decode_token(&cipher, mac_key, shaped.text()),
            Some(pointer(3))
        );
    }
}

// ==========================================================================
// Measured floors, for the record
// ==========================================================================

#[test]
fn measured_carrier_floors_per_mode_and_column() {
    let cipher = cipher();
    let mac_key = b"per-scope-mac-key";

    for column in [NARROW_COLUMN, WIDE_COLUMN, 120] {
        let mut words = Vec::new();
        let mut floors = Vec::new();
        let mut ceilings = Vec::new();
        for seed in 0u8..64 {
            let shaped =
                encode_token_shaped(&cipher, mac_key, &pointer(seed), RowBudget::new(1, column));
            words.push(shaped.text().split_whitespace().count());
            floors.push(shaped.min_rows);
            ceilings.push(shaped.max_rows);
        }
        println!(
            "token mode, column {column}: words {}..{}, floor {}..{} rows, ceiling {}..{} rows",
            words.iter().min().unwrap(),
            words.iter().max().unwrap(),
            floors.iter().min().unwrap(),
            floors.iter().max().unwrap(),
            ceilings.iter().min().unwrap(),
            ceilings.iter().max().unwrap(),
        );
    }

    // Wordbank mode carries payload in the text, so its floor tracks the
    // payload size directly. These are the numbers that decide whether
    // compressing the plaintext could ever buy an exact match.
    for payload_len in [1usize, 8, 24, 60, CHUNK_PAYLOAD_BYTES, MODE1_MAX_RAW_LEN] {
        let payload = vec![0x5au8; payload_len];
        let shaped = encode_mode1_shaped(&cipher, &payload, RowBudget::new(1, WIDE_COLUMN))
            .expect("within cap");
        println!(
            "wordbank mode, column {WIDE_COLUMN}: {payload_len} payload bytes -> {} words, floor {} rows, ceiling {} rows",
            shaped.text().split_whitespace().count(),
            shaped.min_rows,
            shaped.max_rows,
        );
    }
}
