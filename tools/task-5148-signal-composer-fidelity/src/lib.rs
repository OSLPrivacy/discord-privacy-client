use std::collections::BTreeSet;

pub const REQUIRED_STATE: &str = "signal/stable/direct-message/focused-exact-probe";
pub const REQUIRED_MUTANTS: [&str; 3] = ["resolver-fixture", "fixed-font", "boundary-shift"];

#[derive(Clone, Debug)]
pub struct FidelityReceipt<'a> {
    pub state: &'a str,
    pub reference_manifest: &'a str,
    pub candidate_manifest: &'a str,
    pub resolver: &'a str,
    pub painter: &'a str,
    pub live_candidate: bool,
    pub reference_dimensions: (u32, u32),
    pub candidate_dimensions: (u32, u32),
    pub reference_colours: u32,
    pub candidate_colours: u32,
    pub reference_alpha_pixels: u32,
    pub candidate_alpha_pixels: u32,
    pub seam_ring_px: u8,
    pub boundary_displacement_px: u8,
    pub baseline_displacement_px: u8,
    pub median_delta_e00: f32,
    pub p99_delta_e00: f32,
    pub structure_mismatch_percent: f32,
    pub max_tile_mismatch_percent: f32,
    pub raw_mismatch_percent: f32,
    pub exact_probe_mask_pixels: u32,
    pub font_from_live_sample: bool,
}

#[derive(Clone, Debug)]
pub struct ShippingReceipt<'a> {
    pub carrier: &'a str,
    pub qa_entitlement: bool,
    pub display_affinity: &'a str,
    pub captured_osl_pixels: u32,
    pub uia_identity_retained: bool,
    pub uia_bounds_retained: bool,
}

pub fn validate_fidelity(receipt: &FidelityReceipt<'_>) -> Result<(), String> {
    if receipt.state != REQUIRED_STATE {
        return Err(format!("missing composer state: {REQUIRED_STATE}"));
    }
    if !receipt.live_candidate {
        return Err("live-bound candidate missing".into());
    }
    if receipt.resolver != "signal_surface_finder::resolve_signal_composer" {
        return Err(format!("resolver fixture forbidden: {}", receipt.resolver));
    }
    if receipt.painter != "signal_production_composer_painter" {
        return Err(format!(
            "production painter identity missing: {}",
            receipt.painter
        ));
    }
    if receipt.reference_manifest.is_empty()
        || receipt.reference_manifest != receipt.candidate_manifest
    {
        return Err("ordinary reference and candidate are not same-manifest".into());
    }
    if receipt.reference_dimensions != receipt.candidate_dimensions {
        return Err("unequal physical dimensions".into());
    }
    if receipt.reference_colours <= 2 || receipt.candidate_colours <= 2 {
        return Err("degenerate distinct-colour count before scoring".into());
    }
    if receipt.reference_alpha_pixels == 0 || receipt.candidate_alpha_pixels == 0 {
        return Err("all-transparent input before scoring".into());
    }
    if receipt.seam_ring_px != 4 {
        return Err("seam ring must be 4 physical pixels".into());
    }
    if receipt.boundary_displacement_px != 0 {
        return Err("boundary displacement must be 0 physical pixels".into());
    }
    if receipt.baseline_displacement_px != 0 {
        return Err("baseline displacement must be 0 physical pixels".into());
    }
    if receipt.median_delta_e00 >= 1.0 {
        return Err("median flat-fill delta-E00 must be below 1.0".into());
    }
    if receipt.p99_delta_e00 >= 2.3 {
        return Err("p99 flat-fill delta-E00 must be below 2.3".into());
    }
    if receipt.structure_mismatch_percent > 0.5 {
        return Err("owned-ROI structure mismatch must be at or below 0.5%".into());
    }
    if receipt.max_tile_mismatch_percent > 5.0 {
        return Err("16x16 tile mismatch must be at or below 5%".into());
    }
    if receipt.raw_mismatch_percent > 0.5 {
        return Err("exact-probe raw mismatch must be at or below 0.5%".into());
    }
    if receipt.exact_probe_mask_pixels != 0 {
        return Err("exact benign probe may not mask pixels".into());
    }
    if !receipt.font_from_live_sample {
        return Err("fixed font forbidden; font must bind to live sample".into());
    }
    Ok(())
}

pub fn validate_shipping_exclusion(receipt: &ShippingReceipt<'_>) -> Result<(), String> {
    if receipt.carrier != "signal" {
        return Err("shipping exclusion receipt is not Signal".into());
    }
    if receipt.qa_entitlement {
        return Err("shipping exclusion must run without QA entitlement".into());
    }
    if receipt.display_affinity != "WDA_EXCLUDEFROMCAPTURE" {
        return Err("shipping overlay lacks WDA_EXCLUDEFROMCAPTURE".into());
    }
    if receipt.captured_osl_pixels != 0 {
        return Err(format!(
            "shipping capture exposed {} OSL pixels",
            receipt.captured_osl_pixels
        ));
    }
    if !receipt.uia_identity_retained || !receipt.uia_bounds_retained {
        return Err("shipping capture lost expected UIA target identity/bounds".into());
    }
    Ok(())
}

pub fn validate_mutant_inventory(inventory: &[&str]) -> Result<(), String> {
    let actual: BTreeSet<_> = inventory.iter().copied().collect();
    for required in REQUIRED_MUTANTS {
        if !actual.contains(required) {
            return Err(format!("missing red mutant: {required}"));
        }
    }
    Ok(())
}

pub fn known_good_contract_receipt() -> FidelityReceipt<'static> {
    FidelityReceipt {
        state: REQUIRED_STATE,
        reference_manifest: "same-attested-manifest",
        candidate_manifest: "same-attested-manifest",
        resolver: "signal_surface_finder::resolve_signal_composer",
        painter: "signal_production_composer_painter",
        live_candidate: true,
        reference_dimensions: (736, 58),
        candidate_dimensions: (736, 58),
        reference_colours: 512,
        candidate_colours: 512,
        reference_alpha_pixels: 42_688,
        candidate_alpha_pixels: 42_688,
        seam_ring_px: 4,
        boundary_displacement_px: 0,
        baseline_displacement_px: 0,
        median_delta_e00: 0.2,
        p99_delta_e00: 0.8,
        structure_mismatch_percent: 0.1,
        max_tile_mismatch_percent: 1.0,
        raw_mismatch_percent: 0.1,
        exact_probe_mask_pixels: 0,
        font_from_live_sample: true,
    }
}

pub fn known_good_shipping_receipt() -> ShippingReceipt<'static> {
    ShippingReceipt {
        carrier: "signal",
        qa_entitlement: false,
        display_affinity: "WDA_EXCLUDEFROMCAPTURE",
        captured_osl_pixels: 0,
        uia_identity_retained: true,
        uia_bounds_retained: true,
    }
}
