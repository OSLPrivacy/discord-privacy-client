use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use task_5102_carrier_reference::messenger::{
    verify_composer_references, ValidationMode, CAPTURES_PER_KEY, COMPOSER_STATE, MESSENGER_ORIGIN,
    PROBE_TEXT,
};

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn fixture_png(seed: u8) -> (Vec<u8>, usize, usize, String) {
    let width = 20usize;
    let height = 20usize;
    let mut pixels = Vec::with_capacity(width * height * 3);
    let mut all = BTreeSet::new();
    let mut roi = BTreeSet::new();
    let mut seam_bytes = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let in_seam = x < 4 || x >= width - 4 || y < 4 || y >= height - 4;
            let colour = if in_seam {
                // Independent of seed: all five seam rings are byte-identical.
                [x as u8 * 7, y as u8 * 11, (x + y) as u8 * 5]
            } else {
                [x as u8 * 9, y as u8 * 13, seed.wrapping_add((x * y) as u8)]
            };
            pixels.extend_from_slice(&colour);
            all.insert(colour);
            if in_seam {
                seam_bytes.extend_from_slice(&colour);
            } else {
                roi.insert(colour);
            }
        }
    }
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&pixels).unwrap();
    }
    (png, all.len(), roi.len(), sha(&seam_bytes))
}

struct FixtureTree {
    _temporary: tempfile::TempDir,
    census: PathBuf,
    contract: PathBuf,
    manifests: PathBuf,
}

impl FixtureTree {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let census = temporary.path().join("census.json");
        let contract = temporary.path().join("contract.json");
        let manifests = temporary.path().join("manifests");
        fs::create_dir(&manifests).unwrap();
        fs::write(
            &contract,
            include_bytes!("../messenger-composer-contract.json"),
        )
        .unwrap();
        let keys: Vec<_> = ["direct-message", "group", "community"]
            .into_iter()
            .map(|channel| {
                json!({
                    "origin": MESSENGER_ORIGIN,
                    "channel": channel,
                    "composerState": COMPOSER_STATE
                })
            })
            .collect();
        fs::write(
            &census,
            serde_json::to_vec_pretty(&json!({
                "schema": "osl-messenger-live-census-v1",
                "observedAtUtc": "2026-08-11T12:00:00Z",
                "source": "windows-powershell-uia-interactive-session",
                "independence": "created-before-and-without-reading-composer-contract-or-manifests",
                "windowsVersion": "10.0.19045",
                "windowsBuild": "19045",
                "tasklistProcessCount": 8,
                "visibleBrowserWindowCount": 1,
                "installedChannels": ["direct-message", "group", "community"],
                "observedComposerCount": 3,
                "observedComposers": keys
            }))
            .unwrap(),
        )
        .unwrap();
        for (channel_index, channel) in ["direct-message", "group", "community"]
            .into_iter()
            .enumerate()
        {
            let mut captures = Vec::new();
            for ordinal in 1..=CAPTURES_PER_KEY {
                let (png, colours, roi_colours, seam_hash) =
                    fixture_png((channel_index * 20 + ordinal) as u8);
                let png_name = format!("{channel}-{ordinal}.png");
                fs::write(manifests.join(&png_name), &png).unwrap();
                captures.push(json!({
                    "ordinal": ordinal,
                    "pngPath": png_name,
                    "pngSha256": sha(&png),
                    "roi": {"left": 104, "top": 104, "right": 116, "bottom": 116},
                    "capturedBounds": {"left": 100, "top": 100, "right": 120, "bottom": 120},
                    "seamRingPx": 4,
                    "seamRingSha256": seam_hash,
                    "distinctRgbColours": colours,
                    "roiDistinctRgbColours": roi_colours
                }));
            }
            let marker = format!("TASK5130-LIVE-{channel_index}");
            let manifest = json!({
                "schema": "osl-messenger-composer-reference-v1",
                "key": {"origin": MESSENGER_ORIGIN, "channel": channel, "composerState": COMPOSER_STATE},
                "benignProbeText": PROBE_TEXT,
                "sourceKind": "fixture",
                "captureMethod": "fixture-png",
                "uiaProvider": "Windows UI Automation",
                "observationTools": [],
                "routeKind": "live-carrier",
                "catalogueOnly": false,
                "hiddenOverlay": false,
                "testRoute": false,
                "fixtureSpecificDistinctColourFloor": 32,
                "browser": {
                    "browserName": "fixture browser",
                    "executableSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "signer": "fixture signer",
                    "signatureStatus": "valid",
                    "profileId": format!("profile-{channel_index}"),
                    "profilePath": format!("C:/fixture/profile-{channel_index}"),
                    "independentlySignedIn": true,
                    "carrierAccountId": format!("account-{channel_index}"),
                    "hwnd": 1000 + channel_index,
                    "hwndGeneration": 1,
                    "foreground": true,
                    "visible": true,
                    "occluded": false,
                    "origin": MESSENGER_ORIGIN
                },
                "shippingIntegration": {
                    "buildKind": "shipping-windows-release",
                    "executableName": "osl-privacy-hub.exe",
                    "executableSha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    "signer": "fixture release signer",
                    "signatureStatus": "valid",
                    "integrationEnabled": true,
                    "uniqueMarker": marker,
                    "carrierVisibleBefore": "composer empty",
                    "carrierVisibleAfter": format!("composer contains {marker}")
                },
                "review": {
                    "captureAuthor": "capture-author",
                    "reviewer": "independent-reviewer",
                    "reviewerKeyId": "task-5130-review-root",
                    "reviewedAtUtc": "2026-08-11T12:30:00Z",
                    "signatureHex": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
                },
                "captures": captures
            });
            fs::write(
                manifests.join(format!("{channel}.manifest.json")),
                serde_json::to_vec_pretty(&manifest).unwrap(),
            )
            .unwrap();
        }
        Self {
            _temporary: temporary,
            census,
            contract,
            manifests,
        }
    }

    fn manifest(&self, channel: &str) -> PathBuf {
        self.manifests.join(format!("{channel}.manifest.json"))
    }

    fn mutate_manifest(&self, channel: &str, mutation: impl FnOnce(&mut Value)) {
        let path = self.manifest(channel);
        let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        mutation(&mut value);
        fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    }
}

