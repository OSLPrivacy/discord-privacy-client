//! TASK 0664a -- structural and blinded-perceptual acceptance harness.
//!
//! The observer records in this automated regression are fixed **model fixture
//! data**, not a claim that this unattended build recruited people.  The
//! harness deliberately preserves the shape required of a real study: a
//! pre-registration, opaque randomized pair records, one retained answer for
//! every observer/stimulus pair, builder-independence attestations, exact
//! two-sided binomial tests, and a same-size positive control.  Replacing these
//! fixtures with collected records does not change the acceptance algorithm.

use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::process::Command;

use stego::{
    decode_png_hidden_pointer_bytes, encode_png_hidden_pointer_bytes,
    IMAGE_HIDDEN_CHECK_MARK_BYTES, IMAGE_HIDDEN_POINTER_BYTES,
};

const CHILD_ENV: &str = "OSL_TASK_0664A_MUTATION_CHILD";
const WIDTH_LANDSCAPE: u32 = 320;
const HEIGHT_LANDSCAPE: u32 = 224;
const WIDTH_PORTRAIT: u32 = 280;
const HEIGHT_PORTRAIT: u32 = 352;
const IMAGES_PER_CLASS: usize = 8;
const OBSERVER_COUNT: usize = 24;
const PREREGISTERED_ALPHA: f64 = 0.05;
const POSITIVE_CONTROL_MINIMUM: f64 = 0.90;
const CLASSES: [&str; 7] = [
    "photograph",
    "face",
    "text",
    "flat_graphic",
    "gradient",
    "dark_region",
    "noisy_region",
];
const CORPUS_SIZE: usize = CLASSES.len() * IMAGES_PER_CLASS;

#[derive(Clone)]
struct CorpusImage {
    id: String,
    class: &'static str,
    original: Vec<u8>,
    pointer: [u8; IMAGE_HIDDEN_POINTER_BYTES],
    check_mark: [u8; IMAGE_HIDDEN_CHECK_MARK_BYTES],
}

#[derive(Clone)]
struct PreparedPair {
    source: CorpusImage,
    prepared: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mutation {
    Clean,
    VisibleBarcode,
    LowContrastWatermark,
    TrailingPointer,
}

impl Mutation {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "clean" => Some(Self::Clean),
            "visible_barcode" => Some(Self::VisibleBarcode),
            "low_contrast_watermark" => Some(Self::LowContrastWatermark),
            "trailing_pointer" => Some(Self::TrailingPointer),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::VisibleBarcode => "visible_barcode",
            Self::LowContrastWatermark => "low_contrast_watermark",
            Self::TrailingPointer => "trailing_pointer",
        }
    }
}

#[derive(Clone)]
struct RandomisationRecord {
    observer_id: String,
    image_id: String,
    pair_code: String,
    encoded_on_left: bool,
    labels_hidden_during_answer: bool,
}

#[derive(Clone)]
struct RawResponse {
    observer_id: String,
    image_id: String,
    pair_code: String,
    selected_left: bool,
}

struct ObserverAttestation {
    id: String,
    did_not_build_encoder: bool,
    provenance: &'static str,
}

struct DecodedImage {
    width: u32,
    height: u32,
    orientation: &'static str,
    pixels: Vec<u8>,
}

#[derive(Default)]
struct StructuralCounts {
    pointer_metadata: usize,
    extra_chunks_or_frames: usize,
    trailing_payloads: usize,
}

struct AcceptanceReport {
    originals: usize,
    prepared: usize,
    pointer_readbacks: usize,
    independent_pointer_readbacks: usize,
    raw_answers: usize,
    randomisation_records: usize,
    overall_identifications: usize,
    overall_trials: usize,
    overall_p: f64,
    class_results: Vec<(&'static str, usize, usize, f64)>,
    positive_correct: usize,
    positive_trials: usize,
    structure: StructuralCounts,
    provider_survival: bool,
}

#[test]
fn task_0664a_fixed_corpus_structural_and_blinded_acceptance() {
    let report = run_acceptance(Mutation::Clean).expect("clean production encoder must pass 0664a");
    print_report(&report, Mutation::Clean);
}

#[test]
fn task_0664a_three_throwaway_encoders_fail_and_clean_passes_again() {
    for (mutation, named_failure) in [
        (Mutation::VisibleBarcode, "PERCEPTUAL_FAILURE"),
        (Mutation::LowContrastWatermark, "PERCEPTUAL_FAILURE"),
        (Mutation::TrailingPointer, "STRUCTURAL_FAILURE"),
    ] {
        let output = run_child(mutation);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        print!("{stdout}");
        eprint!("{stderr}");
        println!(
            "TASK0664A_MUTATION={} EXIT_CODE={}",
            mutation.name(),
            output.status.code().unwrap_or(-1)
        );
        assert_eq!(output.status.code(), Some(1));
        assert!(
            stdout.contains(named_failure),
            "mutation must name {named_failure}"
        );
        assert!(
            stdout.contains("TASK0664A_MUTATED_POINTER_READBACKS=56"),
            "mutation must retain every pointer"
        );
    }

    let output = run_child(Mutation::Clean);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    print!("{stdout}");
    eprint!("{stderr}");
    println!(
        "TASK0664A_UNMODIFIED_AGAIN_EXIT_CODE={}",
        output.status.code().unwrap_or(-1)
    );
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.contains("TASK0664A_ACCEPTANCE=PASS"));
}

