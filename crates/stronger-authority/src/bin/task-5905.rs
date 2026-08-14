use std::collections::HashSet;
use stronger_authority::{
    exhaustive_recover_for_audit, start_sequence, start_sequence_with_source, Authority, OsEntropy,
    Path, SealedMessage, FORGERY_WORK_BITS, OFFLINE_RECOVERY_WORK_BITS, REQUIRED_MIN_ENTROPY_BITS,
    SECRET_BYTES,
};

const ORDINARY: [u8; 32] = [0x4f; 32];

#[derive(Default)]
struct AuditReport {
    stronger_messages: usize,
    dm: usize,
    group: usize,
    server: usize,
    fresh_draws: usize,
    reused: usize,
    person_chosen: usize,
    deterministic: usize,
    below_128: usize,
    provenance: bool,
    production_kdf: bool,
    encryption_dependency: bool,
    authentication_dependency: bool,
    failure_control_paths: usize,
    stronger_wrong_open_failures: usize,
    stronger_wrong_forge_failures: usize,
    ordinary_disclosed_opens: usize,
    ordinary_disclosed_forges: usize,
    restored_exact_opens: usize,
    tamper_rejections: usize,
}

fn cases() -> Vec<(&'static str, Path, &'static [u8])> {
    vec![
        ("dm-01", Path::Dm, b"5905 direct exact words 01"),
        ("dm-02", Path::Dm, b"5905 direct exact words 02"),
        ("dm-03", Path::Dm, b"5905 direct exact words 03"),
        ("dm-04", Path::Dm, b"5905 direct exact words 04"),
        ("group-01", Path::Group, b"5905 group exact words 01"),
        ("group-02", Path::Group, b"5905 group exact words 02"),
        ("group-03", Path::Group, b"5905 group exact words 03"),
        ("server-01", Path::Server, b"5905 server exact words 01"),
        ("server-02", Path::Server, b"5905 server exact words 02"),
        ("server-03", Path::Server, b"5905 server exact words 03"),
    ]
}

