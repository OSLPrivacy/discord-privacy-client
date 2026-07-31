use std::collections::BTreeSet;

const CURRENT_WINDOW_PROMPTS: &str =
    include_str!("../../../docs/design/osl-current-window-prompts-2026-07-26.md");

#[derive(Debug)]
struct MemoryCardRoute {
    active_account: String,
    prompt_source: String,
    route: String,
    volatile_rule: String,
}

fn section<'a>(markdown: &'a str, heading: &str) -> &'a str {
    let marker = format!("\n{heading}\n");
    let body = markdown
        .split_once(&marker)
        .unwrap_or_else(|| panic!("{heading} section is missing"))
        .1;
    let heading_level = heading.chars().take_while(|ch| *ch == '#').count();
    let mut end = body.len();
    for level in 1..=heading_level {
        if let Some(index) = body.find(&format!("\n{} ", "#".repeat(level))) {
            end = end.min(index);
        }
    }
    &body[..end]
}

fn words(source: &str) -> BTreeSet<String> {
    source
        .to_ascii_lowercase()
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn first_fenced_text_block(source: &str) -> &str {
    let (_, after_start) = source
        .split_once("```text\n")
        .expect("section must include a text prompt block");
    let (block, _) = after_start
        .split_once("\n```")
        .expect("text prompt block must be closed");
    block
}

fn memory_card_routes(markdown: &str) -> Vec<MemoryCardRoute> {
    let contract = section(markdown, "## Shared memory-card adoption contract");
    let table_lines: Vec<&str> = contract
        .lines()
        .filter(|line| line.trim_start().starts_with('|'))
        .collect();
    let header = table_lines
        .iter()
        .position(|line| {
            words(line).is_superset(&words(
                "active account window prompt source memory card route volatile status rule",
            ))
        })
        .expect("shared memory-card table header is missing");
    table_lines
        .iter()
        .skip(header + 2)
        .map(|line| {
            let cells: Vec<String> = line
                .trim()
                .trim_matches('|')
                .split('|')
                .map(|cell| cell.trim().to_string())
                .collect();
            assert_eq!(cells.len(), 4, "memory-card table row must have four cells");
            MemoryCardRoute {
                active_account: cells[0].clone(),
                prompt_source: cells[1].clone(),
                route: cells[2].clone(),
                volatile_rule: cells[3].clone(),
            }
        })
        .collect()
}

fn has_any(words: &BTreeSet<String>, options: &[&str]) -> bool {
    options.iter().any(|option| words.contains(*option))
}

#[test]
fn adopt_shared_memory_cards_across_every_active_account() {
    let routes = memory_card_routes(CURRENT_WINDOW_PROMPTS);
    let mut accounts: Vec<&str> = routes
        .iter()
        .map(|route| route.active_account.as_str())
        .collect();
    accounts.sort_unstable();
    assert_eq!(
        accounts,
        [
            "Coordinating Telegram `/osl` lane",
            "Existing Discord testing window",
            "Existing OSL Hub/UI window",
            "Existing Scrub window",
            "Existing two-way Opus test window",
            "New website/head-developer lane",
        ],
        "the contract must enumerate every active OSL account/window exactly once",
    );

    for route in &routes {
        let route_words = words(&route.route);
        let source_words = words(&route.prompt_source);
        let volatile_words = words(&route.volatile_rule);

        assert!(
            route_words.contains("memory") && route_words.contains("card"),
            "{} must route through the compact memory card",
            route.active_account
        );
        assert!(
            route_words.contains("before")
                || route_words.contains("first")
                || source_words.contains("common")
                || source_words.contains("bootstrap"),
            "{} must load shared context before bounded lane work",
            route.active_account
        );
        assert!(
            has_any(&route_words, &["common", "bootstrap", "prompt", "handoff"]),
            "{} must name the prompt/bootstrap path that carries the memory-card rule",
            route.active_account
        );
        assert!(
            volatile_words.contains("never")
                && volatile_words.contains("copy")
                && volatile_words.contains("volatile")
                && volatile_words.contains("status"),
            "{} must refuse volatile status in durable memory",
            route.active_account
        );
    }

    let common_update = first_fenced_text_block(section(
        CURRENT_WINDOW_PROMPTS,
        "## One update prompt for every active OSL tab",
    ));
    let common_words = words(common_update);
    assert!(
        common_words.contains("never")
            && common_words.contains("copy")
            && common_words.contains("volatile")
            && common_words.contains("status"),
        "the shared update must carry the volatile-status refusal",
    );
    assert!(
        common_words.contains("compact")
            && common_words.contains("memory")
            && common_words.contains("card"),
        "the shared update must instruct first-time accounts to save the compact memory card",
    );
    assert!(
        common_words.contains("already")
            && common_words.contains("read")
            && common_words.contains("only")
            && common_words.contains("revision"),
        "returning accounts must read only the bounded revision/task context",
    );

    let bootstrap = first_fenced_text_block(section(
        CURRENT_WINDOW_PROMPTS,
        "## Reusable safe new-window bootstrap",
    ));
    let bootstrap_words = words(bootstrap);
    assert!(
        bootstrap_words.contains("load")
            && bootstrap_words.contains("compact")
            && bootstrap_words.contains("memory")
            && bootstrap_words.contains("card")
            && bootstrap_words.contains("before")
            && bootstrap_words.contains("editing"),
        "new-window bootstrap must load the memory card before editing",
    );
}