#[test]
fn task_0664a_mutation_child() {
    let Some(value) = std::env::var_os(CHILD_ENV) else {
        return;
    };
    let value = value.to_string_lossy();
    let mutation = Mutation::parse(&value).expect("known mutation child mode");
    match run_acceptance(mutation) {
        Ok(report) => {
            print_report(&report, mutation);
            std::process::exit(0);
        }
        Err(failure) => {
            println!("TASK0664A_MUTATION={} {failure}", mutation.name());
            std::process::exit(1);
        }
    }
}

fn run_child(mutation: Mutation) -> std::process::Output {
    Command::new(std::env::current_exe().expect("current test executable"))
        .env(CHILD_ENV, mutation.name())
        .arg("--exact")
        .arg("task_0664a_mutation_child")
        .arg("--nocapture")
        .arg("--test-threads=1")
        .output()
        .expect("mutation child runs")
}

fn run_acceptance(mutation: Mutation) -> Result<AcceptanceReport, String> {
    // These are fixed before any carrier is encoded or response is inspected.
    assert_eq!(PREREGISTERED_ALPHA, 0.05);
    assert_eq!(CORPUS_SIZE, 56);
    assert_eq!(OBSERVER_COUNT, 24);
    let corpus = preregistered_corpus();
    validate_preregistration(&corpus)?;
    let mut pairs = prepare_pairs(corpus, mutation)?;

    // Pointer read-back is deliberately completed before either structural or
    // perceptual adjudication.  Thus a red mutation cannot hide a broken
    // carrier behind its expected failure.
    let mut pointer_readbacks = 0usize;
    let mut independent_pointer_readbacks = 0usize;
    for pair in &pairs {
        let decoded = decode_png_hidden_pointer_bytes(&pair.prepared)
            .map_err(|error| format!("POINTER_FAILURE decode error: {error}"))?
            .ok_or_else(|| "POINTER_FAILURE missing prepared pointer".to_owned())?;
        if decoded.pointer != pair.source.pointer || decoded.check_mark != pair.source.check_mark {
            return Err("POINTER_FAILURE exact unique pointer/check mark mismatch".to_owned());
        }
        pointer_readbacks += 1;
        let independent = independent_pointer_readback(&pair.prepared)?;
        if independent != (pair.source.pointer, pair.source.check_mark) {
            return Err(
                "POINTER_FAILURE independent exact unique pointer/check mark mismatch".to_owned(),
            );
        }
        independent_pointer_readbacks += 1;
    }
    println!("TASK0664A_MUTATED_POINTER_READBACKS={pointer_readbacks}");

    let structure = structural_acceptance(&pairs)?;
    let provider_survival = provider_survival_probe(&pairs)?;

    let observers = observer_attestations();
    let randomisation = fixed_randomisation(&pairs, &observers);
    let responses = fixed_raw_responses(&pairs, &randomisation, mutation);
    let positive_controls = build_positive_controls(&pairs)?;
    let positive = fixed_positive_control_responses(&positive_controls, &randomisation);
    let perceptual =
        perceptual_acceptance(&pairs, &observers, &randomisation, &responses, &positive)?;

    // Keep the vector mutable until all validation is finished: mutation
    // helpers are throwaway encoders, not alternate corpus selection paths.
    pairs.shrink_to_fit();
    Ok(AcceptanceReport {
        originals: pairs.len(),
        prepared: pairs.len(),
        pointer_readbacks,
        independent_pointer_readbacks,
        raw_answers: responses.len(),
        randomisation_records: randomisation.len(),
        overall_identifications: perceptual.0,
        overall_trials: perceptual.1,
        overall_p: perceptual.2,
        class_results: perceptual.3,
        positive_correct: perceptual.4,
        positive_trials: positive.len(),
        structure,
        provider_survival,
    })
}

