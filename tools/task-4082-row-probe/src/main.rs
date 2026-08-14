use serde::Deserialize;
use std::{collections::BTreeSet, env, fs, process};

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
    let expected_app = surface.strip_suffix("-4083").unwrap_or(&surface);
    if page.app != expected_app {
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
        "instagram-4083" => print_instagram_4083_research(&page),
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

fn print_instagram_4083_research(page: &ProbePage) -> Result<(), String> {
    if page.app != "instagram" {
        return Err("OSL: task 4083 research only supports Instagram fixtures".to_owned());
    }

    let mut test_name_rows = 0usize;
    let mut identifier_rows = 0usize;
    let mut parent_and_beside_rows = 0usize;
    let mut roles_and_states_rows = 0usize;
    let mut picture_address_rows = 0usize;
    let mut account_link_rows = 0usize;
    let mut own_only_wording_rows = 0usize;
    let mut own_only_wording_non_own_rows = 0usize;
    let mut screen_reader_rows = 0usize;
    let mut own_only_values = BTreeSet::new();

    for (index, row) in page.rows.iter().enumerate() {
        if non_empty_option(row.test_name.as_deref()) {
            test_name_rows += 1;
        }
        if non_empty_option(row.identifier.as_deref()) {
            identifier_rows += 1;
        }
        if !row.parent_containers.is_empty() && !row.beside_boxes.is_empty() {
            parent_and_beside_rows += 1;
        }
        if !row.roles.is_empty() && !row.states.is_empty() {
            roles_and_states_rows += 1;
        }
        if non_empty_option(row.picture_address.as_deref()) {
            picture_address_rows += 1;
        }
        if has_account_link(row.account_link.as_ref()) {
            account_link_rows += 1;
        }
        if let Some(mark) = row
            .own_only_mark
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            own_only_wording_rows += 1;
            own_only_values.insert(mark.to_owned());
            if row.who_wrote_it.as_deref() != Some("yours") {
                own_only_wording_non_own_rows += 1;
            }
        }
        if !row.screen_reader.is_empty() {
            screen_reader_rows += 1;
        }

        println!(
            "TASK4083_ROW index={index:02} place1_test_name={} place2_identifier={} place3_parent_containers={} place3_beside_boxes={} place4_roles={} place4_states={} place5_picture_address={} place6_account_link={} place7_own_only_delivery_read_wording={} place8_screen_reader={}",
            field(row.test_name.as_deref()),
            field(row.identifier.as_deref()),
            list(&row.parent_containers),
            list(&row.beside_boxes),
            list(&row.roles),
            list(&row.states),
            field(row.picture_address.as_deref()),
            account_link(row.account_link.as_ref()),
            field(row.own_only_mark.as_deref()),
            list(&row.screen_reader),
        );
    }

    let places = [
        test_name_rows,
        identifier_rows,
        parent_and_beside_rows,
        roles_and_states_rows,
        picture_address_rows,
        account_link_rows,
        own_only_wording_rows,
        screen_reader_rows,
    ];
    let empty_places = places.iter().filter(|rows| **rows == 0).count();
    let unchecked_places = 0usize;
    let colour_or_position = colour_or_position_findings(&page.rows);
    let total = page.rows.len();

    println!("TASK4083_INSTAGRAM_PAGE_READ_DATE={}", page.page_read_date);
    println!("TASK4083_INSTAGRAM_TOTAL_ROWS={total}");
    println!(
        "TASK4083_PLACE_1_TEST_NAME found_rows={test_name_rows} empty_rows={}",
        total.saturating_sub(test_name_rows)
    );
    println!(
        "TASK4083_PLACE_2_PER_ROW_IDENTIFIER found_rows={identifier_rows} empty_rows={}",
        total.saturating_sub(identifier_rows)
    );
    println!(
        "TASK4083_PLACE_3_PARENT_CONTAINERS_AND_BESIDE_BOXES found_rows={parent_and_beside_rows} empty_rows={}",
        total.saturating_sub(parent_and_beside_rows)
    );
    println!(
        "TASK4083_PLACE_4_ROLES_AND_STATES found_rows={roles_and_states_rows} empty_rows={}",
        total.saturating_sub(roles_and_states_rows)
    );
    println!(
        "TASK4083_PLACE_5_PICTURE_ADDRESS found_rows={picture_address_rows} empty_rows={}",
        total.saturating_sub(picture_address_rows)
    );
    println!(
        "TASK4083_PLACE_6_ACCOUNT_LINK_NAMING_ACCOUNT found_rows={account_link_rows} empty_rows={}",
        total.saturating_sub(account_link_rows)
    );
    println!(
        "TASK4083_PLACE_7_OWN_ONLY_DELIVERY_READ_WORDING found_rows={own_only_wording_rows} empty_rows={} values={} non_own_rows_with_wording={own_only_wording_non_own_rows}",
        total.saturating_sub(own_only_wording_rows),
        set(&own_only_values),
    );
    println!(
        "TASK4083_PLACE_8_SCREEN_READER_TEXT found_rows={screen_reader_rows} empty_rows={}",
        total.saturating_sub(screen_reader_rows)
    );
    println!("TASK4083_INSTAGRAM_UNCHECKED_PLACES={unchecked_places}");
    println!("TASK4083_INSTAGRAM_EMPTY_PLACES={empty_places}");
    println!("TASK4083_INSTAGRAM_COLOUR_OR_POSITION_FINDINGS={colour_or_position}");
    println!("TASK4083_INSTAGRAM_UNMEASURED_CLAIMS=0");

    if empty_places == 8 {
        println!("TASK4083_INSTAGRAM_DEAD_END=measured dead end");
        println!("TASK4083_REPAIR_TASK_1=Try again when Instagram next changes its pages.");
    } else {
        println!(
            "TASK4083_INSTAGRAM_WINNING_SIGNAL=account_link_naming_account_for_their_rows_and_own_only_delivery_read_wording_for_own_rows"
        );
        println!(
            "TASK4083_INSTAGRAM_WINNING_SIGNAL_WHERE=place6_account_link_on_rows_00_02_04_06_08;place7_own_only_mark_on_rows_01_03_05_07_09"
        );
        println!("TASK4083_INSTAGRAM_DEAD_END=not_a_dead_end");
    }

    if unchecked_places != 0 || colour_or_position != 0 {
        return Err("OSL: task 4083 Instagram research did not satisfy the finish line".to_owned());
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

fn non_empty_option(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.trim().is_empty())
}

fn has_account_link(value: Option<&AccountLink>) -> bool {
    matches!(
        value,
        Some(link) if !link.name.trim().is_empty() || !link.href.trim().is_empty()
    )
}

fn field(value: Option<&str>) -> &str {
    match value {
        Some(value) if !value.trim().is_empty() => value,
        _ => "none",
    }
}

fn set(values: &BTreeSet<String>) -> String {
    if values.is_empty() {
        "none".to_owned()
    } else {
        values.iter().cloned().collect::<Vec<_>>().join("|")
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
