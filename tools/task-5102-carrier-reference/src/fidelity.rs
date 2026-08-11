//! Regional carrier-fidelity comparator (Task 5103).
//!
//! The comparator deliberately has no resize, arbitrary-mask, or aggregate
//! score path. All inputs are validated before the first `SCORE` line exists.

use png::{BitDepth, ColorType, Transformations};
use serde::Deserialize;
use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

const SEAM_RING_PX: u32 = 4;
const GLYPH_EXPANSION_PX: u32 = 2;
const STRUCTURE_LIMIT_PCT: f64 = 0.5;
const TILE_LIMIT_PCT: f64 = 5.0;
const RAW_LIMIT_PCT: f64 = 0.5;
const FLAT_MEDIAN_LIMIT: f64 = 1.0;
const FLAT_P99_LIMIT: f64 = 2.3;
const STRUCTURE_DIFF_THRESHOLD: f64 = 18.0;
const PERCEPTUAL_THRESHOLD: f64 = 0.1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Rect {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

impl Rect {
    fn width(self) -> Option<u32> {
        self.right.checked_sub(self.left).filter(|value| *value > 0)
    }

    fn height(self) -> Option<u32> {
        self.bottom.checked_sub(self.top).filter(|value| *value > 0)
    }

    fn contains(self, other: Self) -> bool {
        self.width().is_some()
            && self.height().is_some()
            && other.width().is_some()
            && other.height().is_some()
            && other.left >= self.left
            && other.top >= self.top
            && other.right <= self.right
            && other.bottom <= self.bottom
    }

    fn contains_point(self, x: u32, y: u32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }

    fn expand(self, amount: u32, width: u32, height: u32) -> Option<Self> {
        let expected_right = self.right.checked_add(amount)?;
        let expected_bottom = self.bottom.checked_add(amount)?;
        Some(Self {
            left: self.left.checked_sub(amount)?,
            top: self.top.checked_sub(amount)?,
            right: expected_right.min(width),
            bottom: expected_bottom.min(height),
        })
        .filter(|rect| rect.right == expected_right && rect.bottom == expected_bottom)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PhysicalRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl PhysicalRect {
    fn width(self) -> Option<u32> {
        u32::try_from(self.right.checked_sub(self.left)?)
            .ok()
            .filter(|value| *value > 0)
    }

    fn height(self) -> Option<u32> {
        u32::try_from(self.bottom.checked_sub(self.top)?)
            .ok()
            .filter(|value| *value > 0)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum TextRelation {
    ExactBenignProbe,
    ProtectedTextDiffers,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FidelityManifest {
    schema: String,
    seam_ring_physical_px: u32,
    glyph_expansion_physical_px: u32,
    required_channels: Vec<String>,
    required_states: Vec<String>,
    comparisons: Vec<Comparison>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Comparison {
    channel: String,
    state: String,
    reference_png: PathBuf,
    candidate_png: PathBuf,
    physical_width: u32,
    physical_height: u32,
    text_relation: TextRelation,
    known_good_capture_count: usize,
    known_good_distinct_min: usize,
    known_good_distinct_max: usize,
    owned_rois: Vec<OwnedRoi>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnedRoi {
    name: String,
    roi: Rect,
    reference_boundary: PhysicalRect,
    candidate_boundary: PhysicalRect,
    reference_baseline_y: i32,
    candidate_baseline_y: i32,
    flat_fill_rects: Vec<Rect>,
    #[serde(default)]
    uia_text_ranges: Vec<Rect>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Pixel {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

#[derive(Clone)]
struct Image {
    width: u32,
    height: u32,
    pixels: Vec<Pixel>,
    mode: &'static str,
    distinct_rgb: usize,
}

impl Image {
    fn at(&self, x: u32, y: u32) -> Pixel {
        self.pixels[(y * self.width + x) as usize]
    }

    fn index(&self, x: u32, y: u32) -> usize {
        (y * self.width + x) as usize
    }
}

struct PreparedComparison<'a> {
    spec: &'a Comparison,
    reference: Image,
    candidate: Image,
}

/// Validate every input, then score every required state/channel independently.
pub fn compare_manifest(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("cannot read manifest {}: {error}", path.display()))?;
    let manifest: FidelityManifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid closed-schema manifest: {error}"))?;
    validate_manifest_shape(&manifest)?;

    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let mut prepared = Vec::with_capacity(manifest.comparisons.len());
    for comparison in &manifest.comparisons {
        let reference_path = base.join(&comparison.reference_png);
        let candidate_path = base.join(&comparison.candidate_png);
        let reference_canonical = std::fs::canonicalize(&reference_path).map_err(|error| {
            format!(
                "cannot resolve reference {}: {error}",
                reference_path.display()
            )
        })?;
        let candidate_canonical = std::fs::canonicalize(&candidate_path).map_err(|error| {
            format!(
                "cannot resolve candidate {}: {error}",
                candidate_path.display()
            )
        })?;
        if reference_canonical == candidate_canonical {
            return Err(format!(
                "{} / {} uses the same file for reference and candidate",
                comparison.channel, comparison.state
            ));
        }
        let reference = decode_lossless_png(&reference_path, "reference")?;
        let candidate = decode_lossless_png(&candidate_path, "candidate")?;
        validate_input_pair(comparison, &reference, &candidate)?;
        prepared.push(PreparedComparison {
            spec: comparison,
            reference,
            candidate,
        });
    }

    let mut output = String::new();
    writeln!(
        output,
        "CARRIER FIDELITY envelope: boundary=0px baseline=0px flat_median<1.0 flat_p99<2.3 structure<=0.5% tile16<=5.0% exact_raw<=0.5%"
    )
    .unwrap();
    let mut failures = Vec::new();
    for item in &prepared {
        score_comparison(item, &mut output, &mut failures)?;
    }
    if !failures.is_empty() {
        for failure in &failures {
            writeln!(output, "FAIL {failure}").unwrap();
        }
        return Err(output);
    }
    writeln!(
        output,
        "PASS cases={} channels={} states={} averaging=none",
        prepared.len(),
        manifest.required_channels.len(),
        manifest.required_states.len()
    )
    .unwrap();
    Ok(output)
}

fn validate_manifest_shape(manifest: &FidelityManifest) -> Result<(), String> {
    if manifest.schema != "osl-carrier-fidelity-v1" {
        return Err("unsupported schema (expected osl-carrier-fidelity-v1)".to_owned());
    }
    if manifest.seam_ring_physical_px != SEAM_RING_PX {
        return Err("seam ring must be exactly 4 physical pixels".to_owned());
    }
    if manifest.glyph_expansion_physical_px != GLYPH_EXPANSION_PX {
        return Err("UIA glyph interiors must be expanded by exactly 2 physical pixels".to_owned());
    }
    let channels = unique_nonempty("required_channels", &manifest.required_channels)?;
    let states = unique_nonempty("required_states", &manifest.required_states)?;
    let required: BTreeSet<_> = channels
        .iter()
        .flat_map(|channel| {
            states
                .iter()
                .map(move |state| (channel.clone(), state.clone()))
        })
        .collect();
    let actual: BTreeSet<_> = manifest
        .comparisons
        .iter()
        .map(|item| (item.channel.clone(), item.state.clone()))
        .collect();
    if actual.len() != manifest.comparisons.len() {
        return Err("duplicate state/channel comparison; averaging is forbidden".to_owned());
    }
    if required != actual {
        let missing: Vec<_> = required.difference(&actual).cloned().collect();
        let extra: Vec<_> = actual.difference(&required).cloned().collect();
        return Err(format!(
            "state/channel inventory mismatch; missing={missing:?} extra={extra:?}"
        ));
    }
    for comparison in &manifest.comparisons {
        if comparison.physical_width == 0 || comparison.physical_height == 0 {
            return Err(format!(
                "{} / {} has zero physical dimensions",
                comparison.channel, comparison.state
            ));
        }
        if comparison.known_good_capture_count < 5 {
            return Err(format!(
                "{} / {} distinct-colour floor needs at least 5 reviewed known-good captures",
                comparison.channel, comparison.state
            ));
        }
        if comparison.known_good_distinct_min <= 2
            || comparison.known_good_distinct_max < comparison.known_good_distinct_min
        {
            return Err(format!(
                "{} / {} has an invalid reviewed distinct-colour range",
                comparison.channel, comparison.state
            ));
        }
        if comparison.owned_rois.is_empty() {
            return Err(format!(
                "{} / {} owns no ROI",
                comparison.channel, comparison.state
            ));
        }
        let mut names = BTreeSet::new();
        for owned in &comparison.owned_rois {
            if owned.name.is_empty() || !names.insert(&owned.name) {
                return Err(format!(
                    "{} / {} has an empty or duplicate owned ROI name",
                    comparison.channel, comparison.state
                ));
            }
            validate_owned_shape(comparison, owned)?;
        }
    }
    Ok(())
}

fn unique_nonempty(label: &str, values: &[String]) -> Result<BTreeSet<String>, String> {
    if values.is_empty() || values.iter().any(String::is_empty) {
        return Err(format!("{label} must contain non-empty values"));
    }
    let set: BTreeSet<_> = values.iter().cloned().collect();
    if set.len() != values.len() {
        return Err(format!("{label} contains duplicates"));
    }
    Ok(set)
}

fn validate_owned_shape(comparison: &Comparison, owned: &OwnedRoi) -> Result<(), String> {
    let bounds = Rect {
        left: 0,
        top: 0,
        right: comparison.physical_width,
        bottom: comparison.physical_height,
    };
    if !bounds.contains(owned.roi)
        || owned
            .roi
            .expand(
                SEAM_RING_PX,
                comparison.physical_width,
                comparison.physical_height,
            )
            .is_none()
    {
        return Err(format!(
            "owned ROI {} cannot carry an exact 4px seam ring",
            owned.name
        ));
    }
    if owned.reference_boundary.width() != owned.roi.width()
        || owned.reference_boundary.height() != owned.roi.height()
        || owned.candidate_boundary.width() != owned.roi.width()
        || owned.candidate_boundary.height() != owned.roi.height()
    {
        return Err(format!(
            "owned ROI {} boundary dimensions are not bound to its physical ROI",
            owned.name
        ));
    }
    if !(owned.reference_baseline_y >= owned.reference_boundary.top
        && owned.reference_baseline_y < owned.reference_boundary.bottom
        && owned.candidate_baseline_y >= owned.candidate_boundary.top
        && owned.candidate_baseline_y < owned.candidate_boundary.bottom)
    {
        return Err(format!(
            "owned ROI {} baseline lies outside its boundary",
            owned.name
        ));
    }
    if owned.flat_fill_rects.is_empty()
        || owned
            .flat_fill_rects
            .iter()
            .any(|rect| !owned.roi.contains(*rect))
    {
        return Err(format!(
            "owned ROI {} requires reviewed flat-fill samples inside the ROI",
            owned.name
        ));
    }
    let fill_pixels: u64 = owned
        .flat_fill_rects
        .iter()
        .map(|rect| u64::from(rect.width().unwrap_or(0)) * u64::from(rect.height().unwrap_or(0)))
        .sum();
    if fill_pixels < 16 {
        return Err(format!(
            "owned ROI {} flat-fill sample is too small",
            owned.name
        ));
    }
    match comparison.text_relation {
        TextRelation::ExactBenignProbe if !owned.uia_text_ranges.is_empty() => {
            return Err(format!(
                "exact benign probe {} may not mask any pixel",
                owned.name
            ));
        }
        TextRelation::ProtectedTextDiffers if owned.uia_text_ranges.is_empty() => {
            return Err(format!(
                "differing protected text {} requires UIA text ranges",
                owned.name
            ));
        }
        _ => {}
    }
    for range in &owned.uia_text_ranges {
        if !owned.roi.contains(*range) {
            return Err(format!("UIA text range escapes owned ROI {}", owned.name));
        }
        let expanded = range
            .expand(
                GLYPH_EXPANSION_PX,
                comparison.physical_width,
                comparison.physical_height,
            )
            .ok_or_else(|| format!("UIA text range cannot expand inside {}", owned.name))?;
        if !owned.roi.contains(expanded) {
            return Err(format!(
                "expanded UIA glyph mask reaches the seam ring for {}",
                owned.name
            ));
        }
        if owned
            .flat_fill_rects
            .iter()
            .any(|fill| rects_overlap(*fill, expanded))
        {
            return Err(format!(
                "flat-fill sample overlaps protected text mask in {}",
                owned.name
            ));
        }
    }
    Ok(())
}

fn rects_overlap(a: Rect, b: Rect) -> bool {
    a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom
}

fn decode_lossless_png(path: &Path, role: &str) -> Result<Image, String> {
    let file = File::open(path)
        .map_err(|error| format!("cannot open {role} PNG {}: {error}", path.display()))?;
    let mut decoder = png::Decoder::new(BufReader::new(file));
    decoder.set_transformations(Transformations::IDENTITY);
    let mut reader = decoder.read_info().map_err(|error| {
        format!(
            "cannot losslessly decode {role} PNG {}: {error}",
            path.display()
        )
    })?;
    let info = reader.info();
    let (channels, mode) = match (info.color_type, info.bit_depth) {
        (ColorType::Rgb, BitDepth::Eight) => (3usize, "RGB"),
        (ColorType::Rgba, BitDepth::Eight) => (4usize, "RGBA"),
        (color, depth) => {
            return Err(format!(
                "{role} PNG {} has unsupported mode {color:?}/{depth:?}; expected 8-bit RGB/RGBA (1-bit and indexed inputs are forbidden)",
                path.display()
            ));
        }
    };
    let mut bytes = vec![0; reader.output_buffer_size()];
    let frame = reader
        .next_frame(&mut bytes)
        .map_err(|error| format!("cannot decode {role} PNG {}: {error}", path.display()))?;
    if frame.buffer_size() != frame.width as usize * frame.height as usize * channels {
        return Err(format!(
            "{role} PNG {} decoded to an invalid buffer",
            path.display()
        ));
    }
    let mut pixels = Vec::with_capacity(frame.width as usize * frame.height as usize);
    let mut distinct = BTreeSet::new();
    let mut alpha_coverage = 0usize;
    for chunk in bytes[..frame.buffer_size()].chunks_exact(channels) {
        let alpha = if channels == 4 { chunk[3] } else { 255 };
        if alpha > 0 {
            alpha_coverage += 1;
            distinct.insert((chunk[0], chunk[1], chunk[2]));
        }
        pixels.push(Pixel {
            r: chunk[0],
            g: chunk[1],
            b: chunk[2],
            a: alpha,
        });
    }
    if alpha_coverage == 0 {
        return Err(format!("{role} PNG {} is all-transparent", path.display()));
    }
    if distinct.len() <= 2 {
        return Err(format!(
            "{role} PNG {} is degenerate with {} distinct visible RGB colours",
            path.display(),
            distinct.len()
        ));
    }
    Ok(Image {
        width: frame.width,
        height: frame.height,
        pixels,
        mode,
        distinct_rgb: distinct.len(),
    })
}

fn validate_input_pair(
    comparison: &Comparison,
    reference: &Image,
    candidate: &Image,
) -> Result<(), String> {
    if reference.width != candidate.width || reference.height != candidate.height {
        return Err(format!(
            "{} / {} unequal physical dimensions: reference={}x{} candidate={}x{}; resize is forbidden",
            comparison.channel,
            comparison.state,
            reference.width,
            reference.height,
            candidate.width,
            candidate.height
        ));
    }
    if reference.width != comparison.physical_width
        || reference.height != comparison.physical_height
    {
        return Err(format!(
            "{} / {} PNG dimensions do not match the physical manifest dimensions",
            comparison.channel, comparison.state
        ));
    }
    if reference.mode != candidate.mode {
        return Err(format!(
            "{} / {} mode mismatch: reference={} candidate={}",
            comparison.channel, comparison.state, reference.mode, candidate.mode
        ));
    }
    for (role, image) in [("reference", reference), ("candidate", candidate)] {
        if image.distinct_rgb < comparison.known_good_distinct_min
            || image.distinct_rgb > comparison.known_good_distinct_max
        {
            return Err(format!(
                "{} / {} {role} distinct colours {} outside reviewed range {}..={}",
                comparison.channel,
                comparison.state,
                image.distinct_rgb,
                comparison.known_good_distinct_min,
                comparison.known_good_distinct_max
            ));
        }
        for owned in &comparison.owned_rois {
            let mut roi_colours = BTreeSet::new();
            for y in owned.roi.top..owned.roi.bottom {
                for x in owned.roi.left..owned.roi.right {
                    let pixel = image.at(x, y);
                    if pixel.a > 0 {
                        roi_colours.insert((pixel.r, pixel.g, pixel.b));
                    }
                }
            }
            if roi_colours.len() <= 2 {
                return Err(format!(
                    "{} / {} {role} owned ROI {} is degenerate with {} visible colours",
                    comparison.channel,
                    comparison.state,
                    owned.name,
                    roi_colours.len()
                ));
            }
        }
    }
    Ok(())
}

fn score_comparison(
    item: &PreparedComparison<'_>,
    output: &mut String,
    failures: &mut Vec<String>,
) -> Result<(), String> {
    let spec = item.spec;
    for owned in &spec.owned_rois {
        let boundary_displacement =
            boundary_displacement(owned.reference_boundary, owned.candidate_boundary);
        let baseline_displacement =
            abs_i32_diff(owned.reference_baseline_y, owned.candidate_baseline_y);
        let reference_background = dominant_colour(&item.reference, owned.roi);
        let candidate_background = dominant_colour(&item.candidate, owned.roi);
        let mask = derive_mask(
            spec.text_relation,
            owned,
            &item.reference,
            &item.candidate,
            reference_background,
            candidate_background,
        );
        let (masked_reference, masked_candidate) = neutralize_mask(
            &item.reference,
            &item.candidate,
            &mask,
            reference_background,
            candidate_background,
        );
        let raw_map = perceptual_mismatch_map(&masked_reference, &masked_candidate, &mask);
        let blurred_reference = gaussian_blur_9(&masked_reference);
        let blurred_candidate = gaussian_blur_9(&masked_candidate);
        let structure_map = structure_mismatch_map(&blurred_reference, &blurred_candidate, &mask);
        let seam = owned
            .roi
            .expand(SEAM_RING_PX, item.reference.width, item.reference.height)
            .expect("shape was validated");
        let raw_pct = percent_in_rect(&raw_map, &mask, owned.roi, item.reference.width, |_| true);
        let roi_structure = percent_in_rect(
            &structure_map,
            &mask,
            owned.roi,
            item.reference.width,
            |_| true,
        );
        let seam_structure = percent_in_rect(
            &structure_map,
            &mask,
            seam,
            item.reference.width,
            |(x, y)| !owned.roi.contains_point(x, y),
        );
        let tile_max =
            tile_max_percent(&structure_map, &mask, seam, item.reference.width, |_| true);
        let mut flat_deltas = Vec::new();
        for rect in &owned.flat_fill_rects {
            validate_reference_fill(&item.reference, *rect, owned)?;
            for y in rect.top..rect.bottom {
                for x in rect.left..rect.right {
                    flat_deltas.push(delta_e00(
                        rgb_to_lab(item.reference.at(x, y)),
                        rgb_to_lab(item.candidate.at(x, y)),
                    ));
                }
            }
        }
        flat_deltas.sort_by(f64::total_cmp);
        let flat_median = median(&flat_deltas);
        let flat_p99 = percentile_nearest_rank(&flat_deltas, 0.99);
        writeln!(
            output,
            "SCORE channel={} state={} roi={} boundary={}px baseline={}px flat_median={:.4} flat_p99={:.4} structure={:.4}% seam_structure={:.4}% tile16_max={:.4}% perceptual_raw={:.4}% mask={}px",
            spec.channel,
            spec.state,
            owned.name,
            boundary_displacement,
            baseline_displacement,
            flat_median,
            flat_p99,
            roi_structure,
            seam_structure,
            tile_max,
            raw_pct,
            mask.iter().filter(|value| **value).count()
        )
        .unwrap();
        let prefix = format!("{} / {} / {}", spec.channel, spec.state, owned.name);
        if boundary_displacement != 0 {
            failures.push(format!(
                "{prefix}: boundary displacement {boundary_displacement}px"
            ));
        }
        if baseline_displacement != 0 {
            failures.push(format!(
                "{prefix}: baseline displacement {baseline_displacement}px"
            ));
        }
        if flat_median >= FLAT_MEDIAN_LIMIT {
            failures.push(format!(
                "{prefix}: flat-fill median delta-E00 {flat_median:.4}"
            ));
        }
        if flat_p99 >= FLAT_P99_LIMIT {
            failures.push(format!("{prefix}: flat-fill p99 delta-E00 {flat_p99:.4}"));
        }
        if roi_structure > STRUCTURE_LIMIT_PCT {
            failures.push(format!(
                "{prefix}: ROI structure mismatch {roi_structure:.4}%"
            ));
        }
        if seam_structure > STRUCTURE_LIMIT_PCT {
            failures.push(format!(
                "{prefix}: seam structure mismatch {seam_structure:.4}%"
            ));
        }
        if tile_max > TILE_LIMIT_PCT {
            failures.push(format!("{prefix}: 16x16 tile mismatch {tile_max:.4}%"));
        }
        if spec.text_relation == TextRelation::ExactBenignProbe && raw_pct > RAW_LIMIT_PCT {
            failures.push(format!(
                "{prefix}: exact-probe perceptual raw mismatch {raw_pct:.4}%"
            ));
        }
    }
    Ok(())
}

fn boundary_displacement(a: PhysicalRect, b: PhysicalRect) -> u32 {
    [
        abs_i32_diff(a.left, b.left),
        abs_i32_diff(a.top, b.top),
        abs_i32_diff(a.right, b.right),
        abs_i32_diff(a.bottom, b.bottom),
    ]
    .into_iter()
    .max()
    .unwrap_or(0)
}

fn abs_i32_diff(a: i32, b: i32) -> u32 {
    u32::try_from((i64::from(a) - i64::from(b)).unsigned_abs()).unwrap_or(u32::MAX)
}

fn dominant_colour(image: &Image, rect: Rect) -> Pixel {
    let mut counts: HashMap<Pixel, usize> = HashMap::new();
    for y in rect.top..rect.bottom {
        for x in rect.left..rect.right {
            *counts.entry(image.at(x, y)).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .max_by_key(|(pixel, count)| (*count, pixel.r, pixel.g, pixel.b, pixel.a))
        .map(|(pixel, _)| pixel)
        .unwrap_or(Pixel {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        })
}

fn derive_mask(
    relation: TextRelation,
    owned: &OwnedRoi,
    reference: &Image,
    candidate: &Image,
    reference_background: Pixel,
    candidate_background: Pixel,
) -> Vec<bool> {
    let mut glyphs = vec![false; reference.pixels.len()];
    if relation == TextRelation::ExactBenignProbe {
        return glyphs;
    }
    for range in &owned.uia_text_ranges {
        for y in range.top..range.bottom {
            for x in range.left..range.right {
                let reference_ink = delta_e00(
                    rgb_to_lab(reference.at(x, y)),
                    rgb_to_lab(reference_background),
                ) > FLAT_P99_LIMIT;
                let candidate_ink = delta_e00(
                    rgb_to_lab(candidate.at(x, y)),
                    rgb_to_lab(candidate_background),
                ) > FLAT_P99_LIMIT;
                if reference_ink || candidate_ink {
                    glyphs[reference.index(x, y)] = true;
                }
            }
        }
    }
    let mut expanded = glyphs.clone();
    for y in owned.roi.top..owned.roi.bottom {
        for x in owned.roi.left..owned.roi.right {
            if !glyphs[reference.index(x, y)] {
                continue;
            }
            let left = x.saturating_sub(GLYPH_EXPANSION_PX).max(owned.roi.left);
            let top = y.saturating_sub(GLYPH_EXPANSION_PX).max(owned.roi.top);
            let right = (x + GLYPH_EXPANSION_PX + 1).min(owned.roi.right);
            let bottom = (y + GLYPH_EXPANSION_PX + 1).min(owned.roi.bottom);
            for expanded_y in top..bottom {
                for expanded_x in left..right {
                    expanded[reference.index(expanded_x, expanded_y)] = true;
                }
            }
        }
    }
    expanded
}

fn neutralize_mask(
    reference: &Image,
    candidate: &Image,
    mask: &[bool],
    reference_background: Pixel,
    candidate_background: Pixel,
) -> (Image, Image) {
    let mut reference = reference.clone();
    let mut candidate = candidate.clone();
    for (index, masked) in mask.iter().copied().enumerate() {
        if masked {
            reference.pixels[index] = reference_background;
            candidate.pixels[index] = candidate_background;
        }
    }
    (reference, candidate)
}

fn perceptual_mismatch_map(reference: &Image, candidate: &Image, mask: &[bool]) -> Vec<bool> {
    let max_delta = 35_215.0 * PERCEPTUAL_THRESHOLD * PERCEPTUAL_THRESHOLD;
    reference
        .pixels
        .iter()
        .zip(&candidate.pixels)
        .enumerate()
        .map(|(index, (a, b))| {
            if mask[index] || a == b {
                false
            } else if a.a.abs_diff(b.a) > 2 {
                true
            } else {
                yiq_delta(*a, *b).abs() > max_delta
                    && !antialiased(reference, candidate, index)
                    && !antialiased(candidate, reference, index)
            }
        })
        .collect()
}

fn yiq_delta(a: Pixel, b: Pixel) -> f64 {
    let dr = f64::from(a.r) - f64::from(b.r);
    let dg = f64::from(a.g) - f64::from(b.g);
    let db = f64::from(a.b) - f64::from(b.b);
    let y = dr * 0.298_895_31 + dg * 0.586_622_47 + db * 0.114_482_23;
    let i = dr * 0.595_977_99 - dg * 0.274_176_10 - db * 0.321_801_89;
    let q = dr * 0.211_470_17 - dg * 0.522_617_11 + db * 0.311_146_94;
    let delta = 0.5053 * y * y + 0.299 * i * i + 0.1957 * q * q;
    if y > 0.0 {
        -delta
    } else {
        delta
    }
}

fn antialiased(image: &Image, other: &Image, index: usize) -> bool {
    let x = index as u32 % image.width;
    let y = index as u32 / image.width;
    let center = image.pixels[index];
    let mut equal = 0usize;
    let mut darker = false;
    let mut lighter = false;
    let mut other_matches = 0usize;
    for neighbor_y in y.saturating_sub(1)..=(y + 1).min(image.height - 1) {
        for neighbor_x in x.saturating_sub(1)..=(x + 1).min(image.width - 1) {
            if neighbor_x == x && neighbor_y == y {
                continue;
            }
            let neighbor = image.at(neighbor_x, neighbor_y);
            if neighbor == center {
                equal += 1;
            }
            let center_y = luma(center);
            let neighbor_luma = luma(neighbor);
            darker |= neighbor_luma < center_y - 0.5;
            lighter |= neighbor_luma > center_y + 0.5;
            other_matches += usize::from(neighbor == other.at(neighbor_x, neighbor_y));
        }
    }
    equal <= 2 && darker && lighter && other_matches >= 3
}

fn gaussian_blur_9(image: &Image) -> Vec<[f64; 3]> {
    let sigma = 9.0f64;
    let radius = 27i32;
    let mut kernel: Vec<f64> = (-radius..=radius)
        .map(|offset| (-(f64::from(offset * offset)) / (2.0 * sigma * sigma)).exp())
        .collect();
    let sum: f64 = kernel.iter().sum();
    for value in &mut kernel {
        *value /= sum;
    }
    let length = image.pixels.len();
    let mut horizontal = vec![[0.0; 3]; length];
    for y in 0..image.height {
        for x in 0..image.width {
            let mut result = [0.0; 3];
            for (kernel_index, weight) in kernel.iter().copied().enumerate() {
                let offset = kernel_index as i32 - radius;
                let sample_x = (x as i32 + offset).clamp(0, image.width as i32 - 1) as u32;
                let pixel = image.at(sample_x, y);
                result[0] += f64::from(pixel.r) * weight;
                result[1] += f64::from(pixel.g) * weight;
                result[2] += f64::from(pixel.b) * weight;
            }
            horizontal[image.index(x, y)] = result;
        }
    }
    let mut vertical = vec![[0.0; 3]; length];
    for y in 0..image.height {
        for x in 0..image.width {
            let mut result = [0.0; 3];
            for (kernel_index, weight) in kernel.iter().copied().enumerate() {
                let offset = kernel_index as i32 - radius;
                let sample_y = (y as i32 + offset).clamp(0, image.height as i32 - 1) as u32;
                let pixel = horizontal[image.index(x, sample_y)];
                result[0] += pixel[0] * weight;
                result[1] += pixel[1] * weight;
                result[2] += pixel[2] * weight;
            }
            vertical[image.index(x, y)] = result;
        }
    }
    vertical
}

fn structure_mismatch_map(
    reference: &[[f64; 3]],
    candidate: &[[f64; 3]],
    mask: &[bool],
) -> Vec<bool> {
    reference
        .iter()
        .zip(candidate)
        .zip(mask)
        .map(|((a, b), masked)| {
            !masked
                && (0.299 * (a[0] - b[0]).abs()
                    + 0.587 * (a[1] - b[1]).abs()
                    + 0.114 * (a[2] - b[2]).abs())
                    > STRUCTURE_DIFF_THRESHOLD
        })
        .collect()
}

fn percent_in_rect<F>(mismatches: &[bool], mask: &[bool], rect: Rect, width: u32, include: F) -> f64
where
    F: Fn((u32, u32)) -> bool,
{
    let mut total = 0usize;
    let mut different = 0usize;
    for y in rect.top..rect.bottom {
        for x in rect.left..rect.right {
            let index = (y * width + x) as usize;
            if include((x, y)) && !mask[index] {
                total += 1;
                different += usize::from(mismatches[index]);
            }
        }
    }
    if total == 0 {
        0.0
    } else {
        100.0 * different as f64 / total as f64
    }
}

fn tile_max_percent<F>(
    mismatches: &[bool],
    mask: &[bool],
    rect: Rect,
    width: u32,
    include: F,
) -> f64
where
    F: Fn((u32, u32)) -> bool,
{
    let mut maximum = 0.0f64;
    for tile_top in (rect.top..rect.bottom).step_by(16) {
        for tile_left in (rect.left..rect.right).step_by(16) {
            let tile = Rect {
                left: tile_left,
                top: tile_top,
                right: (tile_left + 16).min(rect.right),
                bottom: (tile_top + 16).min(rect.bottom),
            };
            maximum = maximum.max(percent_in_rect(mismatches, mask, tile, width, |point| {
                include(point)
            }));
        }
    }
    maximum
}

fn validate_reference_fill(image: &Image, rect: Rect, owned: &OwnedRoi) -> Result<(), String> {
    let anchor = image.at(rect.left, rect.top);
    let anchor_lab = rgb_to_lab(anchor);
    let mut worst = 0.0f64;
    for y in rect.top..rect.bottom {
        for x in rect.left..rect.right {
            worst = worst.max(delta_e00(anchor_lab, rgb_to_lab(image.at(x, y))));
        }
    }
    if worst >= 1.0 {
        return Err(format!(
            "reviewed flat-fill sample in {} is not flat (delta-E00 {:.4})",
            owned.name, worst
        ));
    }
    Ok(())
}

fn median(values: &[f64]) -> f64 {
    match values.len() {
        0 => f64::INFINITY,
        length if length % 2 == 1 => values[length / 2],
        length => (values[length / 2 - 1] + values[length / 2]) / 2.0,
    }
}

fn percentile_nearest_rank(values: &[f64], percentile: f64) -> f64 {
    if values.is_empty() {
        return f64::INFINITY;
    }
    let rank = (percentile * values.len() as f64).ceil() as usize;
    values[rank.saturating_sub(1).min(values.len() - 1)]
}

#[derive(Clone, Copy)]
struct Lab {
    l: f64,
    a: f64,
    b: f64,
}

fn luma(pixel: Pixel) -> f64 {
    0.299 * f64::from(pixel.r) + 0.587 * f64::from(pixel.g) + 0.114 * f64::from(pixel.b)
}

fn rgb_to_lab(pixel: Pixel) -> Lab {
    fn linear(value: u8) -> f64 {
        let value = f64::from(value) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }
    let r = linear(pixel.r);
    let g = linear(pixel.g);
    let b = linear(pixel.b);
    let x = (0.412_456_4 * r + 0.357_576_1 * g + 0.180_437_5 * b) / 0.95047;
    let y = (0.212_672_9 * r + 0.715_152_2 * g + 0.072_175_0 * b) / 1.0;
    let z = (0.019_333_9 * r + 0.119_192_0 * g + 0.950_304_1 * b) / 1.08883;
    fn pivot(value: f64) -> f64 {
        let delta = 6.0 / 29.0;
        if value > delta * delta * delta {
            value.cbrt()
        } else {
            value / (3.0 * delta * delta) + 4.0 / 29.0
        }
    }
    let fx = pivot(x);
    let fy = pivot(y);
    let fz = pivot(z);
    Lab {
        l: 116.0 * fy - 16.0,
        a: 500.0 * (fx - fy),
        b: 200.0 * (fy - fz),
    }
}

// Sharma, Wu and Dalal's CIEDE2000 formula, angles expressed in degrees.
fn delta_e00(first: Lab, second: Lab) -> f64 {
    let c1 = first.a.hypot(first.b);
    let c2 = second.a.hypot(second.b);
    let c_bar = (c1 + c2) / 2.0;
    let c7 = c_bar.powi(7);
    let g = 0.5 * (1.0 - (c7 / (c7 + 25f64.powi(7))).sqrt());
    let a1p = (1.0 + g) * first.a;
    let a2p = (1.0 + g) * second.a;
    let c1p = a1p.hypot(first.b);
    let c2p = a2p.hypot(second.b);
    let h1p = hue_degrees(first.b, a1p);
    let h2p = hue_degrees(second.b, a2p);
    let delta_l = second.l - first.l;
    let delta_c = c2p - c1p;
    let mut delta_h_angle = h2p - h1p;
    if c1p * c2p == 0.0 {
        delta_h_angle = 0.0;
    } else if delta_h_angle > 180.0 {
        delta_h_angle -= 360.0;
    } else if delta_h_angle < -180.0 {
        delta_h_angle += 360.0;
    }
    let delta_h = 2.0 * (c1p * c2p).sqrt() * (delta_h_angle.to_radians() / 2.0).sin();
    let l_bar = (first.l + second.l) / 2.0;
    let c_bar_p = (c1p + c2p) / 2.0;
    let h_bar = if c1p * c2p == 0.0 {
        h1p + h2p
    } else if (h1p - h2p).abs() <= 180.0 {
        (h1p + h2p) / 2.0
    } else if h1p + h2p < 360.0 {
        (h1p + h2p + 360.0) / 2.0
    } else {
        (h1p + h2p - 360.0) / 2.0
    };
    let t = 1.0 - 0.17 * (h_bar - 30.0).to_radians().cos()
        + 0.24 * (2.0 * h_bar).to_radians().cos()
        + 0.32 * (3.0 * h_bar + 6.0).to_radians().cos()
        - 0.20 * (4.0 * h_bar - 63.0).to_radians().cos();
    let s_l = 1.0 + 0.015 * (l_bar - 50.0).powi(2) / (20.0 + (l_bar - 50.0).powi(2)).sqrt();
    let s_c = 1.0 + 0.045 * c_bar_p;
    let s_h = 1.0 + 0.015 * c_bar_p * t;
    let delta_theta = 30.0 * (-((h_bar - 275.0) / 25.0).powi(2)).exp();
    let r_c = 2.0 * (c_bar_p.powi(7) / (c_bar_p.powi(7) + 25f64.powi(7))).sqrt();
    let r_t = -r_c * (2.0 * delta_theta).to_radians().sin();
    let l_term = delta_l / s_l;
    let c_term = delta_c / s_c;
    let h_term = delta_h / s_h;
    (l_term * l_term + c_term * c_term + h_term * h_term + r_t * c_term * h_term).sqrt()
}

fn hue_degrees(b: f64, a: f64) -> f64 {
    let degrees = b.atan2(a).to_degrees();
    if degrees < 0.0 {
        degrees + 360.0
    } else {
        degrees
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ciede2000_matches_published_reference_pair() {
        let first = Lab {
            l: 50.0,
            a: 2.6772,
            b: -79.7751,
        };
        let second = Lab {
            l: 50.0,
            a: 0.0,
            b: -82.7485,
        };
        assert!((delta_e00(first, second) - 2.0425).abs() < 0.0001);
    }
}
