//! Guided deletion of the operator's OWN messages, through Discord's own UI.
//!
//! This is the `Scan -> Preview -> Confirm -> Execute -> Verify -> Receipt`
//! workflow from `docs/design/osl-gui-final-plan.md:490`, with the six activity
//! states from `:494`. It is a DISTINCT action from Burn. `docs/design/burn-contract.md:14`
//! says burn does not delete carrier messages from a native service, and this
//! module does not change that: nothing here destroys a key, and no burn result
//! may ever report platform removal. The three guarantees at `:496-500` stay
//! three separate facts, and this module can only ever establish the first one.
//!
//! ## What it may and may not use
//!
//! It drives Discord's own affordances: focus the operator's message row through
//! MSAA, open that row's context menu, choose Discord's own `Delete Message`
//! item, confirm in Discord's own dialog. It does NOT use the Discord REST API
//! with the operator's token; that is self-botting, it risks the operator's real
//! account, and `osl-gui-final-plan.md:508` forbids bypassing platform controls
//! merely because no official API exists. There is no flag for it and no code
//! path towards it.
//!
//! ## Why the logic lives here and not in the Windows half
//!
//! Every decision in this file is pure and platform-free, taken over an injected
//! [`DiscordDeletionSurface`]. The MSAA/COM half only answers questions and posts
//! keys. That split is the same one `RehydrateRowReaders` uses in the parent
//! module, and it exists because the safety properties -- one row at a time,
//! bottom-up, own messages only, confirm-before-execute, and above all
//! `Verified` only on a proven re-walk -- must be testable on any host.
//!
//! ## Content rules
//!
//! Nothing here holds, returns, logs, hashes or persists message text, the
//! draft, cover text or a conversation name. Rows are identified by shape
//! (height and child count), by their ordinal in the filtered row sequence, and
//! by their text LENGTH. The one place a row's text is compared byte-exactly is
//! inside the surface implementation, which holds it transiently and hands this
//! module back a COUNT. `RowCensus` is therefore the whole verification input,
//! and it is three integers and two enums.

use serde::Serialize;

use super::stable_hash;

/// Fixed contract name for this workflow's receipt. Bumped, never reinterpreted.
pub const GUIDED_DELETION_CONTRACT: &str = "discord_guided_deletion_v1";

/// Most rows one confirmed plan may attempt.
///
/// Matches `MAX_VISIBLE_CARRIER_ROWS`: a plan may never be broader than one
/// bounded transcript read can preview, because the operator has to be able to
/// see exactly what will be deleted before confirming an irreversible action.
pub const MAX_ROWS_PER_PLAN: usize = 32;

/// The exact Discord affordances one row's deletion drives, in order.
///
/// Shown in the preview so the operator sees the mechanism, not just a count.
/// Fixed `&'static str`, so a preview can never carry conversation content.
pub const PLATFORM_STEPS: &[&str] = &[
    "focus_your_message_row",
    "open_discords_own_row_menu",
    "choose_discords_delete_message_item",
    "confirm_in_discords_own_dialog",
    "re_read_the_transcript_to_prove_the_row_is_gone",
];

/// How long the ladder waits for the tree to change after ONE posted key,
/// cumulatively, testing between waits.
///
/// MEASURED, not guessed. Chromium does act on a posted `WM_KEYDOWN` to
/// `Chrome_RenderWidgetHostHWND` -- 240 posted `VK_BACK` sent four at a time with
/// 60 ms between batches emptied a 293-character composer down to Slate's empty
/// floor -- but it DROPS bursts: 500 of the same key posted back to back removed
/// about twenty characters. The product's own carrier write hit the identical
/// wall from the other direction, where 93 codepoints of `SendInput` landed 15
/// and the fix was paced per-chunk writes with a prefix proof. Two unrelated
/// paths, one mechanism: the renderer's queue, not the call, is the slow part.
///
/// So the rule for this ladder is structural rather than advisory: **one key,
/// then observe the expected tree change before posting the next**. Never a
/// sequence. `DiscordDeletionSurface` already has exactly one posting rung per
/// observation, so the shape holds by construction; this ladder is what each of
/// those rungs waits on, and its first step is above the 60 ms empirical floor.
pub const POSTED_KEY_OBSERVE_MS: [u64; 4] = [60, 120, 240, 480];

/// Total wall clock one posted key may be waited on. Bounded, like everything
/// else that touches Discord's UI thread.
pub fn posted_key_observe_budget_ms() -> u64 {
    POSTED_KEY_OBSERVE_MS.iter().sum()
}

/// Discord's own labels this workflow requires. Nothing is activated unless the
/// focused item's accessible name is byte-exactly one of these.
///
/// These are Discord's UI strings, not OSL's, so they are a compatibility
/// surface: a Discord that renames them makes every row `Unsupported`, which is
/// the correct fail-closed answer and never a silent no-op reported as success.
pub const DELETE_MENU_ITEM_LABEL: &str = "Delete Message";
pub const CONFIRM_DIALOG_LABEL: &str = "Delete Message";
pub const CONFIRM_BUTTON_LABEL: &str = "Delete";

/// The six activity states from `osl-gui-final-plan.md:494`. There is no seventh
/// and no state that means "probably deleted".
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityState {
    Scheduled,
    Running,
    Verified,
    Failed,
    Unsupported,
    Held,
}

impl ActivityState {
    /// The state's own name, exactly as the spec writes it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Scheduled => "Scheduled",
            Self::Running => "Running",
            Self::Verified => "Verified",
            Self::Failed => "Failed",
            Self::Unsupported => "Unsupported",
            Self::Held => "Held",
        }
    }
}

/// One row's shape: the only identity MSAA leaves available.
///
/// Chromium publishes no `runtime_id` over MSAA (measured: UI Automation sees
/// zero transcript nodes), so a row cannot be identified the way a UI Automation
/// element can. It also must NOT be identified by a rectangle recorded earlier:
/// the transcript scrolls and virtualizes, and comparing a live element against
/// a stale rect is the exact defect class that produced five separate failures
/// in this file's history. Shape plus ordinal plus text length is what is left,
/// and it is deliberately coarse: coarseness can only cost a false negative,
/// because the surface additionally proves the row by its own text before acting
/// and by a counted census afterwards.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowShape {
    pub height_px: i32,
    pub children: u16,
}

/// One candidate row from a scan. Owned rows only ever reach a plan.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannedRow {
    /// Position in the filtered row sequence (date separators removed) at scan
    /// time. Advisory: it is re-proven, never trusted across a deletion.
    pub scan_ordinal: usize,
    /// This row's position among rows of the same shape, in row order.
    pub shape_ordinal: usize,
    pub shape: RowShape,
    /// UTF-8 length of the row's own visible line. A length, never the text.
    pub text_len: usize,
    /// Whether the scan believes the operator wrote this row.
    pub authored_by_operator: bool,
}

/// Whether one bounded transcript walk read the whole filtered child list.
///
/// An absence proof is only sound over a COMPLETE walk. A walk that hit its row
/// cap, node ceiling or deadline stopped early, so "the row is not in what I
/// read" says nothing about whether it is still there.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WalkCompleteness {
    Complete,
    Truncated,
}

/// Whether the surface re-proved it is still reading the same conversation in
/// the same window generation, owned by the trusted process.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SurfaceIdentity {
    Reproven,
    Changed,
}

/// One scan of the transcript, before any plan exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletionScan {
    /// Domain-separated hash of the OSL scope binding. Opaque; never a name.
    pub scope_binding_hash: String,
    /// The window generation the scan was taken under.
    pub generation: u64,
    /// Message rows the walk considered, owned or not.
    pub rows_seen: usize,
    /// Rows the walk could not read at all.
    pub rows_unreadable: usize,
    pub walk: WalkCompleteness,
    /// Rows the operator wrote, in transcript order. Nothing else is a candidate.
    pub candidates: Vec<ScannedRow>,
}

impl DeletionScan {
    /// Every candidate is an owned row. Enforced here as well as at plan time so
    /// a surface bug cannot smuggle another person's row into a preview.
    fn candidates_are_all_owned(&self) -> bool {
        self.candidates.iter().all(|row| row.authored_by_operator)
    }
}

/// Why a plan was refused. Fixed reasons, all fail-closed.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PlanRefusal {
    /// Guided cleanup is Pro (`osl-gui-final-plan.md:506-508`).
    ProRequired,
    /// The scan did not read the whole transcript, so a preview taken from it
    /// could not honestly claim to show everything that would be deleted.
    ScanTruncated,
    /// The scan was taken against a different conversation or window generation.
    ScopeChanged,
    /// Nothing was selected.
    NothingSelected,
    /// More rows than one previewable plan may carry.
    TooManyRows,
    /// A selected row is not in this scan's candidate list.
    RowNotInScan,
    /// A selected row is not the operator's own message. Never deletable.
    ForeignRow,
    /// The same row was selected twice.
    DuplicateRow,
    /// The confirmation echoed a digest that does not match this plan.
    ConfirmationStale,
}

impl PlanRefusal {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::ProRequired => "guided_deletion_requires_pro",
            Self::ScanTruncated => "scan_did_not_read_the_whole_transcript",
            Self::ScopeChanged => "conversation_or_window_changed",
            Self::NothingSelected => "nothing_selected",
            Self::TooManyRows => "too_many_rows_for_one_plan",
            Self::RowNotInScan => "row_is_not_in_this_scan",
            Self::ForeignRow => "row_is_not_your_own_message",
            Self::DuplicateRow => "row_selected_twice",
            Self::ConfirmationStale => "confirmation_no_longer_matches_the_plan",
        }
    }
}

