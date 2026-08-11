//! TASK 5130 fail-closed Messenger composer-reference oracle.
//!
//! Release validation is deliberately separate from image diffing.  Every live
//! carrier, inventory, provenance, rectangle, seam, hash and colour invariant
//! is established before a caller can obtain [`VerifiedComposerReferences`].

use png::{BitDepth, ColorType, Decoder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

pub const CONTRACT_SCHEMA: &str = "osl-messenger-composer-contract-v1";
pub const CENSUS_SCHEMA: &str = "osl-messenger-live-census-v1";
pub const MANIFEST_SCHEMA: &str = "osl-messenger-composer-reference-v1";
pub const MESSENGER_ORIGIN: &str = "https://www.messenger.com";
pub const PROBE_TEXT: &str = "Task 5130 benign composer probe";
pub const COMPOSER_STATE: &str = "ordinary-unprotected-probe";
pub const CAPTURES_PER_KEY: usize = 5;
pub const CAPTURE_COLOUR_FLOOR: usize = 32;
pub const ROI_COLOUR_FLOOR_EXCLUSIVE: usize = 2;
pub const SEAM_RING_PX: i32 = 4;

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerKey {
    pub origin: String,
    pub channel: String,
    pub composer_state: String,
}

impl fmt::Display for ComposerKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}/{}/{}",
            self.origin, self.channel, self.composer_state
        )
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Contract {
    schema: String,
    origin: String,
    benign_probe_text: String,
    capture_count_per_key: usize,
    minimum_capture_distinct_rgb_colours: usize,
    minimum_roi_distinct_rgb_colours_exclusive: usize,
    seam_ring_px: i32,
    keys: Vec<ComposerKey>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Census {
    schema: String,
    observed_at_utc: String,
    source: String,
    independence: String,
    windows_version: String,
    windows_build: String,
    tasklist_process_count: usize,
    visible_browser_window_count: usize,
    installed_channels: Vec<String>,
    observed_composer_count: usize,
    observed_composers: Vec<ComposerKey>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl Rect {
    fn width(self) -> Option<u32> {
        u32::try_from(self.right.checked_sub(self.left)?)
            .ok()
            .filter(|v| *v > 0)
    }

    fn height(self) -> Option<u32> {
        u32::try_from(self.bottom.checked_sub(self.top)?)
            .ok()
            .filter(|v| *v > 0)
    }

    fn expand(self, amount: i32) -> Option<Self> {
        Some(Self {
            left: self.left.checked_sub(amount)?,
            top: self.top.checked_sub(amount)?,
            right: self.right.checked_add(amount)?,
            bottom: self.bottom.checked_add(amount)?,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BrowserReceipt {
    browser_name: String,
    executable_sha256: String,
    signer: String,
    signature_status: String,
    profile_id: String,
    profile_path: String,
    independently_signed_in: bool,
    carrier_account_id: String,
    hwnd: u64,
    hwnd_generation: u64,
    foreground: bool,
    visible: bool,
    occluded: bool,
    origin: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ShippingReceipt {
    build_kind: String,
    executable_name: String,
    executable_sha256: String,
    signer: String,
    signature_status: String,
    integration_enabled: bool,
    unique_marker: String,
    carrier_visible_before: String,
    carrier_visible_after: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReviewReceipt {
    capture_author: String,
    reviewer: String,
    reviewer_key_id: String,
    reviewed_at_utc: String,
    signature_hex: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CaptureRecord {
    ordinal: usize,
    png_path: String,
    png_sha256: String,
    roi: Rect,
    captured_bounds: Rect,
    seam_ring_px: i32,
    seam_ring_sha256: String,
    distinct_rgb_colours: usize,
    roi_distinct_rgb_colours: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReferenceManifest {
    schema: String,
    key: ComposerKey,
    benign_probe_text: String,
    source_kind: String,
    capture_method: String,
    uia_provider: String,
    observation_tools: Vec<String>,
    route_kind: String,
    catalogue_only: bool,
    hidden_overlay: bool,
    test_route: bool,
    fixture_specific_distinct_colour_floor: usize,
    browser: BrowserReceipt,
    shipping_integration: ShippingReceipt,
    review: ReviewReceipt,
    captures: Vec<CaptureRecord>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValidationMode {
    Release,
    Fixture,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedComposerReferences {
    pub keys: BTreeSet<ComposerKey>,
    pub captures: usize,
    pub minimum_capture_colours: usize,
    pub minimum_roi_colours: usize,
}

#[derive(Debug, Eq, PartialEq)]
pub struct OracleError(pub String);

impl fmt::Display for OracleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for OracleError {}

fn fail(reason: impl Into<String>) -> OracleError {
    OracleError(format!(
        "missing carrier state before diffing: {}",
        reason.into()
    ))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path, label: &str) -> Result<T, OracleError> {
    let bytes = fs::read(path).map_err(|e| fail(format!("{label} {}: {e}", path.display())))?;
    // Windows PowerShell 5.1 writes UTF-8 evidence with a BOM by default.
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
    serde_json::from_slice(bytes)
        .map_err(|e| fail(format!("malformed {label} {}: {e}", path.display())))
}

fn exact_set<T: Ord + Clone + fmt::Display>(
    values: &[T],
    label: &str,
) -> Result<BTreeSet<T>, OracleError> {
    let set: BTreeSet<_> = values.iter().cloned().collect();
    if set.len() != values.len() {
        return Err(fail(format!("duplicate {label}")));
    }
    Ok(set)
}

fn describe_set_difference(
    expected: &BTreeSet<ComposerKey>,
    actual: &BTreeSet<ComposerKey>,
    actual_label: &str,
) -> OracleError {
    if let Some(key) = expected.difference(actual).next() {
        fail(format!("{actual_label} is missing {key}"))
    } else if let Some(key) = actual.difference(expected).next() {
        fail(format!("{actual_label} has uncontracted state {key}"))
    } else {
        fail(format!("{actual_label} inventory disagrees"))
    }
}

fn validate_contract(path: &Path) -> Result<(Contract, BTreeSet<ComposerKey>), OracleError> {
    let contract: Contract = read_json(path, "shipped composer contract")?;
    if contract.schema != CONTRACT_SCHEMA
        || contract.origin != MESSENGER_ORIGIN
        || contract.benign_probe_text != PROBE_TEXT
        || contract.capture_count_per_key != CAPTURES_PER_KEY
        || contract.minimum_capture_distinct_rgb_colours != CAPTURE_COLOUR_FLOOR
        || contract.minimum_roi_distinct_rgb_colours_exclusive != ROI_COLOUR_FLOOR_EXCLUSIVE
        || contract.seam_ring_px != SEAM_RING_PX
    {
        return Err(fail("shipped composer contract constants changed"));
    }
    let keys = exact_set(&contract.keys, "contract key")?;
    if keys.is_empty()
        || keys.iter().any(|key| {
            key.origin != contract.origin
                || key.composer_state != COMPOSER_STATE
                || key.channel.trim().is_empty()
        })
    {
        return Err(fail("shipped composer contract has an invalid key"));
    }
    Ok((contract, keys))
}

fn validate_census(path: &Path, contract_keys: &BTreeSet<ComposerKey>) -> Result<(), OracleError> {
    let census: Census = read_json(path, "independent live-carrier census")?;
    if census.schema != CENSUS_SCHEMA
        || census.source != "windows-powershell-uia-interactive-session"
        || census.independence
            != "created-before-and-without-reading-composer-contract-or-manifests"
        || !census.observed_at_utc.starts_with("2026-")
        || census.windows_version.trim().is_empty()
        || census.windows_build.trim().is_empty()
        || census.tasklist_process_count == 0
        || census.visible_browser_window_count == 0
    {
        return Err(fail(
            "dated independent Windows live-carrier census receipt",
        ));
    }
    if census.observed_composer_count != census.observed_composers.len() {
        return Err(fail("live census composer count disagrees"));
    }
    let census_keys = exact_set(&census.observed_composers, "live census composer key")?;
    if &census_keys != contract_keys {
        return Err(describe_set_difference(
            contract_keys,
            &census_keys,
            "live census",
        ));
    }
    let installed: BTreeSet<_> = census.installed_channels.iter().cloned().collect();
    if installed.len() != census.installed_channels.len() {
        return Err(fail("duplicate live census installed channel"));
    }
    let expected_channels: BTreeSet<_> = contract_keys.iter().map(|k| k.channel.clone()).collect();
    if installed != expected_channels {
        let missing = expected_channels
            .difference(&installed)
            .next()
            .cloned()
            .unwrap_or_else(|| "unexpected-channel".to_owned());
        return Err(fail(format!("live census installed channel {missing}")));
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

struct DecodedImage {
    width: u32,
    height: u32,
    rgb: Vec<[u8; 3]>,
}

fn decode_png(bytes: &[u8], label: &str) -> Result<DecodedImage, OracleError> {
    let decoder = Decoder::new(Cursor::new(bytes));
    let mut reader = decoder
        .read_info()
        .map_err(|e| fail(format!("{label} PNG: {e}")))?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|e| fail(format!("{label} PNG pixels: {e}")))?;
    if info.bit_depth != BitDepth::Eight {
        return Err(fail(format!("{label} PNG is not 8-bit RGB/RGBA")));
    }
    let channels = match info.color_type {
        ColorType::Rgb => 3,
        ColorType::Rgba => 4,
        _ => return Err(fail(format!("{label} PNG is not RGB/RGBA"))),
    };
    let rgb = buffer[..info.buffer_size()]
        .chunks_exact(channels)
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    Ok(DecodedImage {
        width: info.width,
        height: info.height,
        rgb,
    })
}

fn colour_count<'a>(pixels: impl Iterator<Item = &'a [u8; 3]>) -> usize {
    pixels.copied().collect::<BTreeSet<_>>().len()
}

fn validate_image(
    root: &Path,
    key: &ComposerKey,
    record: &CaptureRecord,
    floor: usize,
) -> Result<(String, usize, usize), OracleError> {
    if record.ordinal == 0 || record.ordinal > CAPTURES_PER_KEY {
        return Err(fail(format!("{key} capture ordinal {}", record.ordinal)));
    }
    if Path::new(&record.png_path).is_absolute()
        || Path::new(&record.png_path)
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(fail(format!(
            "{key} capture path escapes manifest directory"
        )));
    }
    let path = root.join(&record.png_path);
    let bytes =
        fs::read(&path).map_err(|e| fail(format!("{key} capture {}: {e}", path.display())))?;
    if !is_sha256(&record.png_sha256) || hash(&bytes) != record.png_sha256 {
        return Err(fail(format!("{key} capture {} PNG hash", record.ordinal)));
    }
    if record.seam_ring_px != SEAM_RING_PX
        || record.roi.expand(SEAM_RING_PX) != Some(record.captured_bounds)
    {
        return Err(fail(format!(
            "{key} capture {} exact ROI plus 4px seam",
            record.ordinal
        )));
    }
    let image = decode_png(&bytes, &format!("{key} capture {}", record.ordinal))?;
    if record.captured_bounds.width() != Some(image.width)
        || record.captured_bounds.height() != Some(image.height)
    {
        return Err(fail(format!(
            "{key} capture {} bounds/PNG dimensions",
            record.ordinal
        )));
    }
    let width = image.width as usize;
    let height = image.height as usize;
    let seam = SEAM_RING_PX as usize;
    if width <= seam * 2 || height <= seam * 2 {
        return Err(fail(format!("{key} capture {} has no ROI", record.ordinal)));
    }
    let all_colours = colour_count(image.rgb.iter());
    let roi_colours = colour_count(image.rgb.iter().enumerate().filter_map(|(index, pixel)| {
        let x = index % width;
        let y = index / width;
        (x >= seam && x < width - seam && y >= seam && y < height - seam).then_some(pixel)
    }));
    let seam_bytes: Vec<_> = image
        .rgb
        .iter()
        .enumerate()
        .filter(|(index, _)| {
            let x = index % width;
            let y = index / width;
            x < seam || x >= width - seam || y < seam || y >= height - seam
        })
        .flat_map(|(_, pixel)| pixel.iter().copied())
        .collect();
    let seam_hash = hash(&seam_bytes);
    if !is_sha256(&record.seam_ring_sha256) || record.seam_ring_sha256 != seam_hash {
        return Err(fail(format!(
            "{key} capture {} untouched seam ring",
            record.ordinal
        )));
    }
    if all_colours != record.distinct_rgb_colours
        || all_colours < floor
        || all_colours < CAPTURE_COLOUR_FLOOR
    {
        return Err(fail(format!(
            "{key} capture {} distinct RGB colours {} below floor {}",
            record.ordinal,
            all_colours,
            floor.max(CAPTURE_COLOUR_FLOOR)
        )));
    }
    if roi_colours != record.roi_distinct_rgb_colours || roi_colours <= ROI_COLOUR_FLOOR_EXCLUSIVE {
        return Err(fail(format!(
            "{key} capture {} ROI colours {} must be above {}",
            record.ordinal, roi_colours, ROI_COLOUR_FLOOR_EXCLUSIVE
        )));
    }
    Ok((seam_hash, all_colours, roi_colours))
}

fn manifest_paths(root: &Path) -> Result<Vec<PathBuf>, OracleError> {
    let entries = fs::read_dir(root).map_err(|e| {
        fail(format!(
            "reference manifest directory {}: {e}",
            root.display()
        ))
    })?;
    let mut paths = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|e| fail(format!("manifest directory entry: {e}")))?
            .path();
        if path.extension().and_then(|v| v.to_str()) == Some("json") {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn validate_manifest(
    path: &Path,
    root: &Path,
    mode: ValidationMode,
    unique_markers: &mut BTreeSet<String>,
    referenced_pngs: &mut BTreeSet<PathBuf>,
) -> Result<(ComposerKey, usize, usize), OracleError> {
    let manifest: ReferenceManifest = read_json(path, "composer reference manifest")?;
    let key = &manifest.key;
    if manifest.schema != MANIFEST_SCHEMA || manifest.benign_probe_text != PROBE_TEXT {
        return Err(fail(format!("{key} exact benign probe contract")));
    }
    match mode {
        ValidationMode::Release => {
            if manifest.source_kind != "live-carrier-windows-release"
                || manifest.capture_method != "windows-powershell-uia-copyfromscreen"
            {
                return Err(fail(format!(
                    "{key} live Windows PowerShell/UIA/CopyFromScreen source"
                )));
            }
        }
        ValidationMode::Fixture => {
            if manifest.source_kind != "fixture" || manifest.capture_method != "fixture-png" {
                return Err(fail(format!("{key} fixture source declaration")));
            }
        }
    }
    let expected_tools = ["tasklist.exe", "Windows PowerShell", "CopyFromScreen"];
    if mode == ValidationMode::Release
        && manifest
            .observation_tools
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            != expected_tools.to_vec()
    {
        return Err(fail(format!("{key} Windows observation tools")));
    }
    if manifest.uia_provider != "Windows UI Automation"
        || manifest.route_kind != "live-carrier"
        || manifest.catalogue_only
        || manifest.hidden_overlay
        || manifest.test_route
    {
        return Err(fail(format!("{key} visible live carrier route")));
    }
    let browser = &manifest.browser;
    if browser.browser_name.trim().is_empty()
        || !is_sha256(&browser.executable_sha256)
        || browser.signer.trim().is_empty()
        || browser.signature_status != "valid"
        || browser.profile_id.trim().is_empty()
        || browser.profile_path.trim().is_empty()
        || !browser.independently_signed_in
        || browser.carrier_account_id.trim().is_empty()
        || browser.hwnd == 0
        || browser.hwnd_generation == 0
        || !browser.foreground
        || !browser.visible
        || browser.occluded
        || browser.origin != MESSENGER_ORIGIN
    {
        return Err(fail(format!(
            "{key} verified signed browser/profile/HWND binding"
        )));
    }
    let shipping = &manifest.shipping_integration;
    if shipping.build_kind != "shipping-windows-release"
        || shipping.executable_name.trim().is_empty()
        || !is_sha256(&shipping.executable_sha256)
        || shipping.signer.trim().is_empty()
        || shipping.signature_status != "valid"
        || !shipping.integration_enabled
        || shipping.unique_marker.trim().is_empty()
        || shipping.carrier_visible_before.trim().is_empty()
        || shipping.carrier_visible_after.trim().is_empty()
        || shipping.carrier_visible_before == shipping.carrier_visible_after
        || !shipping
            .carrier_visible_after
            .contains(&shipping.unique_marker)
    {
        return Err(fail(format!(
            "{key} shipping integration marker and carrier-visible before/after state"
        )));
    }
    if !unique_markers.insert(shipping.unique_marker.clone()) {
        return Err(fail(format!("{key} unique shipping integration marker")));
    }
    let review = &manifest.review;
    if review.capture_author.trim().is_empty()
        || review.reviewer.trim().is_empty()
        || review.capture_author == review.reviewer
        || review.reviewer_key_id.trim().is_empty()
        || !review.reviewed_at_utc.starts_with("2026-")
        || review.signature_hex.len() != 128
        || !review
            .signature_hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(fail(format!(
            "{key} independently reviewed manifest receipt"
        )));
    }
    let floor = manifest.fixture_specific_distinct_colour_floor;
    if floor < CAPTURE_COLOUR_FLOOR || manifest.captures.len() != CAPTURES_PER_KEY {
        return Err(fail(format!(
            "{key} five-capture fixture-specific colour floor"
        )));
    }
    let mut ordinals = BTreeSet::new();
    let mut seam_hash: Option<String> = None;
    let mut min_capture = usize::MAX;
    let mut min_roi = usize::MAX;
    for record in &manifest.captures {
        if !ordinals.insert(record.ordinal) {
            return Err(fail(format!(
                "{key} duplicate capture ordinal {}",
                record.ordinal
            )));
        }
        if !referenced_pngs.insert(PathBuf::from(&record.png_path)) {
            return Err(fail(format!(
                "{key} capture PNG is reused by another reference"
            )));
        }
        let (this_seam, colours, roi_colours) = validate_image(root, key, record, floor)?;
        if seam_hash
            .as_ref()
            .is_some_and(|expected| expected != &this_seam)
        {
            return Err(fail(format!(
                "{key} seam ring changed across five captures"
            )));
        }
        seam_hash = Some(this_seam);
        min_capture = min_capture.min(colours);
        min_roi = min_roi.min(roi_colours);
    }
    if ordinals != (1..=CAPTURES_PER_KEY).collect() {
        return Err(fail(format!("{key} capture ordinal set")));
    }
    Ok((manifest.key, min_capture, min_roi))
}

fn validate_store_inventory(
    root: &Path,
    referenced_pngs: &BTreeSet<PathBuf>,
) -> Result<(), OracleError> {
    let mut stored_pngs = BTreeSet::new();
    for entry in
        fs::read_dir(root).map_err(|e| fail(format!("reference store {}: {e}", root.display())))?
    {
        let entry = entry.map_err(|e| fail(format!("reference store entry: {e}")))?;
        let path = entry.path();
        if !path.is_file() {
            return Err(fail(format!(
                "reference store contains non-file {}",
                path.display()
            )));
        }
        match path.extension().and_then(|value| value.to_str()) {
            Some("json") => {}
            Some("png") => {
                stored_pngs.insert(PathBuf::from(entry.file_name()));
            }
            _ => {
                return Err(fail(format!(
                    "reference store contains non-ROI artifact {}",
                    path.display()
                )))
            }
        }
    }
    if &stored_pngs != referenced_pngs {
        let name = stored_pngs
            .symmetric_difference(referenced_pngs)
            .next()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "unknown PNG".to_owned());
        return Err(fail(format!("reference store PNG inventory {name}")));
    }
    Ok(())
}

/// Validate all release prerequisites. A caller must obtain this token before
/// reading a reference for visual diffing.
pub fn verify_composer_references(
    census_path: &Path,
    contract_path: &Path,
    manifest_root: &Path,
    mode: ValidationMode,
) -> Result<VerifiedComposerReferences, OracleError> {
    let (_contract, contract_keys) = validate_contract(contract_path)?;
    validate_census(census_path, &contract_keys)?;
    let paths = manifest_paths(manifest_root)?;
    let mut manifest_keys = BTreeSet::new();
    let mut markers = BTreeSet::new();
    let mut referenced_pngs = BTreeSet::new();
    let mut captures = 0;
    let mut minimum_capture_colours = usize::MAX;
    let mut minimum_roi_colours = usize::MAX;
    for path in paths {
        let (key, capture_min, roi_min) = validate_manifest(
            &path,
            manifest_root,
            mode,
            &mut markers,
            &mut referenced_pngs,
        )?;
        if !manifest_keys.insert(key.clone()) {
            return Err(fail(format!("duplicate manifest for {key}")));
        }
        captures += CAPTURES_PER_KEY;
        minimum_capture_colours = minimum_capture_colours.min(capture_min);
        minimum_roi_colours = minimum_roi_colours.min(roi_min);
    }
    if manifest_keys != contract_keys {
        return Err(describe_set_difference(
            &contract_keys,
            &manifest_keys,
            "reference manifests",
        ));
    }
    validate_store_inventory(manifest_root, &referenced_pngs)?;
    Ok(VerifiedComposerReferences {
        keys: manifest_keys,
        captures,
        minimum_capture_colours,
        minimum_roi_colours,
    })
}