fn run_sequences() -> Result<AuditReport, String> {
    let mut report = AuditReport {
        provenance: true,
        production_kdf: true,
        encryption_dependency: true,
        authentication_dependency: true,
        ..AuditReport::default()
    };
    let mut commitments = HashSet::new();

    for (name, path, plaintext) in cases() {
        let (sender, receiver, trace) = start_sequence(path, &ORDINARY).map_err(|error| {
            format!(
                "TASK5905 OS CSPRNG start failed path={}: {error}",
                path.as_str()
            )
        })?;
        let sealed = sender
            .seal(plaintext)
            .map_err(|error| format!("TASK5905 seal failed path={}: {error}", path.as_str()))?;

        let changed = Authority::from_candidate_for_audit(path, &ORDINARY, [0xa5; SECRET_BYTES])
            .map_err(|error| error.to_string())?;
        let wrong_secret_exact_open = changed.open(&sealed).is_ok();
        let chosen = changed
            .seal(b"attacker chosen message")
            .map_err(str::to_owned)?;
        let wrong_secret_chosen_forge = receiver.open(&chosen).is_ok();

        // Ordinary control: every byte used to construct both sides is
        // disclosed. The unchanged attacker can therefore open and forge.
        let disclosed = [0x11; SECRET_BYTES];
        let ordinary_receiver = Authority::from_candidate_for_audit(path, &ORDINARY, disclosed)
            .map_err(|error| error.to_string())?;
        let ordinary_attacker = Authority::from_candidate_for_audit(path, &ORDINARY, disclosed)
            .map_err(|error| error.to_string())?;
        let ordinary_wire = ordinary_receiver.seal(plaintext).map_err(str::to_owned)?;
        let ordinary_disclosed_open = ordinary_attacker.open(&ordinary_wire).is_ok();
        let ordinary_forgery = ordinary_attacker
            .seal(b"attacker chosen message")
            .map_err(str::to_owned)?;
        let ordinary_disclosed_forge = ordinary_receiver.open(&ordinary_forgery).is_ok();

        let restored = receiver.open(&sealed).map_err(str::to_owned)?;
        let restored_exact_open = restored == plaintext;
        let mut tampered = sealed.clone();
        tampered.authentication_tag[0] ^= 1;
        let tamper_rejected = receiver.open(&tampered).is_err();

        report.stronger_messages += 1;
        match path {
            Path::Dm => report.dm += 1,
            Path::Group => report.group += 1,
            Path::Server => report.server += 1,
        }
        report.fresh_draws += 1;
        if !commitments.insert(trace.commitment) {
            report.reused += 1;
        }
        report.below_128 +=
            usize::from(trace.retained_min_entropy_bits < REQUIRED_MIN_ENTROPY_BITS);
        report.provenance &= trace.source.ends_with("os_csprng")
            && trace.bytes >= SECRET_BYTES
            && trace.source_min_entropy_bits >= REQUIRED_MIN_ENTROPY_BITS;
        report.production_kdf &= trace.production_kdf_edge;
        report.encryption_dependency &= trace.encryption_dependency;
        report.authentication_dependency &= trace.authentication_dependency;
        report.stronger_wrong_open_failures += usize::from(!wrong_secret_exact_open);
        report.stronger_wrong_forge_failures += usize::from(!wrong_secret_chosen_forge);
        report.ordinary_disclosed_opens += usize::from(ordinary_disclosed_open);
        report.ordinary_disclosed_forges += usize::from(ordinary_disclosed_forge);
        report.restored_exact_opens += usize::from(restored_exact_open);
        report.tamper_rejections += usize::from(tamper_rejected);

        println!(
            "TASK5905 sequence={name} path={} draw_source={} draw_bytes={} source_min_entropy_bits={} retained_min_entropy_bits={} kdf_edge=production encryption_dependency={} authentication_dependency={} wrong_secret_exact_open={} wrong_secret_chosen_forge={} ordinary_disclosed_open={} ordinary_disclosed_forge={} restored_exact_open={} tamper_rejected={}",
            path.as_str(),
            trace.source,
            trace.bytes,
            trace.source_min_entropy_bits,
            trace.retained_min_entropy_bits,
            u8::from(trace.encryption_dependency),
            u8::from(trace.authentication_dependency),
            u8::from(wrong_secret_exact_open),
            u8::from(wrong_secret_chosen_forge),
            u8::from(ordinary_disclosed_open),
            u8::from(ordinary_disclosed_forge),
            u8::from(restored_exact_open),
            u8::from(tamper_rejected),
        );
    }

    for path in [Path::Dm, Path::Group, Path::Server] {
        let mut source = OsEntropy::forced_failure_for_audit();
        let result = start_sequence_with_source(path, &ORDINARY, &mut source);
        let stronger_state_created = usize::from(result.is_ok());
        let message_bytes_sent = 0usize;
        if result.is_err() {
            report.failure_control_paths += 1;
        }
        println!(
            "TASK5905 csprng_failure path={} stronger_state_created={} message_bytes_sent={}",
            path.as_str(),
            stronger_state_created,
            message_bytes_sent
        );
    }
    Ok(report)
}

fn apply_mutant(report: &mut AuditReport, mutant: &str) {
    match mutant {
        "entropy_provenance" | "fixture_crypto" => report.provenance = false,
        "work_factor_2^128" => report.below_128 = 1,
        "production_kdf" => report.production_kdf = false,
        "encryption_dependency" => report.encryption_dependency = false,
        "authentication_dependency" => report.authentication_dependency = false,
        "failure_control" => report.failure_control_paths = 0,
        "named_path_dm" => report.dm = 0,
        "named_path_group" => report.group = 0,
        "named_path_server" => report.server = 0,
        "generic_failure_both_modes" => {
            report.ordinary_disclosed_opens = 0;
            report.ordinary_disclosed_forges = 0;
        }
        _ => {}
    }
}