/// Exactly what will be attempted, for the operator to read before confirming.
///
/// This is the deletion analogue of the `Exact platform payload` panel at
/// `osl-gui-final-plan.md:423`: the mechanism and the exact target set are shown
/// before anything irreversible happens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletionPreview {
    pub scope_binding_hash: String,
    pub generation: u64,
    /// Digest over the whole plan. Any change to the scope, the generation, the
    /// row set or any row's shape/ordinal/length produces a different digest and
    /// invalidates the confirmation, exactly as `burn_contract` does.
    pub plan_digest: String,
    /// The rows that will be attempted, in the order they were scanned.
    pub rows: Vec<ScannedRow>,
    pub platform_steps: &'static [&'static str],
    /// Always true. Discord's deletion cannot be undone by OSL or by anyone.
    pub irreversible: bool,
    /// The single guarantee this action can establish.
    pub guarantee: &'static str,
    /// This action never expires OSL content and never removes a local copy.
    /// Those are the other two guarantees and they belong to other actions.
    pub expires_osl_content: bool,
    pub removes_local_copies: bool,
}

/// A plan whose exact digest the operator confirmed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmedPlan {
    preview: DeletionPreview,
}

impl ConfirmedPlan {
    pub fn preview(&self) -> &DeletionPreview {
        &self.preview
    }

    /// Rows in the order they must be EXECUTED: bottom of the transcript first.
    ///
    /// Deleting a row shifts every later row's ordinal, so descending order is
    /// what keeps the not-yet-attempted rows' recorded ordinals meaningful. It
    /// is a hint either way -- every row is re-resolved and re-proven before it
    /// is touched -- but a hint that is wrong for every remaining row after the
    /// first deletion would turn each later row into an avoidable `Held`.
    pub fn execution_order(&self) -> Vec<ScannedRow> {
        let mut rows = self.preview.rows.clone();
        rows.sort_by(|left, right| right.scan_ordinal.cmp(&left.scan_ordinal));
        rows
    }
}

/// Build the preview for a selection of scanned rows.
///
/// `pro` is the entitlement answer from native code, never from the renderer.
pub fn build_preview(
    scan: &DeletionScan,
    selection: &[usize],
    pro: bool,
) -> Result<DeletionPreview, PlanRefusal> {
    if !pro {
        return Err(PlanRefusal::ProRequired);
    }
    if scan.walk != WalkCompleteness::Complete || !scan.candidates_are_all_owned() {
        return Err(PlanRefusal::ScanTruncated);
    }
    if selection.is_empty() {
        return Err(PlanRefusal::NothingSelected);
    }
    if selection.len() > MAX_ROWS_PER_PLAN {
        return Err(PlanRefusal::TooManyRows);
    }
    let mut rows = Vec::with_capacity(selection.len());
    for (index, ordinal) in selection.iter().enumerate() {
        if selection[..index].contains(ordinal) {
            return Err(PlanRefusal::DuplicateRow);
        }
        let row = scan
            .candidates
            .iter()
            .find(|candidate| candidate.scan_ordinal == *ordinal)
            .ok_or(PlanRefusal::RowNotInScan)?;
        if !row.authored_by_operator {
            return Err(PlanRefusal::ForeignRow);
        }
        rows.push(*row);
    }
    rows.sort_by_key(|row| row.scan_ordinal);
    Ok(DeletionPreview {
        scope_binding_hash: scan.scope_binding_hash.clone(),
        generation: scan.generation,
        plan_digest: plan_digest(&scan.scope_binding_hash, scan.generation, &rows),
        rows,
        platform_steps: PLATFORM_STEPS,
        irreversible: true,
        guarantee: "platform_removal",
        expires_osl_content: false,
        removes_local_copies: false,
    })
}

/// Turn a preview into an executable plan only when the operator echoed its
/// exact digest back, and only while the scope and generation still hold.
pub fn confirm_preview(
    preview: &DeletionPreview,
    echoed_digest: &str,
    current_scope_binding_hash: &str,
    current_generation: u64,
) -> Result<ConfirmedPlan, PlanRefusal> {
    if preview.scope_binding_hash != current_scope_binding_hash
        || preview.generation != current_generation
    {
        return Err(PlanRefusal::ScopeChanged);
    }
    let expected = plan_digest(
        &preview.scope_binding_hash,
        preview.generation,
        &preview.rows,
    );
    if expected != preview.plan_digest || echoed_digest != expected {
        return Err(PlanRefusal::ConfirmationStale);
    }
    Ok(ConfirmedPlan {
        preview: preview.clone(),
    })
}

/// Content-free digest over the exact plan.
///
/// Shapes, ordinals, lengths and counts only. No row text and no conversation
/// name is in scope here, so none can reach the digest.
fn plan_digest(scope_binding_hash: &str, generation: u64, rows: &[ScannedRow]) -> String {
    let mut value = format!("{scope_binding_hash}\u{1f}{generation}\u{1f}{}", rows.len());
    for row in rows {
        value.push('\u{1e}');
        value.push_str(&format!(
            "{}:{}:{}:{}:{}",
            row.scan_ordinal,
            row.shape_ordinal,
            row.shape.height_px,
            row.shape.children,
            row.text_len
        ));
    }
    stable_hash(GUIDED_DELETION_CONTRACT, &value)
}

/// What re-resolving one planned row found, immediately before touching it.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RowResolution {
    /// A row with the planned shape, ordinal and text length is there, in the
    /// trusted process's transcript list, and it is the operator's own.
    Resolved,
    /// No row matching the plan is there any more. Nothing was deleted by OSL.
    Gone,
    /// More than one row matches, so which one the plan meant is unknowable.
    Ambiguous,
    /// The row resolved into something OSL does not trust: the wrong owning
    /// process, the wrong role, or a list that is no longer the same
    /// conversation.
    Untrusted,
    /// The transcript could not be read within its bounds.
    Unreadable,
}

/// What opening one row's own menu produced.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum MenuObservation {
    /// Discord's menu is open and offers its own delete item.
    DeleteOffered,
    /// The menu is open and there is no delete item in it. Discord itself says
    /// this row cannot be deleted; that is `Unsupported`, not a failure.
    DeleteNotOffered,
    /// No menu was observed. Nothing was activated.
    NotObserved,
}

/// Whether the delete item is the item that currently has focus.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum MenuItemFocus {
    /// The focused item's accessible name is byte-exactly the delete label.
    OnDeleteItem,
    /// Focus never landed on it within the bounded step count.
    NotReached,
}

/// Whether Discord's own confirmation dialog is up, with its own delete button.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ConfirmObservation {
    Present,
    NotObserved,
}

/// A counted census of the transcript with respect to ONE target row.
///
/// Three integers and two enums. This is deliberately the entire verification
/// input: no text reaches this module, so no verification decision can depend on
/// content, and a receipt cannot leak one.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct RowCensus {
    pub walk: WalkCompleteness,
    pub identity: SurfaceIdentity,
    /// Message rows in the filtered sequence.
    pub total_rows: usize,
    /// Rows whose shape equals the target's.
    pub target_shape_rows: usize,
    /// Rows whose own visible line is byte-exactly the target's. Counted inside
    /// the surface, which holds that text transiently and never returns it.
    pub target_text_rows: usize,
}

impl RowCensus {
    /// Whether these counts could have come from one readable list at all.
    ///
    /// `target_shape_rows` and `target_text_rows` are both counted over the same
    /// readable rows `total_rows` counts, so neither can exceed it. A census that
    /// breaks this did not come from a transcript, and reasoning about it would be
    /// reasoning about nothing: without this rule a census of
    /// `{total: 0, shape: 1, text: 1}` followed by `{total: 0, shape: 0, text: 0}`
    /// satisfies every other test in `classify_removal` and reaches `Proven` while
    /// no row was ever read.
    pub const fn is_self_consistent(&self) -> bool {
        self.target_shape_rows <= self.total_rows && self.target_text_rows <= self.total_rows
    }

    /// Whether the target was the ONLY row in this census carrying its own text.
    ///
    /// Uniqueness is what lets a later sighting be attributed to the target
    /// rather than to a lookalike, and it is only meaningful over a walk that saw
    /// the whole list -- a partial read showing one match cannot rule out a
    /// second one it never reached.
    const fn target_is_unique(&self) -> bool {
        matches!(self.walk, WalkCompleteness::Complete) && self.target_text_rows == 1
    }
}

