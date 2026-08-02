//! D50 acceptance harness for local carrier generation.
//!
//! This runs the real `CoverDraftEngine` boundary with a verified deterministic
//! adapter so the measurement is available on every CI machine. A release
//! candidate must run the same harness with the selected model pack before D3
//! may choose it; a missing model is not evidence that a five-second budget is
//! acceptable.

use cover_draft::{
    AuthorizedContextEntry, CancellationToken, CoverDraftEngine, DraftRequest, DraftScope,
    GenerationControl, Limits, LocalCoverModel, ModelError, ModelInput, ModelPackMetadata,
    ModelPackSignatureVerifier, ObservedArtifact, TrustedModelPack,
};
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

// These are deliberately tied to the application's measured interaction
// baseline: 44–85 ms navigation and 25 ms input echo. They are acceptance
// criteria, not a permissive watchdog. Changing this to five seconds must make
// `d50_budget_cannot_be_relaxed_to_five_seconds` fail.
const MAX_WALL_PER_CARRIER: Duration = Duration::from_millis(85);
const MAX_CPU_PER_CARRIER: Duration = Duration::from_millis(85);
const MAX_PEAK_RSS_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const SAMPLE_COUNT: u32 = 16;

struct AcceptingVerifier;

impl ModelPackSignatureVerifier for AcceptingVerifier {
    fn verify(&self, _metadata_digest: &[u8; 32], _signature: &[u8]) -> bool {
        true
    }
}

fn trusted_pack() -> TrustedModelPack {
    TrustedModelPack::verify(
        ModelPackMetadata {
            model_id: "budget-harness-model".to_owned(),
            version_digest: [1; 32],
            artifact_digest: [2; 32],
            artifact_size: 1024,
            max_working_set_bytes: MAX_PEAK_RSS_BYTES,
            signature: vec![3; 64],
        },
        ObservedArtifact {
            digest: [2; 32],
            size: 1024,
        },
        &AcceptingVerifier,
    )
    .expect("the benchmark adapter has a verified pack")
}

struct MeasuredAdapter {
    pack: TrustedModelPack,
}

impl LocalCoverModel for MeasuredAdapter {
    fn trusted_model_pack(&self) -> Option<&TrustedModelPack> {
        Some(&self.pack)
    }

    fn generate(
        &mut self,
        input: ModelInput<'_>,
        control: GenerationControl<'_>,
    ) -> Result<Zeroizing<String>, ModelError> {
        // Keep the two measured phases distinct. A real adapter replaces these
        // bounded operations with prompt prefill and decode, retaining the
        // control check between them.
        let prefill_bytes = input.context().map(str::len).sum::<usize>();
        if control.should_stop() || prefill_bytes == 0 {
            return Err(ModelError::DeadlineExceeded);
        }
        let decoded = "A short carrier reads like an ordinary update.";
        if control.should_stop() || decoded.len() > input.max_output_bytes {
            return Err(ModelError::DeadlineExceeded);
        }
        Ok(Zeroizing::new(decoded.to_owned()))
    }
}

fn request() -> DraftRequest<'static> {
    DraftRequest {
        scope: DraftScope {
            account: b"budget-account",
            conversation: b"budget-conversation",
            recipient: b"budget-recipient",
        },
        canonical_message_hash: [9; 32],
        authorized_context: vec![AuthorizedContextEntry::authorize(
            "The platform-visible cover before this carrier.",
        )],
    }
}

fn cpu_seconds() -> Option<f64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let fields = stat.rsplit_once(") ")?.1.split_whitespace().collect::<Vec<_>>();
    // `utime` and `stime` are fields 14/15; after removing pid+comm, they are
    // indexes 11/12.
    let ticks = fields.get(11)?.parse::<f64>().ok()? + fields.get(12)?.parse::<f64>().ok()?;
    let hz = std::process::Command::new("getconf")
        .arg("CLK_TCK")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|value| value.trim().parse::<f64>().ok())?;
    Some(ticks / hz)
}

fn peak_rss_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()
        .map(|kilobytes| kilobytes * 1024)
}

#[test]
fn t13_tl1_reports_wall_cpu_and_peak_rss_with_a_strict_budget() {
    let engine = CoverDraftEngine::new(Limits {
        max_generation_time: MAX_WALL_PER_CARRIER,
        ..Limits::default()
    })
    .expect("D50 limits are valid");
    let started_cpu = cpu_seconds();
    let started = Instant::now();

    for _ in 0..SAMPLE_COUNT {
        let mut model = MeasuredAdapter { pack: trusted_pack() };
        engine
            .prepare(&mut model, request(), &CancellationToken::default())
            .expect("the bounded adapter produces a carrier draft");
    }

    let wall_per_carrier = started.elapsed() / SAMPLE_COUNT;
    let cpu_per_carrier = started_cpu
        .zip(cpu_seconds())
        .map(|(before, after)| Duration::from_secs_f64((after - before) / f64::from(SAMPLE_COUNT)));
    let peak_rss = peak_rss_bytes();
    eprintln!(
        "T13-L1 measurement: wall={wall_per_carrier:?}/carrier cpu={cpu_per_carrier:?}/carrier peak_rss={peak_rss:?} bytes"
    );

    assert!(wall_per_carrier <= MAX_WALL_PER_CARRIER, "wall-clock budget exceeded");
    if let Some(cpu_per_carrier) = cpu_per_carrier {
        assert!(cpu_per_carrier <= MAX_CPU_PER_CARRIER, "CPU budget exceeded");
    }
    if let Some(peak_rss) = peak_rss {
        assert!(peak_rss <= MAX_PEAK_RSS_BYTES, "peak RSS budget exceeded");
    }
}

#[test]
fn d50_budget_cannot_be_relaxed_to_five_seconds() {
    assert!(
        MAX_WALL_PER_CARRIER < Duration::from_secs(5),
        "five seconds would make the composer feel stalled, not responsive"
    );
}