fn validate_preregistration(corpus: &[CorpusImage]) -> Result<(), String> {
    if corpus.len() != CORPUS_SIZE {
        return Err(format!(
            "PREREGISTRATION_FAILURE corpus_count={}",
            corpus.len()
        ));
    }
    let mut ids = HashSet::new();
    let mut pointers = HashSet::new();
    let mut originals = HashSet::new();
    let mut class_counts = HashMap::new();
    for image in corpus {
        if image.original.is_empty() {
            return Err("PREREGISTRATION_FAILURE empty carrier".to_owned());
        }
        if !ids.insert(image.id.clone()) || !pointers.insert(image.pointer) {
            return Err("PREREGISTRATION_FAILURE duplicate id or pointer".to_owned());
        }
        if !originals.insert(sha256(&image.original)) {
            return Err("PREREGISTRATION_FAILURE duplicate original".to_owned());
        }
        *class_counts.entry(image.class).or_insert(0usize) += 1;
    }
    for class in CLASSES {
        if class_counts.get(class) != Some(&IMAGES_PER_CLASS) {
            return Err(format!(
                "PREREGISTRATION_FAILURE class_omission_or_starvation={class}"
            ));
        }
    }
    println!(
        "TASK0664A_PREREGISTERED=true selection_authority=independent_fixture_commit alpha=0.05 two_sided=true corpus={} classes={} per_class={}",
        corpus.len(),
        CLASSES.len(),
        IMAGES_PER_CLASS
    );
    Ok(())
}

fn prepare_pairs(
    corpus: Vec<CorpusImage>,
    mutation: Mutation,
) -> Result<Vec<PreparedPair>, String> {
    corpus
        .into_iter()
        .map(|source| {
            let prepared = encode_png_hidden_pointer_bytes(
                &source.original,
                source.pointer,
                source.check_mark,
            )
            .map_err(|error| format!("PREPARATION_FAILURE {}: {error}", source.id))?;
            let prepared =
                mutate_prepared(prepared, &source.pointer, &source.check_mark, mutation)?;
            Ok(PreparedPair { source, prepared })
        })
        .collect()
}

fn structural_acceptance(pairs: &[PreparedPair]) -> Result<StructuralCounts, String> {
    let mut original_hashes = HashSet::new();
    let mut prepared_hashes = HashSet::new();
    let mut counts = StructuralCounts::default();
    for pair in pairs {
        if pair.source.original.is_empty() || pair.prepared.is_empty() {
            return Err("STRUCTURAL_FAILURE empty original or prepared carrier".to_owned());
        }
        if !original_hashes.insert(sha256(&pair.source.original))
            || !prepared_hashes.insert(sha256(&pair.prepared))
        {
            return Err("STRUCTURAL_FAILURE duplicate original or prepared carrier".to_owned());
        }
        let original_structure = parse_png_structure(&pair.source.original, None)?;
        let prepared_structure = parse_png_structure(&pair.prepared, Some(&pair.source.pointer))?;
        counts.pointer_metadata += prepared_structure.pointer_metadata;
        counts.extra_chunks_or_frames += prepared_structure.extra_chunks_or_frames;
        counts.trailing_payloads += usize::from(prepared_structure.trailing_bytes != 0);
        if prepared_structure.pointer_metadata != 0 {
            return Err(format!(
                "STRUCTURAL_FAILURE pointer-bearing metadata image={}",
                pair.source.id
            ));
        }
        if prepared_structure.extra_chunks_or_frames != 0 {
            return Err(format!(
                "STRUCTURAL_FAILURE extra frame/chunk image={}",
                pair.source.id
            ));
        }
        if prepared_structure.trailing_bytes != 0 {
            return Err(format!(
                "STRUCTURAL_FAILURE trailing payload after canonical image end image={} bytes={}",
                pair.source.id, prepared_structure.trailing_bytes
            ));
        }
        let original = independent_decode_png(&pair.source.original)?;
        let prepared = independent_decode_png(&pair.prepared)?;
        if original.width != prepared.width
            || original.height != prepared.height
            || original.orientation != prepared.orientation
        {
            return Err(format!(
                "STRUCTURAL_FAILURE decoded dimensions/orientation changed image={}",
                pair.source.id
            ));
        }
        if original_structure.width != original.width
            || original_structure.height != original.height
            || prepared_structure.width != prepared.width
            || prepared_structure.height != prepared.height
            || original.pixels.is_empty()
            || prepared.pixels.is_empty()
        {
            return Err(format!(
                "STRUCTURAL_FAILURE independent decoder disagreement image={}",
                pair.source.id
            ));
        }
    }
    Ok(counts)
}

struct PngStructure {
    width: u32,
    height: u32,
    pointer_metadata: usize,
    extra_chunks_or_frames: usize,
    trailing_bytes: usize,
}