/// What comparing the census before and after the confirmed delete established.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RemovalVerdict {
    /// The target row is provably gone: exactly one row with its text and
    /// exactly one row with its shape left, nothing else was removed, over a
    /// complete walk of the same conversation.
    Proven,
    /// The row is still there.
    StillPresent,
    /// Something happened, but not something that proves this row went. Never
    /// reported as deleted.
    Ambiguous(&'static str),
    /// The transcript could not be read well enough to prove anything.
    Unreadable(&'static str),
}

/// Decide whether the target row is provably gone.
///
/// Fail-closed by construction. Every branch that is not "this row went, from a
/// readable transcript of the same conversation" refuses.
///
/// Three rules do the work, and each exists because a simpler version of it was
/// wrong:
///
/// **Presence is provable from a partial read; absence is not.** Finding the row
/// needs only that the read reached it, so a verify walk that stopped early can
/// still establish `StillPresent` -- provided the BASELINE was complete and
/// showed the row was unique, or the row found might be a lookalike the baseline
/// never reached. Concluding the row is GONE needs the whole list, so every
/// absence branch insists on it. Collapsing the two was honest but told the
/// operator "could not read the transcript" when the available truth was "your
/// message is still there".
///
/// **A unique target is proven by its own absence; a duplicated one is not.**
/// When exactly one row in a complete baseline carried the target's text and no
/// row carries it now, that row went, and nothing about what else arrived can
/// change it. When several rows shared the text, a drop of one says only that
/// *a* matching row went -- the attribution to the planned row rests on the
/// action, not on the count -- so the duplicate path additionally demands that
/// the transcript was otherwise still. Requiring the strict shape for BOTH cases
/// was the original rule and it was fragile in the ordinary case: nearly every
/// Discord row is the same height, so a single reply arriving mid-operation
/// inflated the shape count and turned a real deletion into `Ambiguous`.
///
/// **Additions can only raise a count, so only an over-large DROP is evidence.**
/// An incoming message during the operation is ordinary. It cannot manufacture a
/// removal, so it is tolerated everywhere; but a shape count falling by more than
/// the one row that was targeted means other rows went too, and additions cannot
/// mask that direction.
///
/// KNOWN LIMIT, deliberately not papered over: counts alone cannot fully separate
/// a deletion from a virtualization eviction. Discord drops scrolled-away rows out
/// of the accessible tree entirely, so a row absent from a complete walk may have
/// been evicted rather than deleted. The net-shrinkage and shape-churn guards
/// below catch the ordinary shapes of that -- a scroll almost always removes more
/// than one row -- but a perfectly balanced evict-and-append that touches exactly
/// one row of the target's shape is indistinguishable here. Closing it properly
/// needs an anchor: one neighbouring row's own count carried through the census,
/// so a shifted view can be seen directly. That is a change to the surface
/// contract and is not made silently as part of a verdict function.
pub fn classify_removal(before: RowCensus, after: RowCensus) -> RemovalVerdict {
    // 0. Counts that cannot come from one readable list describe nothing, in
    //    either direction. Checked first so no later rule has to assume it.
    if !before.is_self_consistent() || !after.is_self_consistent() {
        return RemovalVerdict::Unreadable("census_is_not_self_consistent");
    }
    // 1. A census of another conversation, or of a replaced window, is not
    //    evidence about this row -- again in either direction, so this precedes
    //    the presence shortcut as well as every absence branch.
    if before.identity != SurfaceIdentity::Reproven {
        return RemovalVerdict::Unreadable("baseline_surface_identity_changed");
    }
    if after.identity != SurfaceIdentity::Reproven {
        return RemovalVerdict::Unreadable("verify_surface_identity_changed");
    }
    // 2. Nothing can be concluded from a baseline that never held the target.
    if before.target_text_rows == 0 || before.target_shape_rows == 0 {
        return RemovalVerdict::Unreadable("baseline_did_not_contain_the_target");
    }
    // 3. More rows carrying the target's text than before is not a deletion by
    //    any reading. The SHAPE count is deliberately not tested here: additions
    //    raise it legitimately and constantly, because Discord rows are mostly
    //    one height.
    if after.target_text_rows > before.target_text_rows {
        return RemovalVerdict::Ambiguous("matching_rows_increased");
    }
    // 4. PRESENCE, from a possibly partial verify read. Sound only because the
    //    baseline was complete and held exactly one row with this text: any row
    //    still carrying it is therefore that row.
    if before.target_is_unique() && after.target_text_rows >= 1 {
        return RemovalVerdict::StillPresent;
    }
    // 5. Everything below is an ABSENCE argument and needs both walks whole.
    if before.walk != WalkCompleteness::Complete {
        return RemovalVerdict::Unreadable("baseline_walk_truncated");
    }
    if after.walk != WalkCompleteness::Complete {
        return RemovalVerdict::Unreadable("verify_walk_truncated");
    }
    if after.target_text_rows == before.target_text_rows {
        return RemovalVerdict::StillPresent;
    }
    // A shape count that fell by more than one means rows of the target's shape
    // went beyond the target itself, whichever path follows. Additions can only
    // push this count up, so an over-large drop is real.
    let shape_drop = before.target_shape_rows.saturating_sub(after.target_shape_rows);
    if shape_drop > 1 {
        return RemovalVerdict::Ambiguous("more_than_one_matching_row_disappeared");
    }
    if before.target_is_unique() {
        // The only row carrying this text is gone from a complete read. What
        // arrived meanwhile is irrelevant; only the list as a whole SHRINKING
        // still suggests the view moved rather than the row going.
        if after.total_rows.saturating_add(1) < before.total_rows {
            return RemovalVerdict::Ambiguous("another_row_disappeared_too");
        }
        return RemovalVerdict::Proven;
    }
    // Duplicated text. The count cannot say which matching row went, so the
    // attribution rests entirely on the action -- and that is only good enough
    // when the transcript was otherwise completely still.
    if before.target_text_rows.saturating_sub(after.target_text_rows) != 1 {
        return RemovalVerdict::Ambiguous("more_than_one_matching_row_disappeared");
    }
    if shape_drop == 0 {
        return RemovalVerdict::Ambiguous("the_row_that_went_did_not_match_the_target_shape");
    }
    if after.total_rows.saturating_add(1) != before.total_rows {
        return RemovalVerdict::Ambiguous("the_transcript_moved_around_a_duplicate_row");
    }
    RemovalVerdict::Proven
}

/// One row's outcome. `state` is one of the six, `stage` is a fixed label.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowOutcome {
    pub scan_ordinal: usize,
    pub text_len: usize,
    pub state: ActivityState,
    pub stage: &'static str,
    /// Whether OSL actually activated Discord's confirm button for this row.
    pub request_posted: bool,
    /// Whether a re-walk proved the row is gone. The ONLY thing that may make a
    /// row `Verified`.
    pub rewalk_proved_absent: bool,
}

impl RowOutcome {
    /// The label the UI may show for this row.
    ///
    /// `Sent request` is never `Deleted` (`osl-gui-final-plan.md:494`). Exactly
    /// one label in this function says the message is gone from Discord, and it
    /// is only reachable from `Verified`, which is only reachable from a proven
    /// re-walk.
    pub const fn display_label(&self) -> &'static str {
        match self.state {
            ActivityState::Verified => "Deleted from Discord",
            ActivityState::Running if self.request_posted => "Sent request",
            ActivityState::Running => "Running",
            ActivityState::Scheduled => "Scheduled",
            ActivityState::Failed if self.request_posted => "Sent request - not verified",
            ActivityState::Failed => "Failed",
            ActivityState::Unsupported => "Discord offers no delete for this message",
            ActivityState::Held => "Held - nothing was deleted",
        }
    }

    /// The one structural invariant of this whole module.
    pub const fn state_is_earned(&self) -> bool {
        match self.state {
            ActivityState::Verified => self.request_posted && self.rewalk_proved_absent,
            _ => !self.rewalk_proved_absent,
        }
    }

    const fn held(row: &ScannedRow, stage: &'static str) -> Self {
        Self {
            scan_ordinal: row.scan_ordinal,
            text_len: row.text_len,
            state: ActivityState::Held,
            stage,
            request_posted: false,
            rewalk_proved_absent: false,
        }
    }

    const fn unsupported(row: &ScannedRow, stage: &'static str) -> Self {
        Self {
            scan_ordinal: row.scan_ordinal,
            text_len: row.text_len,
            state: ActivityState::Unsupported,
            stage,
            request_posted: false,
            rewalk_proved_absent: false,
        }
    }

    const fn failed_after_request(row: &ScannedRow, stage: &'static str) -> Self {
        Self {
            scan_ordinal: row.scan_ordinal,
            text_len: row.text_len,
            state: ActivityState::Failed,
            stage,
            request_posted: true,
            rewalk_proved_absent: false,
        }
    }

    const fn verified(row: &ScannedRow) -> Self {
        Self {
            scan_ordinal: row.scan_ordinal,
            text_len: row.text_len,
            state: ActivityState::Verified,
            stage: "rewalk_proved_the_row_is_gone",
            request_posted: true,
            rewalk_proved_absent: true,
        }
    }
}

/// The bounded operations one row's guided deletion may ask of Discord.
///
/// Every method is a single bounded step that either OBSERVES a state or posts
/// one key. None of them may block indefinitely, none may run on OSL's UI
/// thread, and none may be called while a lock OSL's UI thread needs is held:
/// a cross-process MSAA call made under that lock is the structural deadlock
/// documented at `ROW_PROOF_OPT_IN_VARIABLE`, and it froze this app for
/// 19,207 ms once already.
pub trait DiscordDeletionSurface {
    /// Re-prove the planned row against the live tree by shape, ordinal, text
    /// length, role and owning process. NEVER against a rectangle recorded
    /// earlier.
    fn resolve_row(&mut self, row: &ScannedRow) -> RowResolution;
    /// Count the transcript with respect to this row, before anything is posted.
    fn census(&mut self, row: &ScannedRow) -> RowCensus;
    /// Take accessibility focus to the row and observe that it landed.
    ///
    /// This is what "hover the row" becomes when the pointer must never move:
    /// MSAA's own `accSelect(TAKEFOCUS)`, not a synthesized mouse move.
    fn focus_row(&mut self, row: &ScannedRow) -> bool;
    /// Post the context-menu key and observe what Discord opened.
    fn open_row_menu(&mut self, row: &ScannedRow) -> MenuObservation;
    /// Step focus through the open menu until the delete item has it.
    fn focus_delete_item(&mut self) -> MenuItemFocus;
    /// Activate whatever currently has focus. Only ever called immediately after
    /// [`Self::focus_delete_item`] answered `OnDeleteItem`.
    fn activate_focused_item(&mut self) -> bool;
    /// Observe Discord's own confirmation dialog and its own delete button.
    fn observe_confirmation(&mut self) -> ConfirmObservation;
    /// Focus that button and observe that it has focus.
    fn focus_confirm_button(&mut self) -> bool;
    /// Activate the confirm button. This is the irreversible step.
    fn activate_confirmation(&mut self) -> bool;
    /// Unwind any menu or dialog OSL opened, so a refusal leaves Discord as it
    /// was found. Best effort by design: it can never turn a refusal into a
    /// success.
    fn dismiss(&mut self);
}

/// Execute one confirmed plan, one row at a time, bottom-up.
pub fn execute_confirmed_plan<S: DiscordDeletionSurface>(
    plan: &ConfirmedPlan,
    surface: &mut S,
) -> GuidedDeletionReceipt {
    let mut outcomes = Vec::with_capacity(plan.preview.rows.len());
    for row in plan.execution_order() {
        outcomes.push(execute_row(&row, surface));
    }
    outcomes.sort_by_key(|outcome| outcome.scan_ordinal);
    GuidedDeletionReceipt::build(&plan.preview, outcomes)
}

