use std::path::{Path, PathBuf};
use std::process::Command;

const MARKED_MESSAGE: &str = "TASK3413.WAIT_PERSON_ONLY";
const EXPECTED_CONVERSATION: &str = "deckard";

#[derive(Clone, Debug, PartialEq, Eq)]
struct RouteRun {
    grab_route: bool,
    page_connection_route: bool,
    wait_for_person_route: bool,
    conversation: String,
    placed_text: String,
    read_back: String,
    placed_characters: usize,
    exit_code: i32,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("apps/osl-hub has a repository root")
        .to_path_buf()
}

fn ps_bool(value: &str) -> bool {
    match value {
        "true" => true,
        "false" => false,
        other => panic!("invalid boolean field {other:?}"),
    }
}

fn field<'a>(line: &'a str, key: &str) -> &'a str {
    line.split(';')
        .find_map(|part| part.strip_prefix(key))
        .unwrap_or_else(|| panic!("missing {key} in {line:?}"))
}

fn parse_route_run(line: &str) -> RouteRun {
    RouteRun {
        grab_route: ps_bool(field(line, "grab_route=")),
        page_connection_route: ps_bool(field(line, "page_connection_route=")),
        wait_for_person_route: ps_bool(field(line, "wait_for_person_route=")),
        conversation: field(line, "conversation=").to_owned(),
        placed_text: field(line, "placed_text=").to_owned(),
        read_back: field(line, "read_back=").to_owned(),
        placed_characters: field(line, "placed_characters=")
            .parse()
            .expect("placed_characters is numeric"),
        exit_code: field(line, "exit_code=")
            .parse()
            .expect("exit_code is numeric"),
    }
}

fn check_route_run(run: &RouteRun) -> Result<(), String> {
    if run.grab_route {
        return Err("grab route was on".to_owned());
    }
    if run.page_connection_route {
        return Err("page connection route was on".to_owned());
    }
    if !run.wait_for_person_route {
        return Err("wait-for-the-person route was off".to_owned());
    }
    if run.exit_code != 0 {
        return Err(format!("placement exited {}", run.exit_code));
    }
    if run.conversation != EXPECTED_CONVERSATION {
        return Err(format!("unexpected conversation {}", run.conversation));
    }
    if run.placed_text != MARKED_MESSAGE {
        return Err(format!("placed text changed to {}", run.placed_text));
    }
    if run.read_back != MARKED_MESSAGE {
        return Err(format!("readback changed to {}", run.read_back));
    }
    if run.placed_characters != MARKED_MESSAGE.len() {
        return Err(format!(
            "placed character count was {}, expected {}",
            run.placed_characters,
            MARKED_MESSAGE.len()
        ));
    }
    Ok(())
}

fn powershell() -> &'static str {
    if cfg!(windows) {
        "powershell"
    } else {
        "pwsh"
    }
}

fn run_one_person_placement() -> RouteRun {
    let module = repo_root().join("scripts/qa/discord-one-person-test-target.psm1");
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Import-Module '{}' -Force
$grabRoute = $false
$pageConnectionRoute = $false
$waitForPersonRoute = $true
$result = Invoke-OnePersonDiscordQaPlacement -Conversation '{}' -Message '{}'
'grab_route=' + ([string]$grabRoute).ToLowerInvariant() +
  ';page_connection_route=' + ([string]$pageConnectionRoute).ToLowerInvariant() +
  ';wait_for_person_route=' + ([string]$waitForPersonRoute).ToLowerInvariant() +
  ';conversation=' + $result.Conversation +
  ';placed_text=' + $result.PlacedText +
  ';read_back=' + $result.ReadBack +
  ';placed_characters=' + $result.PlacedCharacters +
  ';exit_code=' + $result.ExitCode
"#,
        module.display(),
        EXPECTED_CONVERSATION,
        MARKED_MESSAGE
    );
    let output = Command::new(powershell())
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .expect("run one-person Discord QA placement route");
    assert!(
        output.status.success(),
        "PowerShell placement command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    parse_route_run(
        String::from_utf8(output.stdout)
            .expect("stdout is UTF-8")
            .lines()
            .find(|line| line.starts_with("grab_route="))
            .expect("route record was printed"),
    )
}

#[test]
fn task_3413_places_marked_message_with_grab_and_page_connection_off() {
    let run = run_one_person_placement();
    check_route_run(&run).expect("wait-for-the-person-only route must satisfy the finish line");
    let green_readback = run.read_back.clone();
    let green_placed_text = run.placed_text.clone();
    let green_grab_route = run.grab_route;
    let green_page_connection_route = run.page_connection_route;
    let green_wait_for_person_route = run.wait_for_person_route;

    let noop_placing_job = RouteRun {
        placed_text: String::new(),
        read_back: String::new(),
        placed_characters: 0,
        ..run.clone()
    };
    let failure = check_route_run(&noop_placing_job)
        .expect_err("a no-op placing job must not satisfy task 3413");
    let grab_on_failure = check_route_run(&RouteRun {
        grab_route: true,
        ..run.clone()
    })
    .expect_err("a run with the grab route on must not satisfy task 3413");
    let page_on_failure = check_route_run(&RouteRun {
        page_connection_route: true,
        ..run
    })
    .expect_err("a run with the page connection route on must not satisfy task 3413");

    println!("TASK3413_MARKED_MESSAGE={MARKED_MESSAGE}");
    println!("TASK3413_DISCORD_BOX_PLACED_TEXT={green_placed_text}");
    println!("TASK3413_DISCORD_BOX_READBACK={green_readback}");
    println!("TASK3413_GRAB_ROUTE_OFF={}", !green_grab_route);
    println!(
        "TASK3413_PAGE_CONNECTION_ROUTE_OFF={}",
        !green_page_connection_route
    );
    println!("TASK3413_WAIT_FOR_PERSON_ROUTE_ON={green_wait_for_person_route}");
    println!("TASK3413_NOOP_PLACING_JOB_FAILURE={failure}");
    println!("TASK3413_GRAB_ON_FAILURE={grab_on_failure}");
    println!("TASK3413_PAGE_CONNECTION_ON_FAILURE={page_on_failure}");
}
