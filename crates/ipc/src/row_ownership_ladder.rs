#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RowOwnershipEvidenceKindDto {
    pub rank: u8,
    pub kind: String,
    pub strength: String,
    pub why: String,
    pub may_mark_row: bool,
    pub based_on_position_or_bubble_color: bool,
    pub example_app: Option<String>,
    pub compares: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RowOwnershipLadderDto {
    pub ordered_by: String,
    pub minimum_marking_kind: String,
    pub app_clearance_rule: String,
    pub name_alone_rule: String,
    pub forbidden_position_or_bubble_color_kinds: u8,
    pub kinds: Vec<RowOwnershipEvidenceKindDto>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RowOwnershipMarkingAdmissionDto {
    pub app_name: String,
    pub evidence_kind: String,
    pub evidence_rank: u8,
    pub minimum_marking_kind: String,
    pub minimum_rank: u8,
    pub accepted: bool,
}

const MINIMUM_MARKING_KIND: &str = "owner_only_row_control";

pub fn row_ownership_ladder() -> RowOwnershipLadderDto {
    let kinds = vec![
        RowOwnershipEvidenceKindDto {
            rank: 1,
            kind: "platform_account_number_match".to_string(),
            strength: "strongest".to_string(),
            why: "A stable numbered account is taken from the row and compared with the signed-in account number read independently from the account panel.".to_string(),
            may_mark_row: true,
            based_on_position_or_bubble_color: false,
            example_app: Some("Discord".to_string()),
            compares: vec![
                "numbered account taken off each row".to_string(),
                "signed-in account's own number read separately from the account panel".to_string(),
            ],
        },
        RowOwnershipEvidenceKindDto {
            rank: 2,
            kind: "verified_sender_address_match".to_string(),
            strength: "strong".to_string(),
            why: "The row exposes a service-owned sender address or account id, and the signed-in account's address or id is read from a separate profile source.".to_string(),
            may_mark_row: true,
            based_on_position_or_bubble_color: false,
            example_app: None,
            compares: Vec::new(),
        },
        RowOwnershipEvidenceKindDto {
            rank: 3,
            kind: "signed_in_sender_metadata_match".to_string(),
            strength: "strong".to_string(),
            why: "Structured message metadata says the sender is the signed-in account, and that signed-in account is established outside the row.".to_string(),
            may_mark_row: true,
            based_on_position_or_bubble_color: false,
            example_app: None,
            compares: Vec::new(),
        },
        RowOwnershipEvidenceKindDto {
            rank: 4,
            kind: MINIMUM_MARKING_KIND.to_string(),
            strength: "minimum".to_string(),
            why: "A service-gated row control, such as an own-row delete or edit action, is available on that row while the app keeps the row identity stable.".to_string(),
            may_mark_row: true,
            based_on_position_or_bubble_color: false,
            example_app: None,
            compares: Vec::new(),
        },
        RowOwnershipEvidenceKindDto {
            rank: 5,
            kind: "visible_display_name_match".to_string(),
            strength: "weak".to_string(),
            why: "A visible name can be chosen or copied by another account; a name on its own may only narrow an answer, never make one.".to_string(),
            may_mark_row: false,
            based_on_position_or_bubble_color: false,
            example_app: None,
            compares: Vec::new(),
        },
    ];
    RowOwnershipLadderDto {
        ordered_by: "strongest_to_weakest".to_string(),
        minimum_marking_kind: MINIMUM_MARKING_KIND.to_string(),
        app_clearance_rule: "Every app must present owner_only_row_control or stronger before OSL is allowed to mark a row at all.".to_string(),
        name_alone_rule: "A name on its own is weak and may only ever narrow an answer, never make one.".to_string(),
        forbidden_position_or_bubble_color_kinds: kinds
            .iter()
            .filter(|kind| kind.based_on_position_or_bubble_color)
            .count() as u8,
        kinds,
    }
}

pub fn check_row_ownership_marking_admission(
    app_name: String,
    evidence_kind: String,
) -> Result<RowOwnershipMarkingAdmissionDto, String> {
    let ladder = row_ownership_ladder();
    let minimum = ladder
        .kinds
        .iter()
        .find(|kind| kind.kind == ladder.minimum_marking_kind)
        .expect("row ownership ladder must name its minimum marking kind");
    let evidence = ladder
        .kinds
        .iter()
        .find(|kind| kind.kind == evidence_kind)
        .ok_or_else(|| {
            format!(
                "OSL: {} row ownership evidence {} is not on the ladder",
                app_name, evidence_kind
            )
        })?;

    if !evidence.may_mark_row {
        return Err(format!(
            "OSL: {} evidence {} is below the row-ownership marking line; {}",
            app_name, evidence.kind, ladder.name_alone_rule
        ));
    }

    Ok(RowOwnershipMarkingAdmissionDto {
        app_name,
        evidence_kind: evidence.kind.clone(),
        evidence_rank: evidence.rank,
        minimum_marking_kind: minimum.kind.clone(),
        minimum_rank: minimum.rank,
        accepted: true,
    })
}