/// One row's full ladder. Every rung is observation-gated: an unobserved rung
/// stops the row rather than posting the next key hopefully.
fn execute_row<S: DiscordDeletionSurface>(row: &ScannedRow, surface: &mut S) -> RowOutcome {
    if !row.authored_by_operator {
        return RowOutcome::unsupported(row, "row_is_not_your_own_message");
    }
    match surface.resolve_row(row) {
        RowResolution::Resolved => {}
        RowResolution::Gone => return RowOutcome::held(row, "row_was_already_gone"),
        RowResolution::Ambiguous => return RowOutcome::held(row, "row_could_not_be_told_apart"),
        RowResolution::Untrusted => return RowOutcome::held(row, "row_resolved_into_untrusted_tree"),
        RowResolution::Unreadable => return RowOutcome::held(row, "row_could_not_be_read"),
    }
    let before = surface.census(row);
    // Deliberately STRICTER than `classify_removal`. That function interprets what
    // already happened and may lean on a partial read for a positive sighting;
    // this one decides whether to start something irreversible, and a baseline
    // that did not see the whole list can neither establish that the row is
    // unique nor produce counts the later census is comparable against. Nothing
    // destructive begins without both.
    if !before.is_self_consistent() {
        return RowOutcome::held(row, "baseline_census_is_not_self_consistent");
    }
    if before.walk != WalkCompleteness::Complete || before.identity != SurfaceIdentity::Reproven {
        return RowOutcome::held(row, "baseline_transcript_could_not_be_read");
    }
    if before.target_text_rows == 0 {
        return RowOutcome::held(row, "baseline_did_not_contain_the_row");
    }
    if !surface.focus_row(row) {
        return RowOutcome::held(row, "row_focus_was_not_confirmed");
    }
    match surface.open_row_menu(row) {
        MenuObservation::DeleteOffered => {}
        MenuObservation::DeleteNotOffered => {
            surface.dismiss();
            return RowOutcome::unsupported(row, "discord_offers_no_delete_for_this_row");
        }
        MenuObservation::NotObserved => {
            surface.dismiss();
            return RowOutcome::held(row, "row_menu_was_not_observed");
        }
    }
    if surface.focus_delete_item() != MenuItemFocus::OnDeleteItem {
        surface.dismiss();
        return RowOutcome::held(row, "delete_item_never_took_focus");
    }
    if !surface.activate_focused_item() {
        surface.dismiss();
        return RowOutcome::held(row, "delete_item_activation_was_refused");
    }
    if surface.observe_confirmation() != ConfirmObservation::Present {
        surface.dismiss();
        return RowOutcome::held(row, "confirmation_dialog_was_not_observed");
    }
    if !surface.focus_confirm_button() {
        surface.dismiss();
        return RowOutcome::held(row, "confirm_button_focus_was_not_confirmed");
    }
    if !surface.activate_confirmation() {
        surface.dismiss();
        return RowOutcome::held(row, "confirmation_activation_was_refused");
    }
    // From here the platform request HAS been made. Nothing below may report
    // `Held`: the row's fate is now Discord's, and every non-proof is `Failed`
    // with the reason, never a silent success and never `Deleted`.
    let after = surface.census(row);
    match classify_removal(before, after) {
        RemovalVerdict::Proven => RowOutcome::verified(row),
        RemovalVerdict::StillPresent => {
            RowOutcome::failed_after_request(row, "row_is_still_in_the_transcript")
        }
        RemovalVerdict::Ambiguous(stage) | RemovalVerdict::Unreadable(stage) => {
            RowOutcome::failed_after_request(row, stage)
        }
    }
}

/// The receipt for one executed plan.
///
/// The three guarantees at `osl-gui-final-plan.md:496-500` are three separate
/// fields here, and only the first one can ever be true: this action does not
/// expire OSL content, does not delete a local copy, and is not a burn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuidedDeletionReceipt {
    pub contract: &'static str,
    pub plan_digest: String,
    pub scope_binding_hash: String,
    pub generation: u64,
    pub rows_requested: usize,
    pub rows_verified: usize,
    pub rows_failed: usize,
    pub rows_unsupported: usize,
    pub rows_held: usize,
    /// Guarantee 1. True only when every requested row was proven gone.
    pub platform_removal_verified: bool,
    /// Guarantee 2. This action never expires OSL content.
    pub osl_content_expiry_applied: bool,
    /// Guarantee 3. This action never removes a local copy.
    pub local_removal_applied: bool,
    /// This action is never a burn. `burn-contract.md:14` stays true.
    pub burn_performed: bool,
    pub rows: Vec<RowOutcome>,
}

impl GuidedDeletionReceipt {
    fn build(preview: &DeletionPreview, rows: Vec<RowOutcome>) -> Self {
        let count = |state: ActivityState| rows.iter().filter(|row| row.state == state).count();
        let verified = count(ActivityState::Verified);
        Self {
            contract: GUIDED_DELETION_CONTRACT,
            plan_digest: preview.plan_digest.clone(),
            scope_binding_hash: preview.scope_binding_hash.clone(),
            generation: preview.generation,
            rows_requested: preview.rows.len(),
            rows_verified: verified,
            rows_failed: count(ActivityState::Failed),
            rows_unsupported: count(ActivityState::Unsupported),
            rows_held: count(ActivityState::Held),
            platform_removal_verified: !preview.rows.is_empty() && verified == preview.rows.len(),
            osl_content_expiry_applied: false,
            local_removal_applied: false,
            burn_performed: false,
            rows,
        }
    }

