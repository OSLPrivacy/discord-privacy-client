use std::collections::{BTreeMap, BTreeSet};

const REPRO_WORKFLOW: &str = include_str!("../../../.github/workflows/reproducible-build.yml");
const SIMPLE_SPEC: &str = include_str!("../../../docs/design/osl-simple-spec.md");

#[derive(Debug, Default)]
struct Step {
    name: Option<String>,
    shell: Option<String>,
    env: BTreeMap<String, String>,
    run: String,
}

#[derive(Debug, Default)]
struct WorkflowJob {
    steps: Vec<Step>,
}

fn unquote(value: &str) -> String {
    let trimmed = value.trim();
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

fn parse_workflow_jobs(source: &str) -> BTreeMap<String, WorkflowJob> {
    let mut jobs = BTreeMap::<String, WorkflowJob>::new();
    let mut current_job: Option<String> = None;
    let mut current_step: Option<Step> = None;
    let mut in_steps = false;
    let mut in_env = false;
    let mut run_indent: Option<usize> = None;

    let finish_step = |jobs: &mut BTreeMap<String, WorkflowJob>,
                       job: &Option<String>,
                       step: &mut Option<Step>| {
        if let (Some(job), Some(step)) = (job.as_ref(), step.take()) {
            jobs.get_mut(job).expect("job exists").steps.push(step);
        }
    };

    for line in source.lines() {
        if let Some(indent) = run_indent {
            let line_indent = line.len() - line.trim_start_matches(' ').len();
            if line.trim().is_empty() || line_indent >= indent {
                let run_line = if line.len() >= indent {
                    &line[indent..]
                } else {
                    ""
                };
                if let Some(step) = current_step.as_mut() {
                    if !step.run.is_empty() {
                        step.run.push('\n');
                    }
                    step.run.push_str(run_line);
                }
                continue;
            }
            run_indent = None;
        }

        if line.starts_with("  ") && !line.starts_with("    ") && line.ends_with(':') {
            finish_step(&mut jobs, &current_job, &mut current_step);
            let job = line.trim().trim_end_matches(':').to_string();
            jobs.insert(job.clone(), WorkflowJob::default());
            current_job = Some(job);
            in_steps = false;
            in_env = false;
            continue;
        }

        if current_job.is_none() {
            continue;
        }

        if line == "    steps:" {
            finish_step(&mut jobs, &current_job, &mut current_step);
            in_steps = true;
            in_env = false;
            continue;
        }

        if !in_steps {
            continue;
        }

        if let Some(rest) = line.strip_prefix("      - ") {
            finish_step(&mut jobs, &current_job, &mut current_step);
            let mut step = Step::default();
            if let Some((key, value)) = rest.split_once(':') {
                if key.trim() == "name" {
                    step.name = Some(unquote(value));
                }
            }
            current_step = Some(step);
            in_env = false;
            continue;
        }

        let Some(step) = current_step.as_mut() else {
            continue;
        };

        if line == "        env:" {
            in_env = true;
            continue;
        }

        if in_env {
            if let Some(env_line) = line.strip_prefix("          ") {
                if let Some((key, value)) = env_line.split_once(':') {
                    step.env.insert(key.trim().to_string(), unquote(value));
                    continue;
                }
            }
            in_env = false;
        }

        let Some(field) = line.strip_prefix("        ") else {
            continue;
        };
        if let Some((key, value)) = field.split_once(':') {
            match (key.trim(), value.trim()) {
                ("name", value) => step.name = Some(unquote(value)),
                ("shell", value) => step.shell = Some(unquote(value)),
                ("run", "|") => run_indent = Some(10),
                _ => {}
            }
        }
    }

    finish_step(&mut jobs, &current_job, &mut current_step);
    jobs
}

fn success_step_errors(source: &str) -> Vec<String> {
    let jobs = parse_workflow_jobs(source);
    let Some(job) = jobs.get("reproduce-hub-release") else {
        return vec!["reproducible-build job is missing".to_string()];
    };
    let matching: Vec<&Step> = job
        .steps
        .iter()
        .filter(|step| step.name.as_deref() == Some("success'"))
        .collect();
    if matching.len() != 1 {
        return vec!["success proof step is not unique".to_string()];
    }
    let step = matching[0];
    let mut errors = Vec::new();
    if step.shell.as_deref() != Some("pwsh") {
        errors.push("success proof must run under PowerShell".to_string());
    }

    let env: BTreeSet<&str> = step.env.keys().map(String::as_str).collect();
    let expected_env = BTreeSet::from(["CANDIDATE_TAG", "SOURCE_COMMIT", "SOURCE_TREE"]);
    if env != expected_env {
        errors.push("success proof must bind tag, commit, and tree".to_string());
    }

    let commands: BTreeSet<&str> = step.run.lines().map(str::trim).collect();
    for (command, message) in [
        (
            "$proof = Get-Content reproducible-build-proof.json -Raw | ConvertFrom-Json",
            "success proof must load the retained proof artifact",
        ),
        (
            "$releasedHash = (Get-FileHash $env:RELEASED_EXE -Algorithm SHA256).Hash.ToLowerInvariant()",
            "success proof must rehash the released executable",
        ),
        (
            "$rebuiltHash = (Get-FileHash $env:REBUILT_EXE -Algorithm SHA256).Hash.ToLowerInvariant()",
            "success proof must rehash the rebuilt executable",
        ),
        (
            "$installerHash = (Get-FileHash $env:RELEASE_INSTALLER -Algorithm SHA256).Hash.ToLowerInvariant()",
            "success proof must rehash the released installer",
        ),
        (
            "$actualCommit = (git rev-parse HEAD).Trim()",
            "success proof must read the checked-out commit",
        ),
        (
            "$actualTree = (git rev-parse HEAD^{tree}).Trim()",
            "success proof must read the checked-out tree",
        ),
    ] {
        if !commands.contains(command) {
            errors.push(message.to_string());
        }
    }

    let run_words: BTreeSet<&str> = step
        .run
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' || ch == '^'))
        .filter(|word| !word.is_empty())
        .collect();
    for (required, message) in [
        (
            [
                "$actualCommit",
                "$env",
                "SOURCE_COMMIT",
                "$actualTree",
                "SOURCE_TREE",
                "throw",
            ]
            .as_slice(),
            "success proof must refuse a proof from the wrong source",
        ),
        (
            [
                "$proof",
                "candidateTag",
                "$env",
                "CANDIDATE_TAG",
                "sourceCommit",
                "SOURCE_COMMIT",
                "sourceTree",
                "SOURCE_TREE",
                "throw",
            ]
            .as_slice(),
            "success proof must bind proof metadata to the resolved source",
        ),
        (
            [
                "$proof",
                "installer",
                "name",
                "$env",
                "RELEASE_INSTALLER_NAME",
                "sha256",
                "$installerHash",
                "throw",
            ]
            .as_slice(),
            "success proof must bind the installer bytes",
        ),
        (
            [
                "$proof",
                "releasedExecutable",
                "rebuiltExecutable",
                "$releasedHash",
                "$rebuiltHash",
                "throw",
            ]
            .as_slice(),
            "success proof must compare released and rebuilt bytes",
        ),
    ] {
        if !required.iter().all(|word| run_words.contains(word)) {
            errors.push(message.to_string());
        }
    }

    if !commands.contains("$releasedHash -cne $rebuiltHash) {") {
        errors.push("success proof must fail when executable hashes differ".to_string());
    }

    errors
}

#[test]
fn success() {
    assert_eq!(success_step_errors(REPRO_WORKFLOW), Vec::<String>::new());

    let missing_tree = REPRO_WORKFLOW.replace(
        "          SOURCE_TREE: ${{ steps.source.outputs.source_tree }}\n",
        "",
    );
    assert!(
        success_step_errors(&missing_tree)
            .iter()
            .any(|error| error == "success proof must bind tag, commit, and tree"),
        "test must reject a success proof that is not bound to the source tree"
    );

    let permissive_compare = REPRO_WORKFLOW.replace(
        "$releasedHash -cne $rebuiltHash",
        "$releasedHash -ceq $rebuiltHash",
    );
    assert!(
        success_step_errors(&permissive_compare)
            .iter()
            .any(|error| error == "success proof must fail when executable hashes differ"),
        "test must reject a success proof that accepts differing bytes"
    );
}

fn section<'a>(markdown: &'a str, heading: &str) -> &'a str {
    let marker = format!("\n{heading}\n");
    let body = markdown
        .split_once(&marker)
        .unwrap_or_else(|| panic!("{heading} section is missing"))
        .1;
    let level = heading.chars().take_while(|ch| *ch == '#').count();
    let mut end = body.len();
    for candidate in 1..=level {
        if let Some(index) = body.find(&format!("\n{} ", "#".repeat(candidate))) {
            end = end.min(index);
        }
    }
    &body[..end]
}