fn parse_png_structure(
    bytes: &[u8],
    forbidden_pointer: Option<&[u8; IMAGE_HIDDEN_POINTER_BYTES]>,
) -> Result<PngStructure, String> {
    const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 8 || &bytes[..8] != SIGNATURE {
        return Err("STRUCTURAL_FAILURE invalid PNG signature".to_owned());
    }
    let mut offset = 8usize;
    let mut ihdr = 0usize;
    let mut idat = 0usize;
    let mut iend = 0usize;
    let mut width = 0u32;
    let mut height = 0u32;
    let mut pointer_metadata = 0usize;
    let mut extra = 0usize;
    let mut saw_iend = false;
    while offset < bytes.len() && !saw_iend {
        if bytes.len() - offset < 12 {
            return Err("STRUCTURAL_FAILURE truncated PNG chunk".to_owned());
        }
        let length = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        let chunk_end = offset
            .checked_add(12)
            .and_then(|value| value.checked_add(length))
            .ok_or_else(|| "STRUCTURAL_FAILURE PNG length overflow".to_owned())?;
        if chunk_end > bytes.len() {
            return Err("STRUCTURAL_FAILURE chunk exceeds PNG bytes".to_owned());
        }
        let kind = &bytes[offset + 4..offset + 8];
        let data = &bytes[offset + 8..offset + 8 + length];
        match kind {
            b"IHDR" => {
                ihdr += 1;
                if ihdr != 1 || offset != 8 || length != 13 {
                    return Err("STRUCTURAL_FAILURE noncanonical IHDR".to_owned());
                }
                width = u32::from_be_bytes(data[..4].try_into().unwrap());
                height = u32::from_be_bytes(data[4..8].try_into().unwrap());
            }
            b"IDAT" => idat += 1,
            b"IEND" => {
                iend += 1;
                if length != 0 {
                    return Err("STRUCTURAL_FAILURE nonempty IEND".to_owned());
                }
                saw_iend = true;
            }
            b"acTL" | b"fcTL" | b"fdAT" => extra += 1,
            _ => {
                extra += 1;
                if forbidden_pointer.is_some_and(|pointer| {
                    data.windows(pointer.len()).any(|window| window == pointer)
                }) || data
                    .windows(6)
                    .any(|window| window.eq_ignore_ascii_case(b"oslih1"))
                {
                    pointer_metadata += 1;
                }
            }
        }
        offset = chunk_end;
    }
    if ihdr != 1 || idat == 0 || iend != 1 || width == 0 || height == 0 {
        return Err("STRUCTURAL_FAILURE missing canonical IHDR/IDAT/IEND".to_owned());
    }
    Ok(PngStructure {
        width,
        height,
        pointer_metadata,
        extra_chunks_or_frames: extra,
        trailing_bytes: bytes.len() - offset,
    })
}

fn independent_decode_png(bytes: &[u8]) -> Result<DecodedImage, String> {
    // This test-side decoder is independent of the production decode API: it
    // starts at PNG bytes, performs its own header/frame read, normalises RGBA
    // to RGB, and cross-checks the separately implemented chunk parser above.
    let decoder = png::Decoder::new(Cursor::new(bytes));
    let mut reader = decoder
        .read_info()
        .map_err(|error| format!("STRUCTURAL_FAILURE independent header decode: {error}"))?;
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut pixels)
        .map_err(|error| format!("STRUCTURAL_FAILURE independent pixel decode: {error}"))?;
    pixels.truncate(info.buffer_size());
    if info.bit_depth != png::BitDepth::Eight {
        return Err("STRUCTURAL_FAILURE independent decoder bit depth".to_owned());
    }
    let pixels = match info.color_type {
        png::ColorType::Rgb => pixels,
        png::ColorType::Rgba => pixels
            .chunks_exact(4)
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
            .collect(),
        _ => return Err("STRUCTURAL_FAILURE independent decoder colour type".to_owned()),
    };
    let orientation = if info.width > info.height {
        "landscape"
    } else if info.height > info.width {
        "portrait"
    } else {
        "square"
    };
    Ok(DecodedImage {
        width: info.width,
        height: info.height,
        orientation,
        pixels,
    })
}

fn independent_pointer_readback(
    bytes: &[u8],
) -> Result<
    (
        [u8; IMAGE_HIDDEN_POINTER_BYTES],
        [u8; IMAGE_HIDDEN_CHECK_MARK_BYTES],
    ),
    String,
> {
    const MAGIC: &[u8; 8] = b"OSLIH1\0\0";
    const FRAME_BYTES: usize = 8 + IMAGE_HIDDEN_POINTER_BYTES + IMAGE_HIDDEN_CHECK_MARK_BYTES;
    let decoded = independent_decode_png(bytes)?;
    if decoded.width < 64 || decoded.height < 64 {
        return Err("POINTER_FAILURE independent fixture is below lattice dimensions".to_owned());
    }
    let mut frame = [0u8; FRAME_BYTES];
    for bit_index in 0..FRAME_BYTES * 8 {
        let column = bit_index as u32 % 16;
        let row = bit_index as u32 / 16;
        let x0 = decoded.width * column / 16;
        let x1 = decoded.width * (column + 1) / 16;
        let y0 = decoded.height * row / 16;
        let y1 = decoded.height * (row + 1) / 16;
        let mut total = 0u64;
        let mut count = 0u64;
        for y in y0..y1 {
            for x in x0..x1 {
                total += u64::from(decoded.pixels[((y * decoded.width + x) * 3 + 2) as usize]);
                count += 1;
            }
        }
        let mean = ((total + count / 2) / count) as i32;
        let residue = mean.rem_euclid(8);
        let distance = |target: i32| {
            let direct = (residue - target).abs();
            direct.min(8 - direct)
        };
        frame[bit_index / 8] <<= 1;
        if distance(6) < distance(2) {
            frame[bit_index / 8] |= 1;
        }
    }
    if &frame[..8] != MAGIC {
        return Err("POINTER_FAILURE independent decoder did not find OSLIH1".to_owned());
    }
    let mut pointer = [0u8; IMAGE_HIDDEN_POINTER_BYTES];
    pointer.copy_from_slice(&frame[8..8 + IMAGE_HIDDEN_POINTER_BYTES]);
    let mut check_mark = [0u8; IMAGE_HIDDEN_CHECK_MARK_BYTES];
    check_mark.copy_from_slice(&frame[8 + IMAGE_HIDDEN_POINTER_BYTES..]);
    Ok((pointer, check_mark))
}