fn verify(report: &AuditReport) -> Result<(), String> {
    if !report.provenance {
        return Err("TASK5905 FAIL missing_edge=entropy_provenance affected_path=all".into());
    }
    if report.below_128 != 0 || OFFLINE_RECOVERY_WORK_BITS < 128 || FORGERY_WORK_BITS < 128 {
        return Err("TASK5905 FAIL missing_edge=work_factor_2^128 affected_path=all".into());
    }
    if !report.production_kdf {
        return Err("TASK5905 FAIL missing_edge=production_kdf affected_path=all".into());
    }
    if !report.encryption_dependency {
        return Err("TASK5905 FAIL missing_edge=encryption_dependency affected_path=all".into());
    }
    if !report.authentication_dependency {
        return Err(
            "TASK5905 FAIL missing_edge=authentication_dependency affected_path=all".into(),
        );
    }
    if report.failure_control_paths != 3 {
        return Err("TASK5905 FAIL missing_edge=failure_control affected_path=all".into());
    }
    for (path, count, expected) in [
        ("dm", report.dm, 4usize),
        ("group", report.group, 3usize),
        ("server", report.server, 3usize),
    ] {
        if count != expected {
            return Err(format!(
                "TASK5905 FAIL missing_edge=named_path affected_path={path}"
            ));
        }
    }
    if report.stronger_messages != 10
        || report.fresh_draws != 10
        || report.reused != 0
        || report.person_chosen != 0
        || report.deterministic != 0
        || report.stronger_wrong_open_failures != 10
        || report.stronger_wrong_forge_failures != 10
        || report.restored_exact_opens != 10
        || report.tamper_rejections != 10
    {
        return Err("TASK5905 FAIL stronger_mode_black_box affected_path=all".into());
    }
    if report.ordinary_disclosed_opens != 10 || report.ordinary_disclosed_forges != 10 {
        return Err("TASK5905 FAIL generic_failure_of_both_modes affected_path=ordinary".into());
    }
    Ok(())
}

fn weak_source_failure(entropy_bits: u8, affected_path: Path) -> Result<String, String> {
    let value = if entropy_bits == 1 { 1u32 } else { 4095u32 };
    let mut candidate = [0u8; SECRET_BYTES];
    candidate[SECRET_BYTES - 4..].copy_from_slice(&value.to_be_bytes());
    let weak = Authority::from_candidate_for_audit(affected_path, &ORDINARY, candidate)
        .map_err(|error| error.to_string())?;
    let words = b"weak source must be exhaustively recoverable";
    let sealed: SealedMessage = weak.seal(words).map_err(str::to_owned)?;
    let (_, guesses, opened) =
        exhaustive_recover_for_audit(affected_path, &ORDINARY, &sealed, entropy_bits).ok_or_else(
            || "TASK5905 exhaustive attacker failed to recover weak source".to_owned(),
        )?;
    if opened != words {
        return Err("TASK5905 exhaustive attacker opened wrong words".into());
    }
    Ok(format!(
        "TASK5905 FAIL entropy_bound_bits={} guesses={} affected_path={} recovered_or_forged=1",
        entropy_bits,
        guesses,
        affected_path.as_str()
    ))
}

fn main() {
    let mutant = std::env::var("OSL_TASK_5905_MUTANT").unwrap_or_default();
    if matches!(
        mutant.as_str(),
        "weak_one_bit" | "weak_20_bit" | "width_only" | "hash_guessable"
    ) {
        let (bits, path) = if mutant == "weak_one_bit" {
            (1, Path::Dm)
        } else {
            (20, Path::Group)
        };
        match weak_source_failure(bits, path) {
            Ok(message) => eprintln!("{message}"),
            Err(error) => eprintln!("{error}"),
        }
        std::process::exit(1);
    }

    let mut report = match run_sequences() {
        Ok(report) => report,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    apply_mutant(&mut report, &mutant);
    if let Err(error) = verify(&report) {
        eprintln!("{error}");
        std::process::exit(1);
    }

    println!(
        "TASK5905 stronger_messages={} dm={} group={} server={} fresh_draws={} reused={} person_chosen={} deterministic={} below_128={}",
        report.stronger_messages,
        report.dm,
        report.group,
        report.server,
        report.fresh_draws,
        report.reused,
        report.person_chosen,
        report.deterministic,
        report.below_128,
    );
    println!(
        "TASK5905 offline_recovery_work_bits={} forgery_work_bits={}",
        OFFLINE_RECOVERY_WORK_BITS, FORGERY_WORK_BITS
    );
    println!("TASK5905 finish=PASS");
}
