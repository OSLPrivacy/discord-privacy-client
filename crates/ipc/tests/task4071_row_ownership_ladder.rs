use ipc::commands::{
    cmd_osl_check_row_ownership_marking_admission, cmd_osl_read_row_ownership_ladder,
};
use ipc::AppState;

#[test]
fn task4071_direct_command_reads_ladder_and_refuses_below_line_evidence() {
    let state = AppState::new();
    let ladder = cmd_osl_read_row_ownership_ladder(&state).expect("read ladder");

    println!("TASK4071 direct_command=cmd_osl_read_row_ownership_ladder");
    println!("TASK4071 ordered_by={}", ladder.ordered_by);
    println!("TASK4071 evidence_kind_count={}", ladder.kinds.len());
    for kind in &ladder.kinds {
        println!(
            "TASK4071 ladder rank={} kind={} strength={} may_mark_row={} why={}",
            kind.rank, kind.kind, kind.strength, kind.may_mark_row, kind.why
        );
    }
    println!(
        "TASK4071 minimum_marking_kind={}",
        ladder.minimum_marking_kind
    );
    println!("TASK4071 app_clearance_rule={}", ladder.app_clearance_rule);
    println!("TASK4071 name_alone_rule={}", ladder.name_alone_rule);
    println!(
        "TASK4071 position_or_bubble_color_kind_count={}",
        ladder.forbidden_position_or_bubble_color_kinds
    );

    let discord = ladder
        .kinds
        .iter()
        .find(|kind| kind.example_app.as_deref() == Some("Discord"))
        .expect("Discord top example");
    println!("TASK4071 discord_top_kind={}", discord.kind);
    println!("TASK4071 discord_top_rank={}", discord.rank);
    println!("TASK4071 discord_compare_count={}", discord.compares.len());
    for piece in &discord.compares {
        println!("TASK4071 discord_compares={piece}");
    }

    let minimum = cmd_osl_check_row_ownership_marking_admission(
        &state,
        "WhatsApp".to_string(),
        ladder.minimum_marking_kind.clone(),
    )
    .expect("minimum evidence is accepted");
    println!("TASK4071 minimum_check.accepted={}", minimum.accepted);
    println!(
        "TASK4071 minimum_check.evidence_kind={}",
        minimum.evidence_kind
    );

    let weak_refusal = cmd_osl_check_row_ownership_marking_admission(
        &state,
        "Signal".to_string(),
        "visible_display_name_match".to_string(),
    )
    .expect_err("name-only evidence is below the marking line");
    println!("TASK4071 refusal_direct_command=cmd_osl_check_row_ownership_marking_admission");
    println!("TASK4071 below_line_refused=true");
    println!("TASK4071 below_line_refusal={weak_refusal}");

    assert!(ladder.kinds.len() >= 4);
    assert_eq!(ladder.ordered_by, "strongest_to_weakest");
    assert!(ladder
        .kinds
        .windows(2)
        .all(|pair| pair[0].rank < pair[1].rank));
    assert_eq!(ladder.minimum_marking_kind, "owner_only_row_control");
    assert_eq!(
        ladder.app_clearance_rule,
        "Every app must present owner_only_row_control or stronger before OSL is allowed to mark a row at all."
    );
    assert_eq!(
        ladder.name_alone_rule,
        "A name on its own is weak and may only ever narrow an answer, never make one."
    );
    let accepted_position_or_bubble_color_kinds: Vec<_> = ladder
        .kinds
        .iter()
        .filter(|kind| kind.based_on_position_or_bubble_color && kind.may_mark_row)
        .map(|kind| kind.kind.as_str())
        .collect();
    assert!(
        accepted_position_or_bubble_color_kinds.is_empty(),
        "position_or_bubble_color accepted evidence kinds present: {}",
        accepted_position_or_bubble_color_kinds.join(",")
    );
    assert_eq!(ladder.forbidden_position_or_bubble_color_kinds, 0);
    assert_eq!(discord.kind, "platform_account_number_match");
    assert_eq!(discord.rank, 1);
    assert_eq!(
        discord.compares,
        vec![
            "numbered account taken off each row",
            "signed-in account's own number read separately from the account panel"
        ]
    );
    assert!(minimum.accepted);
    assert_eq!(minimum.evidence_kind, "owner_only_row_control");
    assert_eq!(
        weak_refusal,
        "OSL: Signal evidence visible_display_name_match is below the row-ownership marking line; A name on its own is weak and may only ever narrow an answer, never make one."
    );
}