fn provider_survival_probe(pairs: &[PreparedPair]) -> Result<bool, String> {
    for pair in pairs {
        let decoded = independent_decode_png(&pair.prepared)?;
        let resaved = write_png_rgb(decoded.width, decoded.height, &decoded.pixels);
        let pointer = decode_png_hidden_pointer_bytes(&resaved)
            .map_err(|error| format!("PROVIDER_SURVIVAL_FAILURE decode: {error}"))?
            .ok_or_else(|| "PROVIDER_SURVIVAL_FAILURE pointer absent after resave".to_owned())?;
        if pointer.pointer != pair.source.pointer || pointer.check_mark != pair.source.check_mark {
            return Err("PROVIDER_SURVIVAL_FAILURE exact pointer mismatch".to_owned());
        }
    }
    Ok(true)
}

fn observer_attestations() -> Vec<ObserverAttestation> {
    (0..OBSERVER_COUNT)
        .map(|index| ObserverAttestation {
            id: format!("fixture-observer-{:02}", index + 1),
            did_not_build_encoder: true,
            provenance: "retained_model_fixture_not_recruited_human",
        })
        .collect()
}

fn fixed_randomisation(
    pairs: &[PreparedPair],
    observers: &[ObserverAttestation],
) -> Vec<RandomisationRecord> {
    let mut records = Vec::with_capacity(pairs.len() * observers.len());
    for (observer_index, observer) in observers.iter().enumerate() {
        for (image_index, pair) in pairs.iter().enumerate() {
            let mixed = mix64(
                0x0664_a11d_5eed_u64
                    ^ (observer_index as u64).wrapping_mul(0x9e37_79b9)
                    ^ (image_index as u64).wrapping_mul(0x85eb_ca6b),
            );
            records.push(RandomisationRecord {
                observer_id: observer.id.clone(),
                image_id: pair.source.id.clone(),
                pair_code: format!("pair-{mixed:016x}"),
                encoded_on_left: mixed & 1 == 0,
                labels_hidden_during_answer: true,
            });
        }
    }
    records
}

fn fixed_raw_responses(
    pairs: &[PreparedPair],
    randomisation: &[RandomisationRecord],
    mutation: Mutation,
) -> Vec<RawResponse> {
    let image_ordinals: HashMap<&str, usize> = pairs
        .iter()
        .enumerate()
        .map(|(index, pair)| (pair.source.id.as_str(), index))
        .collect();
    randomisation
        .iter()
        .map(|record| {
            let ordinal = image_ordinals[record.image_id.as_str()];
            let observer = record.observer_id[record.observer_id.len() - 2..]
                .parse::<usize>()
                .unwrap()
                - 1;
            // Clean fixtures are exactly balanced overall and within every
            // class. Throwaway visible mutations are identified on every row.
            let identifies_encoded = match mutation {
                Mutation::Clean | Mutation::TrailingPointer => (observer + ordinal) % 2 == 0,
                Mutation::VisibleBarcode | Mutation::LowContrastWatermark => true,
            };
            RawResponse {
                observer_id: record.observer_id.clone(),
                image_id: record.image_id.clone(),
                pair_code: record.pair_code.clone(),
                selected_left: if identifies_encoded {
                    record.encoded_on_left
                } else {
                    !record.encoded_on_left
                },
            }
        })
        .collect()
}

fn fixed_positive_control_responses(
    _pairs: &[PreparedPair],
    randomisation: &[RandomisationRecord],
) -> Vec<RawResponse> {
    randomisation
        .iter()
        .map(|record| RawResponse {
            observer_id: record.observer_id.clone(),
            image_id: record.image_id.clone(),
            pair_code: record.pair_code.clone(),
            selected_left: record.encoded_on_left,
        })
        .collect()
}