fn ordered_items(section: &str) -> Vec<(String, String)> {
    let mut items = Vec::new();
    let mut current: Option<(String, Vec<String>)> = None;

    for line in section.lines() {
        let number = line
            .split_once(". **")
            .filter(|(prefix, _)| prefix.chars().all(|ch| ch.is_ascii_digit()));
        if let Some((_number, rest)) = number {
            if let Some((label, body)) = rest.split_once(":** ") {
                if let Some((old_label, old_body)) = current.take() {
                    items.push((old_label, old_body.join(" ")));
                }
                current = Some((label.to_string(), vec![body.trim().to_string()]));
                continue;
            }
        }
        if let Some((_label, body)) = current.as_mut() {
            if line.starts_with("   ") {
                body.push(line.trim().to_string());
            }
        }
    }

    if let Some((label, body)) = current {
        items.push((label, body.join(" ")));
    }
    items
}

fn words(source: &str) -> BTreeSet<String> {
    source
        .to_ascii_lowercase()
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn burn_contract_errors(markdown: &str) -> Vec<String> {
    let burn = section(markdown, "## Burn");
    let items = ordered_items(burn);
    let labels: Vec<&str> = items.iter().map(|(label, _)| label.as_str()).collect();
    let expected_labels = [
        "Local cleanup",
        "OSL server cleanup",
        "Cooperative peer request",
        "Exact scope",
        "Honest result",
    ];
    let mut errors = Vec::new();
    if labels != expected_labels {
        errors.push("Burn must encode exactly the five guarantee labels".to_string());
    }

    let item_words: BTreeMap<&str, BTreeSet<String>> = items
        .iter()
        .map(|(label, body)| (label.as_str(), words(body)))
        .collect();
    for (label, required) in [
        (
            "Local cleanup",
            [
                "shreds",
                "local",
                "stored",
                "ciphertext",
                "nonce",
                "cached",
                "attachments",
            ]
            .as_slice(),
        ),
        (
            "OSL server cleanup",
            ["server", "side", "state", "selected", "osl", "scope"].as_slice(),
        ),
        (
            "Cooperative peer request",
            [
                "absence",
                "consent",
                "binding",
                "authority",
                "refused",
                "unavailable",
            ]
            .as_slice(),
        ),
        (
            "Exact scope",
            [
                "reviewed",
                "confirmed",
                "scope",
                "requires",
                "confirmation",
                "again",
            ]
            .as_slice(),
        ),
        (
            "Honest result",
            [
                "actually",
                "verified",
                "never",
                "displayed",
                "deletion",
                "unsupported",
                "unverified",
            ]
            .as_slice(),
        ),
    ] {
        let Some(body_words) = item_words.get(label) else {
            errors.push(format!("Burn guarantee is missing: {label}"));
            continue;
        };
        if !required.iter().all(|word| body_words.contains(*word)) {
            errors.push(format!("Burn guarantee is incomplete: {label}"));
        }
    }

    let burn_words = words(burn);
    for required in [
        ["burn", "does", "not", "un", "send"].as_slice(),
        ["does", "not", "delete", "carrier", "messages"].as_slice(),
        ["does", "not", "erase", "provider", "retention"].as_slice(),
        ["screenshots"].as_slice(),
        ["already", "read", "plaintext"].as_slice(),
        ["can", "remain", "readable"].as_slice(),
    ] {
        if !required.iter().all(|word| burn_words.contains(*word)) {
            errors.push("Burn limitation is incomplete".to_string());
        }
    }

    let quoted: BTreeSet<&str> = burn.split('"').skip(1).step_by(2).collect();
    let expected_quoted = BTreeSet::from([
        "cryptographic burn",
        "destroys keys, not messages",
        "permanent ciphertext",
        "permanent gibberish",
        "mathematically opaque",
        "disappears forever",
        "permanently undecryptable",
        "gone for good",
    ]);
    if quoted != expected_quoted {
        errors.push("Burn banned phrases changed".to_string());
    }

    errors
}

#[test]
fn docs_design_osl_simple_spec_md() {
    assert_eq!(burn_contract_errors(SIMPLE_SPEC), Vec::<String>::new());

    let optimistic = SIMPLE_SPEC
        .replace("5. **Honest result:**", "5. **Optimistic result:**")
        .replace("\"gone for good\"", "\"secure cleanup\"");
    let optimistic_errors = burn_contract_errors(&optimistic);
    assert!(
        optimistic_errors
            .iter()
            .any(|error| error == "Burn must encode exactly the five guarantee labels"),
        "test must reject a renamed honest-result guarantee"
    );
    assert!(
        optimistic_errors
            .iter()
            .any(|error| error == "Burn banned phrases changed"),
        "test must reject changed banned phrase coverage"
    );

    let permissive_peer = SIMPLE_SPEC.replace(
        "absence of consent, binding, authority, transport delivery or\n   verification means the peer cleanup is refused or reported as unavailable.",
        "transport delivery means peer cleanup can be reported as available.",
    );
    assert!(
        burn_contract_errors(&permissive_peer)
            .iter()
            .any(|error| error == "Burn guarantee is incomplete: Cooperative peer request"),
        "test must reject peer cleanup without consent, binding, and authority refusal"
    );
}
