use serde::Deserialize;
use std::{env, fs, process};

#[derive(Debug, Deserialize)]
struct ProbePage {
    app: String,
    page_read_date: String,
    signed_in: bool,
    conversation_state: String,
    rows: Vec<ProbeRow>,
}

#[derive(Debug, Deserialize)]
struct ProbeRow {
    test_name: Option<String>,
    identifier: Option<String>,
    who_wrote_it: Option<String>,
    roles: Vec<String>,
    states: Vec<String>,
    parent_containers: Vec<String>,
    beside_boxes: Vec<String>,
    picture_address: Option<String>,
    account_link: Option<AccountLink>,
    own_only_mark: Option<String>,
    screen_reader: Vec<String>,
    evidence_kinds: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct AccountLink {
    name: String,
    href: String,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut surface = None::<String>;
    let mut fixture = None::<String>;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--surface" => surface = args.next(),
            "--fixture" => fixture = args.next(),
            other => return Err(format!("OSL: unknown task 4082 argument {other}")),
        }
    }
    let surface = surface.ok_or_else(|| "OSL: task 4082 --surface is required".to_owned())?;
    let fixture = fixture.ok_or_else(|| "OSL: task 4082 --fixture is required".to_owned())?;
    let raw = fs::read_to_string(&fixture)
        .map_err(|error| format!("OSL: task 4082 could not read {fixture}: {error}"))?;
    let page: ProbePage = serde_json::from_str(&raw)
        .map_err(|error| format!("OSL: task 4082 fixture is not valid JSON: {error}"))?;
    if page.app != surface {
        return Err(format!(
            "OSL: task 4082 fixture app {} did not match requested surface {surface}",
            page.app
        ));
    }
    if !page.signed_in {
        return Err(format!(
            "OSL: task 4082 probe refused signed-out {} page",
            page.app
        ));
    }
    if page.conversation_state != "open_conversation" {
        return Err(format!(
            "OSL: task 4082 probe needs one open conversation, found {}",
            page.conversation_state
        ));
    }
    if page.rows.is_empty() {
        return Err(format!(
            "OSL: task 4082 probe refused empty {} result",
            page.app
        ));
    }

    match surface.as_str() {
        "discord" => print_discord_control(&page),
        "instagram" => print_instagram_probe(&page),
        other => Err(format!("OSL: task 4082 unsupported surface {other}")),
    }
}

fn print_discord_control(page: &ProbePage) -> Result<(), String> {
    let who_wrote_it_rows = page
        .rows
        .iter()
        .filter(|row| matches!(row.who_wrote_it.as_deref(), Some("yours" | "theirs")))
        .count();
    println!("{who_wrote_it_rows} TASK4082_DISCORD_CONTROL_WHO_WROTE_IT_ROWS={who_wrote_it_rows}");
    println!("TASK4082_DISCORD_CONTROL_TOTAL_ROWS={}", page.rows.len());
    println!(
        "TASK4082_DISCORD_CONTROL_COLOUR_OR_POSITION_FINDINGS={}",
        colour_or_position_findings(&page.rows)
    );
    if who_wrote_it_rows < 8 {
        return Err(format!(
            "OSL: task 4082 Discord control only found {who_wrote_it_rows} who-wrote-it rows"
        ));
    }
    Ok(())
}

fn print_instagram_probe(page: &ProbePage) -> Result<(), String> {
    let mut printed_rows = 0usize;
    for (index, row) in page.rows.iter().enumerate() {
        printed_rows += 1;
        println!(
            "TASK4082_INSTAGRAM_ROW index={index:02} test_name={} identifier={} who_wrote_it={} roles={} states={} parent_containers={} beside_boxes={} picture_address={} account_link={} own_only_mark={} screen_reader={} evidence={}",
            field(row.test_name.as_deref()),
            field(row.identifier.as_deref()),
            field(row.who_wrote_it.as_deref()),
            list(&row.roles),
            list(&row.states),
            list(&row.parent_containers),
            list(&row.beside_boxes),
            field(row.picture_address.as_deref()),
            account_link(row.account_link.as_ref()),
            field(row.own_only_mark.as_deref()),
            list(&row.screen_reader),
            list(&row.evidence_kinds),
        );
    }
    let missing_lines = page.rows.len().saturating_sub(printed_rows);
    let colour_or_position = colour_or_position_findings(&page.rows);
    println!("TASK4082_INSTAGRAM_TOTAL_ROWS={}", page.rows.len());
    println!("TASK4082_INSTAGRAM_ROWS_WITH_NO_LINE={missing_lines}");
    println!("TASK4082_INSTAGRAM_COLOUR_OR_POSITION_FINDINGS={colour_or_position}");
    println!("TASK4082_INSTAGRAM_PAGE_READ_DATE={}", page.page_read_date);
    if page.rows.len() != 10 || missing_lines != 0 || colour_or_position != 0 {
        return Err("OSL: task 4082 Instagram probe did not satisfy the row-count gate".to_owned());
    }
    Ok(())
}

fn colour_or_position_findings(rows: &[ProbeRow]) -> usize {
    rows.iter()
        .flat_map(|row| row.evidence_kinds.iter())
        .filter(|kind| {
            let lower = kind.to_ascii_lowercase();
            lower.contains("colour")
                || lower.contains("color")
                || lower.contains("position")
                || lower.contains("bubble")
        })
        .count()
}

fn field(value: Option<&str>) -> &str {
    match value {
        Some(value) if !value.trim().is_empty() => value,
        _ => "none",
    }
}

fn list(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_owned()
    } else {
        values.join("|")
    }
}

fn account_link(value: Option<&AccountLink>) -> String {
    match value {
        Some(link) if !link.name.trim().is_empty() || !link.href.trim().is_empty() => {
            format!("{}->{}", field(Some(&link.name)), field(Some(&link.href)))
        }
        _ => "none".to_owned(),
    }
}