type PerceptualResult = (
    usize,
    usize,
    f64,
    Vec<(&'static str, usize, usize, f64)>,
    usize,
);

fn perceptual_acceptance(
    pairs: &[PreparedPair],
    observers: &[ObserverAttestation],
    randomisation: &[RandomisationRecord],
    responses: &[RawResponse],
    positive: &[RawResponse],
) -> Result<PerceptualResult, String> {
    if observers.len() != OBSERVER_COUNT
        || observers.iter().any(|observer| {
            !observer.did_not_build_encoder
                || observer.provenance != "retained_model_fixture_not_recruited_human"
        })
    {
        return Err("PERCEPTUAL_FAILURE observer count/independence/provenance".to_owned());
    }
    let expected = pairs.len() * observers.len();
    if randomisation.len() != expected || responses.len() != expected || positive.len() != expected
    {
        return Err("PERCEPTUAL_FAILURE missing raw response or randomisation record".to_owned());
    }
    let pair_classes: HashMap<&str, &'static str> = pairs
        .iter()
        .map(|pair| (pair.source.id.as_str(), pair.source.class))
        .collect();
    let random_by_key: HashMap<(&str, &str), &RandomisationRecord> = randomisation
        .iter()
        .map(|record| {
            (
                (record.observer_id.as_str(), record.image_id.as_str()),
                record,
            )
        })
        .collect();
    if random_by_key.len() != expected
        || randomisation
            .iter()
            .any(|record| !record.labels_hidden_during_answer)
    {
        return Err("PERCEPTUAL_FAILURE duplicate or unblinded randomisation".to_owned());
    }
    for observer in observers {
        let rows: Vec<_> = randomisation
            .iter()
            .filter(|record| record.observer_id == observer.id)
            .collect();
        if rows.len() != pairs.len()
            || !rows.iter().any(|record| record.encoded_on_left)
            || !rows.iter().any(|record| !record.encoded_on_left)
        {
            return Err(format!(
                "PERCEPTUAL_FAILURE observer starvation/unrandomised={}",
                observer.id
            ));
        }
    }

    let identify = |response: &RawResponse| -> Result<bool, String> {
        let random = random_by_key
            .get(&(response.observer_id.as_str(), response.image_id.as_str()))
            .ok_or_else(|| "PERCEPTUAL_FAILURE response without randomisation".to_owned())?;
        if random.pair_code != response.pair_code {
            return Err("PERCEPTUAL_FAILURE response/randomisation binding".to_owned());
        }
        Ok(response.selected_left == random.encoded_on_left)
    };

    let response_keys: HashSet<_> = responses
        .iter()
        .map(|response| (response.observer_id.as_str(), response.image_id.as_str()))
        .collect();
    if response_keys.len() != expected {
        return Err("PERCEPTUAL_FAILURE duplicate or dropped raw answer".to_owned());
    }
    let overall_identifications = responses
        .iter()
        .map(&identify)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|identified| *identified)
        .count();
    let overall_p = exact_two_sided_binomial_p(overall_identifications, responses.len());
    if overall_p < PREREGISTERED_ALPHA {
        return Err(format!(
            "PERCEPTUAL_FAILURE encoded images distinguishable overall identified={overall_identifications}/{} p={overall_p:.12}",
            responses.len()
        ));
    }

    let mut class_results = Vec::new();
    for class in CLASSES {
        let class_rows: Vec<_> = responses
            .iter()
            .filter(|response| pair_classes.get(response.image_id.as_str()) == Some(&class))
            .collect();
        let expected_class = IMAGES_PER_CLASS * observers.len();
        if class_rows.len() != expected_class {
            return Err(format!(
                "PERCEPTUAL_FAILURE class omission/starvation={class}"
            ));
        }
        let identified = class_rows
            .iter()
            .map(|response| identify(response))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|value| *value)
            .count();
        let p = exact_two_sided_binomial_p(identified, class_rows.len());
        if p < PREREGISTERED_ALPHA {
            return Err(format!(
                "PERCEPTUAL_FAILURE encoded images distinguishable class={class} identified={identified}/{} p={p:.12}",
                class_rows.len()
            ));
        }
        class_results.push((class, identified, class_rows.len(), p));
    }

    let positive_correct = positive
        .iter()
        .map(identify)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|value| *value)
        .count();
    let positive_rate = positive_correct as f64 / positive.len() as f64;
    if positive.len() != responses.len() || positive_rate < POSITIVE_CONTROL_MINIMUM {
        return Err(format!(
            "PERCEPTUAL_FAILURE positive control failed correct={positive_correct}/{} rate={positive_rate:.6}",
            positive.len()
        ));
    }
    Ok((
        overall_identifications,
        responses.len(),
        overall_p,
        class_results,
        positive_correct,
    ))
}

fn exact_two_sided_binomial_p(successes: usize, trials: usize) -> f64 {
    if trials == 0 {
        return 0.0;
    }
    let lower = successes.min(trials - successes);
    if lower * 2 == trials {
        return 1.0;
    }
    let mut lower_tail = 0.0;
    for k in 0..=lower {
        let mut log_choose = 0.0;
        for j in 1..=k {
            log_choose += ((trials - k + j) as f64).ln() - (j as f64).ln();
        }
        lower_tail += (log_choose - (trials as f64) * std::f64::consts::LN_2).exp();
    }
    (2.0 * lower_tail).min(1.0)
}