fn verify(tree: &FixtureTree, mode: ValidationMode) -> Result<(), String> {
    verify_composer_references(&tree.census, &tree.contract, &tree.manifests, mode)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[test]
fn complete_five_capture_fixture_passes_but_release_mode_rejects_it() {
    let tree = FixtureTree::new();
    let verified = verify_composer_references(
        &tree.census,
        &tree.contract,
        &tree.manifests,
        ValidationMode::Fixture,
    )
    .unwrap();
    assert_eq!(verified.keys.len(), 3);
    assert_eq!(verified.captures, 15);
    assert!(verified.minimum_capture_colours >= 32);
    assert!(verified.minimum_roi_colours > 2);
    let release_error = verify(&tree, ValidationMode::Release).unwrap_err();
    assert!(release_error.contains("live Windows PowerShell/UIA/CopyFromScreen source"));
}

#[test]
fn starving_one_state_and_coordinated_inventory_shrink_fail_before_diffing() {
    let tree = FixtureTree::new();
    fs::remove_file(tree.manifest("community")).unwrap();
    for ordinal in 1..=5 {
        fs::remove_file(tree.manifests.join(format!("community-{ordinal}.png"))).unwrap();
    }
    let error = verify(&tree, ValidationMode::Fixture).unwrap_err();
    assert!(error.contains("reference manifests is missing"));
    assert!(error.contains("community"));

    let tree = FixtureTree::new();
    let mut contract: Value = serde_json::from_slice(&fs::read(&tree.contract).unwrap()).unwrap();
    contract["keys"]
        .as_array_mut()
        .unwrap()
        .retain(|key| key["channel"] != "community");
    fs::write(
        &tree.contract,
        serde_json::to_vec_pretty(&contract).unwrap(),
    )
    .unwrap();
    fs::remove_file(tree.manifest("community")).unwrap();
    let error = verify(&tree, ValidationMode::Fixture).unwrap_err();
    assert!(error.contains("live census has uncontracted state"));
    assert!(error.contains("community"));
}

#[test]
fn blank_roi_hidden_route_and_disabled_shipping_integration_are_named_blockers() {
    let tree = FixtureTree::new();
    tree.mutate_manifest("group", |manifest| {
        manifest["catalogueOnly"] = json!(true);
    });
    assert!(verify(&tree, ValidationMode::Fixture)
        .unwrap_err()
        .contains("visible live carrier route"));

    let tree = FixtureTree::new();
    tree.mutate_manifest("direct-message", |manifest| {
        manifest["shippingIntegration"]["integrationEnabled"] = json!(false);
    });
    assert!(verify(&tree, ValidationMode::Fixture)
        .unwrap_err()
        .contains("shipping integration marker"));

    let tree = FixtureTree::new();
    let png_path = tree.manifests.join("community-1.png");
    let mut png = Vec::new();
    {
        let pixels = vec![17u8; 20 * 20 * 3];
        let mut encoder = png::Encoder::new(&mut png, 20, 20);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&pixels).unwrap();
    }
    fs::write(&png_path, &png).unwrap();
    tree.mutate_manifest("community", |manifest| {
        manifest["captures"][0]["pngSha256"] = json!(sha(&png));
        manifest["captures"][0]["seamRingSha256"] =
            json!(sha(&vec![17u8; (20 * 20 - 12 * 12) * 3]));
        manifest["captures"][0]["distinctRgbColours"] = json!(1);
        manifest["captures"][0]["roiDistinctRgbColours"] = json!(1);
    });
    let error = verify(&tree, ValidationMode::Fixture).unwrap_err();
    assert!(error.contains("distinct RGB colours 1 below floor 32"));
}