    /// Whether every claim in this receipt is backed by the evidence beside it.
    /// The renderer's parser enforces the same rule; this is the native half.
    pub fn is_self_consistent(&self) -> bool {
        self.rows.len() == self.rows_requested
            && self.rows.iter().all(RowOutcome::state_is_earned)
            && self.rows_verified == self
                .rows
                .iter()
                .filter(|row| row.state == ActivityState::Verified)
                .count()
            && self.platform_removal_verified
                == (self.rows_requested > 0 && self.rows_verified == self.rows_requested)
            && !self.osl_content_expiry_applied
            && !self.local_removal_applied
            && !self.burn_performed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(height: i32, children: u16) -> RowShape {
        RowShape {
            height_px: height,
            children,
        }
    }

    fn owned_row(ordinal: usize) -> ScannedRow {
        ScannedRow {
            scan_ordinal: ordinal,
            shape_ordinal: 0,
            shape: shape(44, 1),
            text_len: 12,
            authored_by_operator: true,
        }
    }

    fn scan(candidates: Vec<ScannedRow>) -> DeletionScan {
        DeletionScan {
            scope_binding_hash: "a".repeat(64),
            generation: 7,
            rows_seen: candidates.len(),
            rows_unreadable: 0,
            walk: WalkCompleteness::Complete,
            candidates,
        }
    }

    fn complete_census(total: usize, shape_rows: usize, text_rows: usize) -> RowCensus {
        RowCensus {
            walk: WalkCompleteness::Complete,
            identity: SurfaceIdentity::Reproven,
            total_rows: total,
            target_shape_rows: shape_rows,
            target_text_rows: text_rows,
        }
    }

    /// A scripted surface. Every rung defaults to the cooperative answer so a
    /// test only has to state the one thing it is about.
    struct FakeSurface {
        resolution: RowResolution,
        censuses: Vec<RowCensus>,
        census_calls: usize,
        focus_row: bool,
        menu: MenuObservation,
        item_focus: MenuItemFocus,
        activate_item: bool,
        confirmation: ConfirmObservation,
        focus_confirm: bool,
        activate_confirm: bool,
        dismissals: usize,
        order: Vec<usize>,
    }

    impl FakeSurface {
        fn happy() -> Self {
            Self {
                resolution: RowResolution::Resolved,
                censuses: vec![complete_census(9, 1, 1), complete_census(8, 0, 0)],
                census_calls: 0,
                focus_row: true,
                menu: MenuObservation::DeleteOffered,
                item_focus: MenuItemFocus::OnDeleteItem,
                activate_item: true,
                confirmation: ConfirmObservation::Present,
                focus_confirm: true,
                activate_confirm: true,
                dismissals: 0,
                order: Vec::new(),
            }
        }
    }

    impl DiscordDeletionSurface for FakeSurface {
        fn resolve_row(&mut self, row: &ScannedRow) -> RowResolution {
            self.order.push(row.scan_ordinal);
            self.resolution
        }
        fn census(&mut self, _row: &ScannedRow) -> RowCensus {
            let index = self.census_calls.min(self.censuses.len().saturating_sub(1));
            self.census_calls = self.census_calls.saturating_add(1);
            self.censuses[index]
        }
        fn focus_row(&mut self, _row: &ScannedRow) -> bool {
            self.focus_row
        }
        fn open_row_menu(&mut self, _row: &ScannedRow) -> MenuObservation {
            self.menu
        }
        fn focus_delete_item(&mut self) -> MenuItemFocus {
            self.item_focus
        }
        fn activate_focused_item(&mut self) -> bool {
            self.activate_item
        }
        fn observe_confirmation(&mut self) -> ConfirmObservation {
            self.confirmation
        }
        fn focus_confirm_button(&mut self) -> bool {
            self.focus_confirm
        }
        fn activate_confirmation(&mut self) -> bool {
            self.activate_confirm
        }
        fn dismiss(&mut self) {
            self.dismissals = self.dismissals.saturating_add(1);
        }
    }

    fn confirmed(rows: Vec<ScannedRow>) -> ConfirmedPlan {
        let scan = scan(rows.clone());
        let selection: Vec<usize> = rows.iter().map(|row| row.scan_ordinal).collect();
        let preview = build_preview(&scan, &selection, true).expect("preview");
        confirm_preview(
            &preview,
            &preview.plan_digest,
            &scan.scope_binding_hash,
            scan.generation,
        )
        .expect("confirmation")
    }

    #[test]
    fn guided_deletion_is_pro_only_and_says_so_before_anything_else() {
        let scan = scan(vec![owned_row(3)]);
        assert_eq!(
            build_preview(&scan, &[3], false),
            Err(PlanRefusal::ProRequired)
        );
        assert_eq!(PlanRefusal::ProRequired.reason(), "guided_deletion_requires_pro");
    }

    #[test]
    fn a_truncated_scan_can_never_be_previewed_as_the_whole_picture() {
        let mut truncated = scan(vec![owned_row(1)]);
        truncated.walk = WalkCompleteness::Truncated;
        assert_eq!(
            build_preview(&truncated, &[1], true),
            Err(PlanRefusal::ScanTruncated)
        );
    }

    #[test]
    fn only_the_operators_own_rows_can_enter_a_plan() {
        let mut foreign = owned_row(2);
        foreign.authored_by_operator = false;
        let scan = scan(vec![owned_row(1), foreign]);
        // The scan itself is refused, because a candidate list containing a row
        // OSL does not own is a surface bug, not a selection mistake.
        assert_eq!(
            build_preview(&scan, &[1], true),
            Err(PlanRefusal::ScanTruncated)
        );
        let clean = super::DeletionScan {
            candidates: vec![owned_row(1)],
            ..scan
        };
        assert_eq!(
            build_preview(&clean, &[2], true),
            Err(PlanRefusal::RowNotInScan)
        );
    }

    #[test]
    fn a_plan_is_bounded_deduplicated_and_never_empty() {
        let rows: Vec<ScannedRow> = (0..MAX_ROWS_PER_PLAN + 1).map(owned_row).collect();
        let scan = scan(rows);
        let selection: Vec<usize> = (0..MAX_ROWS_PER_PLAN + 1).collect();
        assert_eq!(
            build_preview(&scan, &selection, true),
            Err(PlanRefusal::TooManyRows)
        );
        assert_eq!(
            build_preview(&scan, &[], true),
            Err(PlanRefusal::NothingSelected)
        );
        assert_eq!(
            build_preview(&scan, &[1, 1], true),
            Err(PlanRefusal::DuplicateRow)
        );
    }

    #[test]
    fn the_preview_shows_the_mechanism_and_claims_exactly_one_guarantee() {
        let scan = scan(vec![owned_row(4)]);
        let preview = build_preview(&scan, &[4], true).expect("preview");
        assert!(preview.irreversible);
        assert_eq!(preview.guarantee, "platform_removal");
        assert!(!preview.expires_osl_content);
        assert!(!preview.removes_local_copies);
        assert_eq!(preview.platform_steps, PLATFORM_STEPS);
        assert!(preview
            .platform_steps
            .iter()
            .any(|step| step.contains("re_read_the_transcript")));
        assert_eq!(preview.plan_digest.len(), 64);
    }

    #[test]
    fn changing_anything_about_the_plan_invalidates_the_confirmation() {
        let scan = scan(vec![owned_row(1), owned_row(2)]);
        let preview = build_preview(&scan, &[1], true).expect("preview");
        let wider = build_preview(&scan, &[1, 2], true).expect("preview");
        assert_ne!(preview.plan_digest, wider.plan_digest);
        assert_eq!(
            confirm_preview(
                &preview,
                &wider.plan_digest,
                &scan.scope_binding_hash,
                scan.generation
            ),
            Err(PlanRefusal::ConfirmationStale)
        );
        assert_eq!(
            confirm_preview(
                &preview,
                &preview.plan_digest,
                &scan.scope_binding_hash,
                scan.generation + 1
            ),
            Err(PlanRefusal::ScopeChanged)
        );
        assert_eq!(
            confirm_preview(
                &preview,
                &preview.plan_digest,
                &"b".repeat(64),
                scan.generation
            ),
            Err(PlanRefusal::ScopeChanged)
        );
        assert!(confirm_preview(
            &preview,
            &preview.plan_digest,
            &scan.scope_binding_hash,
            scan.generation
        )
        .is_ok());
    }

    #[test]
    fn a_tampered_preview_digest_cannot_be_confirmed() {
        let scan = scan(vec![owned_row(1)]);
        let mut preview = build_preview(&scan, &[1], true).expect("preview");
        preview.plan_digest = "c".repeat(64);
        assert_eq!(
            confirm_preview(
                &preview,
                &preview.plan_digest.clone(),
                &scan.scope_binding_hash,
                scan.generation
            ),
            Err(PlanRefusal::ConfirmationStale)
        );
    }

    #[test]
    fn the_plan_digest_covers_the_scope_the_generation_and_every_row_field() {
        let base = scan(vec![owned_row(1)]);
        let baseline = build_preview(&base, &[1], true).expect("preview").plan_digest;
        let mut wider_shape = owned_row(1);
        wider_shape.shape = shape(45, 1);
        let mut longer = owned_row(1);
        longer.text_len = 13;
        let mut later = owned_row(1);
        later.shape_ordinal = 2;
        for variant in [wider_shape, longer, later] {
            let scan = scan(vec![variant]);
            assert_ne!(
                baseline,
                build_preview(&scan, &[1], true).expect("preview").plan_digest
            );
        }
        let other_generation = DeletionScan {
            generation: 8,
            ..scan(vec![owned_row(1)])
        };
        assert_ne!(
            baseline,
            build_preview(&other_generation, &[1], true)
                .expect("preview")
                .plan_digest
        );
    }

    #[test]
    fn rows_are_executed_from_the_bottom_of_the_transcript_upward() {
        let plan = confirmed(vec![owned_row(1), owned_row(5), owned_row(3)]);
        assert_eq!(
            plan.execution_order()
                .iter()
                .map(|row| row.scan_ordinal)
                .collect::<Vec<_>>(),
            vec![5, 3, 1]
        );
        let mut surface = FakeSurface::happy();
        let receipt = execute_confirmed_plan(&plan, &mut surface);
        assert_eq!(surface.order, vec![5, 3, 1]);
        // The receipt is reported in transcript order regardless.
        assert_eq!(
            receipt
                .rows
                .iter()
                .map(|row| row.scan_ordinal)
                .collect::<Vec<_>>(),
            vec![1, 3, 5]
        );
    }

    #[test]
    fn a_row_is_verified_only_when_the_rewalk_proves_it_is_gone() {
        let plan = confirmed(vec![owned_row(2)]);
        let mut surface = FakeSurface::happy();
        let receipt = execute_confirmed_plan(&plan, &mut surface);
        assert_eq!(receipt.rows[0].state, ActivityState::Verified);
        assert!(receipt.rows[0].rewalk_proved_absent);
        assert!(receipt.platform_removal_verified);
        assert!(receipt.is_self_consistent());
        assert_eq!(receipt.rows[0].display_label(), "Deleted from Discord");
    }

    #[test]
    fn a_posted_request_whose_row_is_still_there_is_failed_and_never_deleted() {
        let plan = confirmed(vec![owned_row(2)]);
        let mut surface = FakeSurface::happy();
        surface.censuses = vec![complete_census(9, 1, 1), complete_census(9, 1, 1)];
        let receipt = execute_confirmed_plan(&plan, &mut surface);
        let row = receipt.rows[0];
        assert_eq!(row.state, ActivityState::Failed);
        assert!(row.request_posted);
        assert!(!row.rewalk_proved_absent);
        assert_eq!(row.stage, "row_is_still_in_the_transcript");
        assert_eq!(row.display_label(), "Sent request - not verified");
        assert!(!receipt.platform_removal_verified);
        assert!(receipt.is_self_consistent());
    }

    #[test]
    fn a_verification_walk_that_could_not_finish_never_proves_absence() {
        let plan = confirmed(vec![owned_row(2)]);
        let mut surface = FakeSurface::happy();
        surface.censuses = vec![
            complete_census(9, 1, 1),
            RowCensus {
                walk: WalkCompleteness::Truncated,
                ..complete_census(8, 0, 0)
            },
        ];
        let receipt = execute_confirmed_plan(&plan, &mut surface);
        assert_eq!(receipt.rows[0].state, ActivityState::Failed);
        assert_eq!(receipt.rows[0].stage, "verify_walk_truncated");
        assert!(!receipt.rows[0].rewalk_proved_absent);
    }

    #[test]
    fn a_conversation_change_during_verification_never_proves_absence() {
        let plan = confirmed(vec![owned_row(2)]);
        let mut surface = FakeSurface::happy();
        surface.censuses = vec![
            complete_census(9, 1, 1),
            RowCensus {
                identity: SurfaceIdentity::Changed,
                ..complete_census(8, 0, 0)
            },
        ];
        let receipt = execute_confirmed_plan(&plan, &mut surface);
        assert_eq!(receipt.rows[0].state, ActivityState::Failed);
        assert_eq!(receipt.rows[0].stage, "verify_surface_identity_changed");
    }

    #[test]
    fn every_unobserved_rung_stops_the_row_before_anything_is_posted() {
        let cases: [(fn(&mut FakeSurface), ActivityState, &'static str); 7] = [
            (
                |surface| surface.focus_row = false,
                ActivityState::Held,
                "row_focus_was_not_confirmed",
            ),
            (
                |surface| surface.menu = MenuObservation::NotObserved,
                ActivityState::Held,
                "row_menu_was_not_observed",
            ),
            (
                |surface| surface.menu = MenuObservation::DeleteNotOffered,
                ActivityState::Unsupported,
                "discord_offers_no_delete_for_this_row",
            ),
            (
                |surface| surface.item_focus = MenuItemFocus::NotReached,
                ActivityState::Held,
                "delete_item_never_took_focus",
            ),
            (
                |surface| surface.activate_item = false,
                ActivityState::Held,
                "delete_item_activation_was_refused",
            ),
            (
                |surface| surface.confirmation = ConfirmObservation::NotObserved,
                ActivityState::Held,
                "confirmation_dialog_was_not_observed",
            ),
            (
                |surface| surface.activate_confirm = false,
                ActivityState::Held,
                "confirmation_activation_was_refused",
            ),
        ];
        let plan = confirmed(vec![owned_row(1)]);
        for (break_it, state, stage) in cases {
            let mut surface = FakeSurface::happy();
            break_it(&mut surface);
            let receipt = execute_confirmed_plan(&plan, &mut surface);
            let row = receipt.rows[0];
            assert_eq!(row.state, state, "{stage}");
            assert_eq!(row.stage, stage);
            assert!(!row.request_posted, "{stage} must not post the request");
            assert!(!row.rewalk_proved_absent);
            assert!(surface.dismissals > 0 || stage == "row_focus_was_not_confirmed");
            assert!(!receipt.platform_removal_verified);
            assert!(receipt.is_self_consistent());
        }
    }

    #[test]
    fn a_row_that_cannot_be_told_apart_is_held_rather_than_guessed_at() {
        let plan = confirmed(vec![owned_row(1)]);
        for (resolution, stage) in [
            (RowResolution::Gone, "row_was_already_gone"),
            (RowResolution::Ambiguous, "row_could_not_be_told_apart"),
            (
                RowResolution::Untrusted,
                "row_resolved_into_untrusted_tree",
            ),
            (RowResolution::Unreadable, "row_could_not_be_read"),
        ] {
            let mut surface = FakeSurface::happy();
            surface.resolution = resolution;
            let receipt = execute_confirmed_plan(&plan, &mut surface);
            assert_eq!(receipt.rows[0].state, ActivityState::Held);
            assert_eq!(receipt.rows[0].stage, stage);
        }
    }

    #[test]
    fn a_row_that_is_already_absent_is_held_and_never_counted_as_deleted() {
        // "It went away on its own" is not "OSL deleted it". The distinction is
        // the whole reason `Held` exists.
        let plan = confirmed(vec![owned_row(1)]);
        let mut surface = FakeSurface::happy();
        surface.censuses = vec![complete_census(9, 0, 0)];
        let receipt = execute_confirmed_plan(&plan, &mut surface);
        assert_eq!(receipt.rows[0].state, ActivityState::Held);
        assert_eq!(receipt.rows[0].stage, "baseline_did_not_contain_the_row");
        assert_eq!(receipt.rows_verified, 0);
    }

    #[test]
    fn scroll_eviction_is_never_mistaken_for_a_deletion() {
        // One row gone AND another gone: exactly the shape of a scroll.
        assert_eq!(
            classify_removal(complete_census(20, 3, 1), complete_census(18, 2, 0)),
            RemovalVerdict::Ambiguous("another_row_disappeared_too")
        );
        // Two same-text rows gone at once: which one was the target is unknowable.
        assert_eq!(
            classify_removal(complete_census(20, 4, 3), complete_census(18, 2, 1)),
            RemovalVerdict::Ambiguous("more_than_one_matching_row_disappeared")
        );
    }

    #[test]
    fn an_incoming_message_during_the_operation_still_allows_a_proof() {
        // The target's text count dropped by exactly one and the total went up:
        // somebody replied while OSL was deleting. That does not weaken the
        // absence proof for the target row.
        assert_eq!(
            classify_removal(complete_census(20, 3, 1), complete_census(21, 2, 0)),
            RemovalVerdict::Proven
        );
    }

    #[test]
    fn a_baseline_that_never_contained_the_target_proves_nothing() {
        assert_eq!(
            classify_removal(complete_census(20, 0, 0), complete_census(19, 0, 0)),
            RemovalVerdict::Unreadable("baseline_did_not_contain_the_target")
        );
        assert_eq!(
            classify_removal(
                RowCensus {
                    walk: WalkCompleteness::Truncated,
                    ..complete_census(20, 1, 1)
                },
                complete_census(19, 0, 0)
            ),
            RemovalVerdict::Unreadable("baseline_walk_truncated")
        );
        assert_eq!(
            classify_removal(
                RowCensus {
                    identity: SurfaceIdentity::Changed,
                    ..complete_census(20, 1, 1)
                },
                complete_census(19, 0, 0)
            ),
            RemovalVerdict::Unreadable("baseline_surface_identity_changed")
        );
    }

    #[test]
    fn matching_rows_appearing_rather_than_going_is_ambiguous() {
        assert_eq!(
            classify_removal(complete_census(20, 1, 1), complete_census(21, 2, 2)),
            RemovalVerdict::Ambiguous("matching_rows_increased")
        );
    }

    #[test]
    fn sent_request_is_never_displayed_as_deleted() {
        // Exhaustive over the six states in both request-posted positions: the
        // ONLY label that says the message is gone belongs to `Verified`.
        for state in [
            ActivityState::Scheduled,
            ActivityState::Running,
            ActivityState::Verified,
            ActivityState::Failed,
            ActivityState::Unsupported,
            ActivityState::Held,
        ] {
            for request_posted in [false, true] {
                let outcome = RowOutcome {
                    scan_ordinal: 0,
                    text_len: 1,
                    state,
                    stage: "stage",
                    request_posted,
                    rewalk_proved_absent: state == ActivityState::Verified,
                };
                let says_deleted = outcome.display_label().contains("Deleted");
                assert_eq!(
                    says_deleted,
                    state == ActivityState::Verified,
                    "{} / posted={request_posted} said {:?}",
                    state.name(),
                    outcome.display_label()
                );
                if request_posted && state != ActivityState::Verified {
                    assert!(!outcome.display_label().contains("Deleted"));
                }
            }
        }
    }

    #[test]
    fn no_state_but_verified_may_claim_a_proven_rewalk() {
        for state in [
            ActivityState::Scheduled,
            ActivityState::Running,
            ActivityState::Failed,
            ActivityState::Unsupported,
            ActivityState::Held,
        ] {
            let lying = RowOutcome {
                scan_ordinal: 0,
                text_len: 1,
                state,
                stage: "stage",
                request_posted: true,
                rewalk_proved_absent: true,
            };
            assert!(!lying.state_is_earned(), "{}", state.name());
        }
        let unproven_verified = RowOutcome {
            scan_ordinal: 0,
            text_len: 1,
            state: ActivityState::Verified,
            stage: "stage",
            request_posted: true,
            rewalk_proved_absent: false,
        };
        assert!(!unproven_verified.state_is_earned());
        let unposted_verified = RowOutcome {
            rewalk_proved_absent: true,
            request_posted: false,
            ..unproven_verified
        };
        assert!(!unposted_verified.state_is_earned());
    }

    #[test]
    fn the_receipt_keeps_the_three_guarantees_apart_and_is_never_a_burn() {
        let plan = confirmed(vec![owned_row(1)]);
        let mut surface = FakeSurface::happy();
        let receipt = execute_confirmed_plan(&plan, &mut surface);
        assert!(receipt.platform_removal_verified);
        assert!(!receipt.osl_content_expiry_applied);
        assert!(!receipt.local_removal_applied);
        assert!(!receipt.burn_performed);
        assert_eq!(receipt.contract, "discord_guided_deletion_v1");
    }

    #[test]
    fn a_partly_successful_plan_never_claims_platform_removal() {
        let plan = confirmed(vec![owned_row(1), owned_row(2)]);
        let mut surface = FakeSurface::happy();
        // Row 2 (executed first, bottom-up) is proven; row 1 then finds the
        // transcript unchanged and fails.
        surface.censuses = vec![
            complete_census(9, 1, 1),
            complete_census(8, 0, 0),
            complete_census(8, 1, 1),
            complete_census(8, 1, 1),
        ];
        let receipt = execute_confirmed_plan(&plan, &mut surface);
        assert_eq!(receipt.rows_requested, 2);
        assert_eq!(receipt.rows_verified, 1);
        assert_eq!(receipt.rows_failed, 1);
        assert!(!receipt.platform_removal_verified);
        assert!(receipt.is_self_consistent());
    }

    #[test]
    fn the_receipt_carries_lengths_and_counts_only() {
        let plan = confirmed(vec![owned_row(1)]);
        let mut surface = FakeSurface::happy();
        let receipt = execute_confirmed_plan(&plan, &mut surface);
        let rendered = format!("{receipt:?}");
        // Every field is a count, a length, a fixed label or an opaque digest.
        assert!(rendered.contains("text_len: 12"));
        assert!(!rendered.contains("Messages in "));
        assert_eq!(receipt.scope_binding_hash.len(), 64);
        assert_eq!(receipt.plan_digest.len(), 64);
    }

    #[test]
    fn one_posted_key_is_waited_on_above_the_measured_floor_and_stays_bounded() {
        // 60 ms per 4 posted units is the measured floor at which Chromium stops
        // dropping them; a single key followed by the first wait is well inside
        // it. The ladder must also be finite and monotonic -- a wait that shrinks
        // is a wait that stops meaning anything.
        assert!(POSTED_KEY_OBSERVE_MS[0] >= 60);
        assert!(POSTED_KEY_OBSERVE_MS.windows(2).all(|pair| pair[1] > pair[0]));
        assert_eq!(posted_key_observe_budget_ms(), 900);
        // And it is one key per observation by construction: the surface exposes
        // exactly one posting rung between each pair of observations, so no code
        // path can post a sequence blind.
        assert_eq!(PLATFORM_STEPS.len(), 5);
    }

    #[test]
    fn discords_own_labels_are_the_only_thing_activation_accepts() {
        assert_eq!(DELETE_MENU_ITEM_LABEL, "Delete Message");
        assert_eq!(CONFIRM_DIALOG_LABEL, "Delete Message");
        assert_eq!(CONFIRM_BUTTON_LABEL, "Delete");
    }

    // -----------------------------------------------------------------------
    // Adversarial cases for the verification rule.
    //
    // `classify_removal` is the only thing standing between "OSL posted a
    // request" and "OSL says your message is gone", so it is worth attacking
    // rather than merely exercising. Each test below is one way a transcript can
    // lie, and the required answer is never `Proven`.
    // -----------------------------------------------------------------------

    fn census(
        walk: WalkCompleteness,
        total: usize,
        shape_rows: usize,
        text_rows: usize,
    ) -> RowCensus {
        RowCensus {
            walk,
            identity: SurfaceIdentity::Reproven,
            total_rows: total,
            target_shape_rows: shape_rows,
            target_text_rows: text_rows,
        }
    }

    #[test]
    fn a_census_that_could_not_have_come_from_a_list_proves_nothing() {
        // The hole this closes: with no self-consistency rule, a surface that
        // read NOTHING but reported the target as present and then absent walks
        // straight through every other test to `Proven`.
        let phantom_before = census(WalkCompleteness::Complete, 0, 1, 1);
        let phantom_after = census(WalkCompleteness::Complete, 0, 0, 0);
        assert!(!phantom_before.is_self_consistent());
        assert_eq!(
            classify_removal(phantom_before, phantom_after),
            RemovalVerdict::Unreadable("census_is_not_self_consistent")
        );
        // Either side being malformed is enough.
        assert_eq!(
            classify_removal(
                census(WalkCompleteness::Complete, 9, 3, 1),
                census(WalkCompleteness::Complete, 2, 9, 0)
            ),
            RemovalVerdict::Unreadable("census_is_not_self_consistent")
        );
        // And shape counts are bounded by the same total as text counts.
        assert!(!census(WalkCompleteness::Complete, 2, 3, 0).is_self_consistent());
        assert!(census(WalkCompleteness::Complete, 3, 3, 3).is_self_consistent());
    }

    #[test]
    fn a_changed_conversation_is_refused_before_any_count_is_believed() {
        // Identity outranks the counts in BOTH directions: a census of another
        // conversation is not evidence that the row went, and not evidence that
        // it stayed either. Ordered ahead of the presence shortcut for exactly
        // that reason.
        let elsewhere = RowCensus {
            identity: SurfaceIdentity::Changed,
            ..census(WalkCompleteness::Complete, 9, 1, 1)
        };
        assert_eq!(
            classify_removal(census(WalkCompleteness::Complete, 9, 1, 1), elsewhere),
            RemovalVerdict::Unreadable("verify_surface_identity_changed")
        );
        // Even when those counts would otherwise have said "still present".
        assert_eq!(
            classify_removal(
                RowCensus {
                    identity: SurfaceIdentity::Changed,
                    ..census(WalkCompleteness::Complete, 9, 1, 1)
                },
                census(WalkCompleteness::Complete, 9, 1, 1)
            ),
            RemovalVerdict::Unreadable("baseline_surface_identity_changed")
        );
    }

    #[test]
    fn a_partial_verify_read_can_still_prove_the_row_is_present() {
        // Presence needs only that the read reached the row. The baseline was
        // complete and held exactly one row with this text, so a row still
        // carrying it IS that row -- no completeness required of the second walk.
        assert_eq!(
            classify_removal(
                census(WalkCompleteness::Complete, 20, 3, 1),
                census(WalkCompleteness::Truncated, 12, 2, 1)
            ),
            RemovalVerdict::StillPresent
        );
        // The honest difference this buys: before, the operator was told the
        // transcript could not be read; now they are told their message is still
        // there. Both are `Failed`, but only one of them is true.
        assert_ne!(
            classify_removal(
                census(WalkCompleteness::Complete, 20, 3, 1),
                census(WalkCompleteness::Truncated, 12, 2, 1)
            ),
            RemovalVerdict::Unreadable("verify_walk_truncated")
        );
    }

    #[test]
    fn a_partial_verify_read_can_never_prove_the_row_is_gone() {
        // The other half of the asymmetry. Absence from what was read says
        // nothing when the read stopped early, however unique the target was.
        assert_eq!(
            classify_removal(
                census(WalkCompleteness::Complete, 20, 3, 1),
                census(WalkCompleteness::Truncated, 12, 2, 0)
            ),
            RemovalVerdict::Unreadable("verify_walk_truncated")
        );
    }

    #[test]
    fn a_partial_baseline_cannot_establish_that_the_target_was_unique() {
        // The presence shortcut rests on uniqueness, and a partial baseline
        // showing one match cannot rule out a second it never reached. So a
        // sighting afterwards might be a lookalike, and the shortcut must not
        // fire.
        assert_eq!(
            classify_removal(
                census(WalkCompleteness::Truncated, 20, 3, 1),
                census(WalkCompleteness::Complete, 20, 3, 1)
            ),
            RemovalVerdict::Unreadable("baseline_walk_truncated")
        );
        assert!(!census(WalkCompleteness::Truncated, 20, 3, 1).target_is_unique());
        assert!(census(WalkCompleteness::Complete, 20, 3, 1).target_is_unique());
        // Two matching rows is not unique either, however complete the walk.
        assert!(!census(WalkCompleteness::Complete, 20, 3, 2).target_is_unique());
    }

    #[test]
    fn the_wrong_row_disappearing_is_named_as_such() {
        // A row with the target's text went, but no row with the target's shape
        // did -- so what went was not the row the plan pointed at. That is a
        // different fact from "too much happened" and gets its own label.
        assert_eq!(
            classify_removal(
                census(WalkCompleteness::Complete, 20, 2, 2),
                census(WalkCompleteness::Complete, 20, 2, 1)
            ),
            RemovalVerdict::Ambiguous("the_row_that_went_did_not_match_the_target_shape")
        );
    }

    #[test]
    fn a_unique_target_is_proven_by_its_own_absence_whatever_else_arrived() {
        // The strongest statement the counts support: one row carried this text,
        // a complete read now shows none, so that row went. Six replies arriving
        // meanwhile change nothing about it.
        assert!(census(WalkCompleteness::Complete, 20, 3, 1).target_is_unique());
        assert_eq!(
            classify_removal(
                census(WalkCompleteness::Complete, 20, 3, 1),
                census(WalkCompleteness::Complete, 26, 8, 0)
            ),
            RemovalVerdict::Proven
        );
    }

    #[test]
    fn a_duplicated_target_needs_the_transcript_to_have_been_still() {
        // With several rows sharing the text, the count cannot say WHICH one
        // went, so the attribution rests on the action -- and that is only good
        // enough over an otherwise unchanged transcript. Every deviation refuses.
        let before = census(WalkCompleteness::Complete, 20, 4, 2);
        assert!(!before.target_is_unique());
        assert_eq!(
            classify_removal(before, census(WalkCompleteness::Complete, 19, 3, 1)),
            RemovalVerdict::Proven
        );
        // A reply arrived as well: tolerated for a unique target, refused here.
        assert_eq!(
            classify_removal(before, census(WalkCompleteness::Complete, 20, 4, 1)),
            RemovalVerdict::Ambiguous("the_row_that_went_did_not_match_the_target_shape")
        );
        assert_eq!(
            classify_removal(before, census(WalkCompleteness::Complete, 21, 3, 1)),
            RemovalVerdict::Ambiguous("the_transcript_moved_around_a_duplicate_row")
        );
    }

    #[test]
    fn duplicate_rows_never_let_one_deletion_stand_in_for_another() {
        // Two identical rows, one deleted: provable, because the counted diff is
        // over multiplicities rather than a set.
        assert_eq!(
            classify_removal(
                census(WalkCompleteness::Complete, 20, 4, 2),
                census(WalkCompleteness::Complete, 19, 3, 1)
            ),
            RemovalVerdict::Proven
        );
        // Two identical rows, BOTH gone: which one the plan meant is unknowable,
        // and one of them was not the operator's to delete in this plan.
        assert_eq!(
            classify_removal(
                census(WalkCompleteness::Complete, 20, 4, 2),
                census(WalkCompleteness::Complete, 18, 2, 0)
            ),
            RemovalVerdict::Ambiguous("more_than_one_matching_row_disappeared")
        );
    }

    #[test]
    fn scroll_churn_is_refused_in_every_shape_it_takes() {
        // One evicted from the top while the target went: the classic scroll.
        assert_eq!(
            classify_removal(
                census(WalkCompleteness::Complete, 20, 3, 1),
                census(WalkCompleteness::Complete, 18, 2, 0)
            ),
            RemovalVerdict::Ambiguous("another_row_disappeared_too")
        );
        // An eviction that happened to take a same-shape row with it is caught
        // earlier, by the shape multiplicity, and refused just as hard.
        assert_eq!(
            classify_removal(
                census(WalkCompleteness::Complete, 20, 3, 1),
                census(WalkCompleteness::Complete, 18, 1, 0)
            ),
            RemovalVerdict::Ambiguous("more_than_one_matching_row_disappeared")
        );
        // A scroll that evicts AND appends can leave the total unchanged, so the
        // total is never the only guard. A drop in the shape count survives
        // additions -- they can only push it up -- which is what catches this.
        assert_eq!(
            classify_removal(
                census(WalkCompleteness::Complete, 20, 3, 1),
                census(WalkCompleteness::Complete, 20, 1, 0)
            ),
            RemovalVerdict::Ambiguous("more_than_one_matching_row_disappeared")
        );
    }

    #[test]
    fn a_conversation_emptied_between_the_two_reads_is_never_one_deletion() {
        assert_eq!(
            classify_removal(
                census(WalkCompleteness::Complete, 20, 3, 1),
                census(WalkCompleteness::Complete, 0, 0, 0)
            ),
            RemovalVerdict::Ambiguous("more_than_one_matching_row_disappeared")
        );
        // The genuine boundary case: a one-row conversation whose only row was
        // the target. Provable, and reachable only because a real surface reports
        // an empty read as truncated, so this shape cannot arrive from a wedged
        // leg.
        assert_eq!(
            classify_removal(
                census(WalkCompleteness::Complete, 1, 1, 1),
                census(WalkCompleteness::Complete, 0, 0, 0)
            ),
            RemovalVerdict::Proven
        );
    }

    #[test]
    fn a_busy_conversation_can_still_verify() {
        // Several replies arrived while OSL worked. Additions never weaken an
        // absence proof, and refusing them would make the feature useless in
        // exactly the conversations it is for.
        assert_eq!(
            classify_removal(
                census(WalkCompleteness::Complete, 20, 3, 1),
                census(WalkCompleteness::Complete, 26, 2, 0)
            ),
            RemovalVerdict::Proven
        );
        // Including replies that SHARE the target's shape, which is the ordinary
        // case: nearly every Discord row is the same height, so the shape count
        // goes UP even though the target's own row went. The original rule
        // demanded a shape drop of exactly one and turned this -- a real
        // deletion in a conversation someone replied in -- into `Ambiguous`.
        assert_eq!(
            classify_removal(
                census(WalkCompleteness::Complete, 20, 3, 1),
                census(WalkCompleteness::Complete, 24, 4, 0)
            ),
            RemovalVerdict::Proven
        );
    }

    #[test]
    fn no_reachable_verdict_but_proven_can_reach_a_verified_row() {
        // The end-to-end invariant, driven through the whole ladder rather than
        // asserted about the classifier: every non-`Proven` census pair leaves a
        // row that says `Failed`, carries `request_posted`, and never claims the
        // re-walk.
        let plan = confirmed(vec![owned_row(1)]);
        let hostile = [
            census(WalkCompleteness::Complete, 20, 3, 1),
            census(WalkCompleteness::Truncated, 12, 2, 0),
            census(WalkCompleteness::Complete, 18, 2, 0),
            census(WalkCompleteness::Complete, 20, 3, 1),
            RowCensus {
                identity: SurfaceIdentity::Changed,
                ..census(WalkCompleteness::Complete, 19, 2, 0)
            },
        ];
        for after in hostile.into_iter().skip(1) {
            let mut surface = FakeSurface::happy();
            surface.censuses = vec![census(WalkCompleteness::Complete, 20, 3, 1), after];
            let receipt = execute_confirmed_plan(&plan, &mut surface);
            let row = receipt.rows[0];
            assert_eq!(row.state, ActivityState::Failed, "{:?}", after);
            assert!(row.request_posted);
            assert!(!row.rewalk_proved_absent);
            assert!(!row.display_label().contains("Deleted"));
            assert!(!receipt.platform_removal_verified);
            assert!(receipt.is_self_consistent());
        }
    }

    #[test]
    fn nothing_destructive_starts_from_a_baseline_that_cannot_be_compared() {
        // The pre-flight is deliberately stricter than the classifier: a partial
        // or malformed baseline stops the row at `Held`, with nothing posted,
        // rather than posting and then discovering the evidence was never usable.
        let plan = confirmed(vec![owned_row(1)]);
        for (baseline, stage) in [
            (
                census(WalkCompleteness::Truncated, 20, 3, 1),
                "baseline_transcript_could_not_be_read",
            ),
            (
                RowCensus {
                    identity: SurfaceIdentity::Changed,
                    ..census(WalkCompleteness::Complete, 20, 3, 1)
                },
                "baseline_transcript_could_not_be_read",
            ),
            (
                census(WalkCompleteness::Complete, 0, 1, 1),
                "baseline_census_is_not_self_consistent",
            ),
            (
                census(WalkCompleteness::Complete, 20, 3, 0),
                "baseline_did_not_contain_the_row",
            ),
        ] {
            let mut surface = FakeSurface::happy();
            surface.censuses = vec![baseline];
            let receipt = execute_confirmed_plan(&plan, &mut surface);
            assert_eq!(receipt.rows[0].state, ActivityState::Held, "{stage}");
            assert_eq!(receipt.rows[0].stage, stage);
            assert!(!receipt.rows[0].request_posted);
        }
    }

    // -----------------------------------------------------------------------
    // Adversarial cases for the plan digest.
    // -----------------------------------------------------------------------

    #[test]
    fn the_digest_is_canonical_in_selection_order_but_not_in_selection_content() {
        // The same rows chosen in a different order are the same plan, so the
        // operator cannot be shown one preview and made to confirm another by
        // reordering. A different SET is a different plan.
        let scan = scan(vec![owned_row(1), owned_row(4), owned_row(7)]);
        let ascending = build_preview(&scan, &[1, 4, 7], true).expect("preview");
        let shuffled = build_preview(&scan, &[7, 1, 4], true).expect("preview");
        assert_eq!(ascending.plan_digest, shuffled.plan_digest);
        assert_eq!(
            ascending.rows.iter().map(|row| row.scan_ordinal).collect::<Vec<_>>(),
            shuffled.rows.iter().map(|row| row.scan_ordinal).collect::<Vec<_>>()
        );
        for subset in [vec![1, 4], vec![4, 7], vec![1], vec![1, 4, 7]] {
            let other = build_preview(&scan, &subset, true).expect("preview");
            assert_eq!(
                other.plan_digest == ascending.plan_digest,
                subset.len() == 3,
                "{subset:?}"
            );
        }
    }

    #[test]
    fn no_two_different_row_sets_can_share_a_digest_through_field_run_together() {
        // Adjacent ordinals and shared field values are where a digest built by
        // concatenation goes wrong: `1,23` and `12,3` must not render alike. The
        // separators and the row count in the header are what prevent it.
        let rows: Vec<ScannedRow> = [1usize, 2, 3, 12, 23, 123]
            .into_iter()
            .map(|ordinal| ScannedRow {
                scan_ordinal: ordinal,
                shape_ordinal: ordinal % 3,
                shape: shape(44, 1),
                text_len: 1,
                authored_by_operator: true,
            })
            .collect();
        let scan = scan(rows);
        let selections: Vec<Vec<usize>> = vec![
            vec![1, 23],
            vec![12, 3],
            vec![123],
            vec![1, 2, 3],
            vec![1, 2],
            vec![2, 3],
        ];
        let mut digests: Vec<String> = selections
            .iter()
            .map(|selection| {
                build_preview(&scan, selection, true)
                    .expect("preview")
                    .plan_digest
            })
            .collect();
        let total = digests.len();
        digests.sort_unstable();
        digests.dedup();
        assert_eq!(digests.len(), total, "two different plans shared a digest");
    }

    #[test]
    fn a_confirmation_cannot_be_replayed_onto_a_plan_it_was_not_taken_for() {
        // The renderer only ever echoes a digest string, so the attack to close
        // is echoing a digest the operator really did approve -- for something
        // else. Scope, generation and the exact row set all bind into it.
        let scan_a = scan(vec![owned_row(1), owned_row(2)]);
        let scan_b = DeletionScan {
            scope_binding_hash: "d".repeat(64),
            ..scan(vec![owned_row(1), owned_row(2)])
        };
        let approved = build_preview(&scan_a, &[1], true).expect("preview");
        let elsewhere = build_preview(&scan_b, &[1], true).expect("preview");
        assert_ne!(approved.plan_digest, elsewhere.plan_digest);
        // The digest the operator approved, replayed against the other
        // conversation's preview.
        assert_eq!(
            confirm_preview(
                &elsewhere,
                &approved.plan_digest,
                &scan_b.scope_binding_hash,
                scan_b.generation
            ),
            Err(PlanRefusal::ConfirmationStale)
        );
        // And a preview from one conversation confirmed against another's live
        // scope is refused before the digest is even considered.
        assert_eq!(
            confirm_preview(
                &approved,
                &approved.plan_digest,
                &scan_b.scope_binding_hash,
                scan_b.generation
            ),
            Err(PlanRefusal::ScopeChanged)
        );
    }

    #[test]
    fn a_row_that_changed_since_the_preview_invalidates_the_confirmation() {
        // The row is still at the same ordinal but is no longer the same row --
        // it was edited, or the transcript renumbered. Re-deriving the digest
        // from the preview's own rows is what catches it, because the operator
        // approved the row's shape and length, not merely its position.
        let original = scan(vec![owned_row(3)]);
        let approved = build_preview(&original, &[3], true).expect("preview");
        let mut edited = owned_row(3);
        edited.text_len = 40;
        let rescanned = scan(vec![edited]);
        let now = build_preview(&rescanned, &[3], true).expect("preview");
        assert_ne!(approved.plan_digest, now.plan_digest);
        assert_eq!(
            confirm_preview(
                &now,
                &approved.plan_digest,
                &rescanned.scope_binding_hash,
                rescanned.generation
            ),
            Err(PlanRefusal::ConfirmationStale)
        );
    }

    #[test]
    fn the_receipt_serializes_with_exactly_the_keys_the_renderer_parses() {
        // `adapters.ts:parseGuidedDeletionReceipt` requires an EXACT key set and
        // rejects anything else, so this DTO and that parser cannot be allowed to
        // drift silently in either direction.
        let plan = confirmed(vec![owned_row(1)]);
        let mut surface = FakeSurface::happy();
        let receipt = execute_confirmed_plan(&plan, &mut surface);
        let json = serde_json::to_value(&receipt).expect("receipt json");
        let object = json.as_object().expect("receipt object");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "burnPerformed",
                "contract",
                "generation",
                "localRemovalApplied",
                "oslContentExpiryApplied",
                "planDigest",
                "platformRemovalVerified",
                "rows",
                "rowsFailed",
                "rowsHeld",
                "rowsRequested",
                "rowsUnsupported",
                "rowsVerified",
                "scopeBindingHash",
            ]
        );
        let row = object["rows"][0].as_object().expect("row object");
        let mut row_keys: Vec<&str> = row.keys().map(String::as_str).collect();
        row_keys.sort_unstable();
        assert_eq!(
            row_keys,
            [
                "requestPosted",
                "rewalkProvedAbsent",
                "scanOrdinal",
                "stage",
                "state",
                "textLen",
            ]
        );
        assert_eq!(row["state"], serde_json::json!("verified"));
        assert_eq!(row["rewalkProvedAbsent"], serde_json::json!(true));
    }

    #[test]
    fn the_six_activity_state_names_are_exactly_the_specs() {
        assert_eq!(
            [
                ActivityState::Scheduled,
                ActivityState::Running,
                ActivityState::Verified,
                ActivityState::Failed,
                ActivityState::Unsupported,
                ActivityState::Held,
            ]
            .map(ActivityState::name),
            [
                "Scheduled",
                "Running",
                "Verified",
                "Failed",
                "Unsupported",
                "Held"
            ]
        );
    }
}