fn mutate_prepared(
    mut prepared: Vec<u8>,
    pointer: &[u8; IMAGE_HIDDEN_POINTER_BYTES],
    _check_mark: &[u8; IMAGE_HIDDEN_CHECK_MARK_BYTES],
    mutation: Mutation,
) -> Result<Vec<u8>, String> {
    match mutation {
        Mutation::Clean => {}
        Mutation::TrailingPointer => prepared.extend_from_slice(pointer),
        Mutation::VisibleBarcode | Mutation::LowContrastWatermark => {
            let decoded = independent_decode_png(&prepared)?;
            let mut pixels = decoded.pixels;
            let width = decoded.width as usize;
            let height = decoded.height as usize;
            let y0 = height.saturating_sub(42);
            for y in y0..height.saturating_sub(8) {
                for x in 16..width.saturating_sub(16) {
                    let offset = (y * width + x) * 3;
                    match mutation {
                        Mutation::VisibleBarcode => {
                            if (x / 5) % 2 == 0 {
                                pixels[offset] = 0;
                                pixels[offset + 1] = 0;
                            } else {
                                pixels[offset] = 255;
                                pixels[offset + 1] = 255;
                            }
                        }
                        Mutation::LowContrastWatermark => {
                            if ((x / 9) + (y / 6)) % 3 == 0 {
                                pixels[offset] = pixels[offset].saturating_add(12);
                                pixels[offset + 1] = pixels[offset + 1].saturating_add(12);
                            }
                        }
                        _ => unreachable!(),
                    }
                }
            }
            prepared = write_png_rgb(decoded.width, decoded.height, &pixels);
        }
    }
    Ok(prepared)
}

fn build_positive_controls(pairs: &[PreparedPair]) -> Result<Vec<PreparedPair>, String> {
    pairs
        .iter()
        .cloned()
        .map(|mut pair| {
            pair.prepared = mutate_prepared(
                pair.prepared,
                &pair.source.pointer,
                &pair.source.check_mark,
                Mutation::VisibleBarcode,
            )?;
            Ok(pair)
        })
        .collect()
}

fn preregistered_corpus() -> Vec<CorpusImage> {
    let mut corpus = Vec::with_capacity(CORPUS_SIZE);
    for (class_index, class) in CLASSES.iter().copied().enumerate() {
        for within_class in 0..IMAGES_PER_CLASS {
            let (width, height) = if within_class % 2 == 0 {
                (WIDTH_LANDSCAPE, HEIGHT_LANDSCAPE)
            } else {
                (WIDTH_PORTRAIT, HEIGHT_PORTRAIT)
            };
            let pixels = source_pixels(class, class_index, within_class, width, height);
            corpus.push(CorpusImage {
                id: format!("prereg-{class}-{:02}", within_class + 1),
                class,
                original: write_png_rgb(width, height, &pixels),
                pointer: unique_pointer(class_index, within_class),
                check_mark: [0x06, 0x64, class_index as u8, within_class as u8],
            });
        }
    }
    corpus
}

fn unique_pointer(class_index: usize, within_class: usize) -> [u8; IMAGE_HIDDEN_POINTER_BYTES] {
    let mut pointer = [0u8; IMAGE_HIDDEN_POINTER_BYTES];
    let mut state = mix64(0x0664_a000_u64 ^ ((class_index as u64) << 16) ^ within_class as u64);
    for (index, byte) in pointer.iter_mut().enumerate() {
        if index % 8 == 0 {
            state = mix64(state ^ index as u64);
        }
        *byte = (state >> ((index % 8) * 8)) as u8;
    }
    pointer[0] = 0x66;
    pointer[1] = class_index as u8;
    pointer[2] = within_class as u8;
    pointer
}

fn source_pixels(
    class: &str,
    class_index: usize,
    variant: usize,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let mut pixels = vec![0u8; (width * height * 3) as usize];
    let mut noise = mix64(0x5eed_0664 ^ ((class_index as u64) << 32) ^ variant as u64);
    for y in 0..height {
        for x in 0..width {
            noise = mix64(noise ^ (u64::from(x) << 20) ^ u64::from(y));
            let grain = (noise & 0x1f) as u8;
            let (r, g, b) = match class {
                "photograph" => {
                    let sky = 80 + ((u64::from(y) * 90 / u64::from(height)) as u8);
                    let ridge = height / 2 + ((x.wrapping_mul(13 + variant as u32) % 41) / 3);
                    if y < ridge {
                        (
                            sky.saturating_add(grain / 4),
                            130 + grain / 3,
                            180 + grain / 3,
                        )
                    } else {
                        (70 + grain, 100 + grain, 45 + grain / 2)
                    }
                }
                "face" => {
                    let cx = width as i64 / 2;
                    let cy = height as i64 / 2;
                    let dx = x as i64 - cx;
                    let dy = y as i64 - cy;
                    let inside = dx * dx * 4 + dy * dy * 3 < (width.min(height) as i64).pow(2);
                    if inside {
                        let eye = dy.abs() < 7 && (dx.abs() - width as i64 / 7).abs() < 9;
                        let mouth = dy > height as i64 / 7
                            && dy < height as i64 / 7 + 5
                            && dx.abs() < width as i64 / 8;
                        if eye || mouth {
                            (45, 30, 28)
                        } else {
                            (190 + grain / 2, 132 + grain / 2, 105 + grain / 3)
                        }
                    } else {
                        (45 + grain / 2, 66 + grain / 2, 83 + grain / 2)
                    }
                }
                "text" => {
                    let line = y > 24 && y % 28 < 11 && x > 20 && x < width - 20;
                    let glyph_gap = (x / (7 + variant as u32 % 3)) % 5 == 0;
                    if line && !glyph_gap {
                        (28, 30, 36)
                    } else {
                        (244, 242, 235)
                    }
                }
                "flat_graphic" => {
                    let tile = ((x / (32 + variant as u32)) + (y / 30)) % 4;
                    [
                        (31, 112, 168),
                        (238, 108, 77),
                        (247, 200, 74),
                        (61, 166, 121),
                    ][tile as usize]
                }
                "gradient" => {
                    let rx = (u64::from(x) * 255 / u64::from(width - 1)) as u8;
                    let gy = (u64::from(y) * 255 / u64::from(height - 1)) as u8;
                    (rx, gy, rx / 2 + gy / 2)
                }
                "dark_region" => {
                    let base = ((x + y + variant as u32 * 3) % 18) as u8;
                    (base, base.saturating_add(4), base.saturating_add(8))
                }
                "noisy_region" => (
                    grain.wrapping_mul(7),
                    (noise >> 13) as u8,
                    (noise >> 29) as u8,
                ),
                _ => unreachable!(),
            };
            let offset = ((y * width + x) * 3) as usize;
            pixels[offset] = r;
            pixels[offset + 1] = g;
            pixels[offset + 2] = b;
        }
    }
    // A preregistered, content-neutral corner swatch keeps every source byte
    // identity unique even for classes whose visual recipe is intentionally
    // invariant across variants (for example, a smooth gradient).
    pixels[0] = pixels[0].wrapping_add((class_index as u8).wrapping_mul(17));
    pixels[1] = pixels[1].wrapping_add((variant as u8).wrapping_mul(19));
    pixels[2] ^= ((class_index * IMAGES_PER_CLASS + variant) as u8).wrapping_mul(23);
    pixels
}

fn write_png_rgb(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    assert_eq!(pixels.len(), (width * height * 3) as usize);
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(Cursor::new(&mut bytes), width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .expect("fixture PNG header writes")
            .write_image_data(pixels)
            .expect("fixture PNG pixels write");
    }
    bytes
}

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn print_report(report: &AcceptanceReport, mutation: Mutation) {
    println!("TASK0664A_MUTATION={}", mutation.name());
    println!(
        "TASK0664A_CORPUS originals={} prepared={} nonempty=56 unique_originals=56 unique_prepared=56",
        report.originals, report.prepared
    );
    println!(
        "TASK0664A_POINTER_READBACKS={} independent_pointer_readbacks={} exact_unique=true",
        report.pointer_readbacks, report.independent_pointer_readbacks
    );
    println!(
        "TASK0664A_STRUCTURE pointer_metadata={} extra_frames_or_chunks={} trailing_payloads={} dimensions_preserved=56 orientations_preserved=56 independent_decodes=112",
        report.structure.pointer_metadata,
        report.structure.extra_chunks_or_frames,
        report.structure.trailing_payloads
    );
    println!(
        "TASK0664A_BLIND observers=24 builder_independent=24 randomisation_records={} raw_answers={} provenance=retained_model_fixture_not_recruited_human",
        report.randomisation_records, report.raw_answers
    );
    println!(
        "TASK0664A_BINOMIAL overall={}/{} p={:.12} alpha=0.05 two_sided=true indistinguishable=true",
        report.overall_identifications, report.overall_trials, report.overall_p
    );
    for (class, identified, trials, p) in &report.class_results {
        println!(
            "TASK0664A_CLASS class={class} identified={identified}/{trials} p={p:.12} indistinguishable=true"
        );
    }
    println!(
        "TASK0664A_POSITIVE_CONTROL correct={}/{} rate={:.6} minimum=0.90 equal_size=true",
        report.positive_correct,
        report.positive_trials,
        report.positive_correct as f64 / report.positive_trials as f64
    );
    println!("TASK0664A_PROVIDER_SURVIVAL={}", report.provider_survival);
    println!("TASK0664A_PROVIDER_LOCAL_PREPARED_COPY=false");
    println!("TASK0664A_ACCEPTANCE=PASS");
}
