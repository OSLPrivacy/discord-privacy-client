//! TASK 1476 - write AutoScrub notices.
//!
//! A scheduled AutoScrub batch tells the person four things, and every one of
//! them is a notice that opens the activity record behind it (TASK 1472,
//! `autoscrub_activity`):
//!
//! - **before-run**: the batch is about to start, and it names how many
//!   accounts are scheduled for it;
//! - **per-account**: one notice per account that finished, quoting that
//!   account's own record;
//! - **deletion-count**: how many messages the whole batch actually deleted;
//! - **failure**: one per account whose record carries failed deletions.
//!
//! Every count a notice shows is read back out of the saved activity records
//! -- this module never takes a count as a caller-supplied number, so a
//! notice cannot drift from the run it describes. Sending refuses outright if
//! a named run has no activity record, or if a record names an account that
//! was never scheduled for the batch.
//!
//! Pure apart from the two in-memory stores it reads/writes: no I/O, no live
//! clock read (`now_unix_secs` is always an explicit input), no Tauri. Placed
//! in `crates/ipc` alongside its gate 1472 for the reason recorded there:
//! `apps/osl-hub` does not compile in this lane (`services.rs` defines
//! `finish_active_service_account_run_notice` twice, at lines 907 and 2485,
//! from unrelated merge damage).

use crate::autoscrub_activity::{get_autoscrub_activity, AutoScrubActivityRecord};
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};

pub const AUTOSCRUB_NOTICES_SEND_COMMAND: &str = "autoscrub_notices_send";
pub const AUTOSCRUB_NOTICES_LIST_COMMAND: &str = "autoscrub_notices_list";
pub const AUTOSCRUB_NOTICE_OPEN_COMMAND: &str = "autoscrub_notice_open";

/// The activity command a notice's open-link resolves through. Kept as a
/// constant on the notice itself so a surface that shows a notice does not
/// have to know which command reads activity back.
pub const NOTICE_ACTIVITY_COMMAND: &str = "autoscrub_activity_get";

const BEFORE_RUN_ID_PREFIX: &str = "autoscrub-before-run-";
const ACCOUNT_ID_PREFIX: &str = "autoscrub-account-";
const DELETIONS_ID_PREFIX: &str = "autoscrub-deletions-";
const FAILURE_ID_PREFIX: &str = "autoscrub-failure-";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoScrubNoticeKind {
    BeforeRun,
    Account,
    DeletionCount,
    Failure,
}

/// What activating a notice opens. `run_id: None` means the whole batch's
/// activity (the before-run and deletion-count notices cover every account in
/// the batch); `Some(run_id)` means exactly that one run's record.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubActivityLink {
    pub command: String,
    pub batch_id: String,
    pub run_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubNotice {
    pub id: String,
    pub kind: AutoScrubNoticeKind,
    pub batch_id: String,
    pub sent_unix_secs: i64,
    /// The account this notice is about, when it is about exactly one.
    pub account_id: Option<String>,
    pub title: String,
    pub body: String,
    /// Set on the before-run notice only: how many accounts the batch has
    /// scheduled. Copied verbatim into `body`.
    pub scheduled_account_count: Option<u32>,
    /// Set on the deletion-count notice only: how many messages the finished
    /// records say were deleted. Copied verbatim into `body`.
    pub deleted_count: Option<u64>,
    pub opens: AutoScrubActivityLink,
}

/// What the caller knows before any notice is built: which batch, which
/// accounts are scheduled for it, and which runs finished. Counts are
/// deliberately absent -- they come from the activity records.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutoScrubNoticePlan {
    pub batch_id: String,
    pub scheduled_account_ids: Vec<String>,
    /// The run ids of the accounts that finished, each of which must already
    /// have an activity record (TASK 1472).
    pub finished_run_ids: Vec<String>,
    pub now_unix_secs: i64,
}

/// Every notice one batch sends, in the order a surface should show them.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubNoticeSet {
    pub batch_id: String,
    pub before_run: AutoScrubNotice,
    pub per_account: Vec<AutoScrubNotice>,
    pub deletion_count: AutoScrubNotice,
    pub failures: Vec<AutoScrubNotice>,
    /// The scheduled account count the before-run notice quotes.
    pub scheduled_account_count: u32,
    /// How many of the scheduled accounts finished with a record.
    pub finished_account_count: u32,
    /// The deletion total the deletion-count notice quotes: the sum of
    /// `deleted_count` over every finished record.
    pub total_deleted_count: u64,
}

impl AutoScrubNoticeSet {
    /// Every notice in the set, in show order.
    pub fn all(&self) -> Vec<AutoScrubNotice> {
        let mut all = Vec::with_capacity(3 + self.per_account.len() + self.failures.len());
        all.push(self.before_run.clone());
        all.extend(self.per_account.iter().cloned());
        all.push(self.deletion_count.clone());
        all.extend(self.failures.iter().cloned());
        all
    }
}

fn plural(count: u64, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("{count} {singular}")
    } else {
        format!("{count} {plural}")
    }
}

fn batch_link(batch_id: &str) -> AutoScrubActivityLink {
    AutoScrubActivityLink {
        command: NOTICE_ACTIVITY_COMMAND.to_owned(),
        batch_id: batch_id.to_owned(),
        run_id: None,
    }
}

fn run_link(batch_id: &str, run_id: &str) -> AutoScrubActivityLink {
    AutoScrubActivityLink {
        command: NOTICE_ACTIVITY_COMMAND.to_owned(),
        batch_id: batch_id.to_owned(),
        run_id: Some(run_id.to_owned()),
    }
}

fn validate_plan(plan: &AutoScrubNoticePlan) -> Result<(), String> {
    if plan.batch_id.trim().is_empty() {
        return Err("AutoScrub notices need a batch id".to_owned());
    }
    if plan.scheduled_account_ids.is_empty() {
        return Err("AutoScrub notices need at least one scheduled account".to_owned());
    }
    for (index, account_id) in plan.scheduled_account_ids.iter().enumerate() {
        if account_id.trim().is_empty() {
            return Err("a scheduled account id is empty".to_owned());
        }
        if plan.scheduled_account_ids[..index].contains(account_id) {
            return Err(format!("account {account_id} is scheduled twice"));
        }
    }
    for (index, run_id) in plan.finished_run_ids.iter().enumerate() {
        if run_id.trim().is_empty() {
            return Err("a finished run id is empty".to_owned());
        }
        if plan.finished_run_ids[..index].contains(run_id) {
            return Err(format!("run {run_id} is listed twice"));
        }
    }
    Ok(())
}

/// Reads the activity record (TASK 1472) for every finished run named by the
/// plan. Refuses a run with no record, and a record for an account that was
/// never scheduled for this batch -- either would let a notice quote a number
/// with no run behind it.
fn records_for_plan(plan: &AutoScrubNoticePlan) -> Result<Vec<AutoScrubActivityRecord>, String> {
    let mut records = Vec::with_capacity(plan.finished_run_ids.len());
    for run_id in &plan.finished_run_ids {
        let record = get_autoscrub_activity(run_id)?.ok_or_else(|| {
            format!("no AutoScrub activity record for run {run_id}; cannot send a notice about it")
        })?;
        if !plan.scheduled_account_ids.contains(&record.account_id) {
            return Err(format!(
                "run {run_id} recorded account {account}, which is not scheduled in batch {batch}",
                account = record.account_id,
                batch = plan.batch_id,
            ));
        }
        records.push(record);
    }
    Ok(records)
}

/// Builds every notice for one batch from the plan plus the saved activity
/// records. Pure with respect to the notice store: builds, does not send.
pub fn build_autoscrub_notices(plan: &AutoScrubNoticePlan) -> Result<AutoScrubNoticeSet, String> {
    validate_plan(plan)?;
    let records = records_for_plan(plan)?;

    let scheduled_account_count = u32::try_from(plan.scheduled_account_ids.len())
        .map_err(|_| "too many scheduled accounts to count".to_owned())?;
    let finished_account_count =
        u32::try_from(records.len()).map_err(|_| "too many finished runs to count".to_owned())?;
    let total_deleted_count: u64 = records.iter().map(|r| u64::from(r.deleted_count)).sum();

    let batch_id = plan.batch_id.clone();
    let sent = plan.now_unix_secs;

    let before_run = AutoScrubNotice {
        id: format!("{BEFORE_RUN_ID_PREFIX}{batch_id}"),
        kind: AutoScrubNoticeKind::BeforeRun,
        batch_id: batch_id.clone(),
        sent_unix_secs: sent,
        account_id: None,
        title: "AutoScrub is about to run".to_owned(),
        body: format!(
            "AutoScrub is scheduled to run for {accounts}.",
            accounts = plural(u64::from(scheduled_account_count), "account", "accounts"),
        ),
        scheduled_account_count: Some(scheduled_account_count),
        deleted_count: None,
        opens: batch_link(&batch_id),
    };

    let per_account = records
        .iter()
        .map(|record| AutoScrubNotice {
            id: format!("{ACCOUNT_ID_PREFIX}{run}", run = record.run_id),
            kind: AutoScrubNoticeKind::Account,
            batch_id: batch_id.clone(),
            sent_unix_secs: sent,
            account_id: Some(record.account_id.clone()),
            title: format!("AutoScrub finished {account}", account = record.account_id),
            body: format!(
                "{account}: matched {matched}, deleted {deleted}, failed {failed}.",
                account = record.account_id,
                matched = record.matched_count,
                deleted = record.deleted_count,
                failed = record.failed_count,
            ),
            scheduled_account_count: None,
            deleted_count: Some(u64::from(record.deleted_count)),
            opens: run_link(&batch_id, &record.run_id),
        })
        .collect::<Vec<_>>();

    let deletion_count = AutoScrubNotice {
        id: format!("{DELETIONS_ID_PREFIX}{batch_id}"),
        kind: AutoScrubNoticeKind::DeletionCount,
        batch_id: batch_id.clone(),
        sent_unix_secs: sent,
        account_id: None,
        title: format!(
            "AutoScrub deleted {messages}",
            messages = plural(total_deleted_count, "message", "messages"),
        ),
        body: format!(
            "AutoScrub finished {finished} of {scheduled} scheduled accounts and deleted \
             {messages}.",
            finished = finished_account_count,
            scheduled = scheduled_account_count,
            messages = plural(total_deleted_count, "message", "messages"),
        ),
        scheduled_account_count: Some(scheduled_account_count),
        deleted_count: Some(total_deleted_count),
        opens: batch_link(&batch_id),
    };

    let failures = records
        .iter()
        .filter(|record| record.failed_count > 0)
        .map(|record| AutoScrubNotice {
            id: format!("{FAILURE_ID_PREFIX}{run}", run = record.run_id),
            kind: AutoScrubNoticeKind::Failure,
            batch_id: batch_id.clone(),
            sent_unix_secs: sent,
            account_id: Some(record.account_id.clone()),
            title: "AutoScrub could not delete everything".to_owned(),
            body: format!(
                "{account}: {failed} could not be deleted.",
                account = record.account_id,
                failed = plural(u64::from(record.failed_count), "message", "messages"),
            ),
            scheduled_account_count: None,
            deleted_count: Some(u64::from(record.deleted_count)),
            opens: run_link(&batch_id, &record.run_id),
        })
        .collect::<Vec<_>>();

    Ok(AutoScrubNoticeSet {
        batch_id,
        before_run,
        per_account,
        deletion_count,
        failures,
        scheduled_account_count,
        finished_account_count,
        total_deleted_count,
    })
}

#[derive(Clone)]
struct SentBatch {
    batch_id: String,
    /// The runs this batch's activity link covers, in plan order.
    run_ids: Vec<String>,
    notices: Vec<AutoScrubNotice>,
}

#[derive(Default)]
struct AutoScrubNoticeOutbox {
    batches: Vec<SentBatch>,
}

static NOTICE_OUTBOX: OnceLock<Mutex<AutoScrubNoticeOutbox>> = OnceLock::new();

fn notice_outbox() -> &'static Mutex<AutoScrubNoticeOutbox> {
    NOTICE_OUTBOX.get_or_init(|| Mutex::new(AutoScrubNoticeOutbox::default()))
}

fn lock_outbox() -> Result<std::sync::MutexGuard<'static, AutoScrubNoticeOutbox>, String> {
    notice_outbox()
        .lock()
        .map_err(|_| "AutoScrub notice outbox is unavailable".to_owned())
}

#[cfg(test)]
fn reset_notice_outbox_for_test() {
    *notice_outbox().lock().expect("notice outbox lock") = AutoScrubNoticeOutbox::default();
}

/// Builds the batch's notices and sends them, replacing anything already sent
/// for the same batch id.
pub fn send_autoscrub_notices(plan: &AutoScrubNoticePlan) -> Result<AutoScrubNoticeSet, String> {
    let set = build_autoscrub_notices(plan)?;
    let mut outbox = lock_outbox()?;
    outbox
        .batches
        .retain(|batch| batch.batch_id != set.batch_id);
    outbox.batches.push(SentBatch {
        batch_id: set.batch_id.clone(),
        run_ids: plan.finished_run_ids.clone(),
        notices: set.all(),
    });
    Ok(set)
}

/// Every notice sent for `batch_id`, in show order.
pub fn list_autoscrub_notices(batch_id: &str) -> Result<Vec<AutoScrubNotice>, String> {
    let outbox = lock_outbox()?;
    Ok(outbox
        .batches
        .iter()
        .find(|batch| batch.batch_id == batch_id)
        .map(|batch| batch.notices.clone())
        .unwrap_or_default())
}

/// What activating a notice showed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedAutoScrubActivity {
    pub notice_id: String,
    pub batch_id: String,
    /// The activity records the notice opened: one for a notice about a
    /// single run, every record in the batch for a batch-wide notice.
    pub records: Vec<AutoScrubActivityRecord>,
}

/// Activates a sent notice: resolves its activity link through TASK 1472's
/// store and returns the record(s) it opens.
pub fn open_autoscrub_notice(notice_id: &str) -> Result<OpenedAutoScrubActivity, String> {
    let (notice, run_ids) = {
        let outbox = lock_outbox()?;
        let mut found = None;
        for batch in &outbox.batches {
            if let Some(notice) = batch.notices.iter().find(|n| n.id == notice_id) {
                found = Some((notice.clone(), batch.run_ids.clone()));
                break;
            }
        }
        found.ok_or_else(|| format!("no AutoScrub notice with id {notice_id} has been sent"))?
    };

    let wanted: Vec<String> = match &notice.opens.run_id {
        Some(run_id) => vec![run_id.clone()],
        None => run_ids,
    };

    let mut records = Vec::with_capacity(wanted.len());
    for run_id in &wanted {
        let record = get_autoscrub_activity(run_id)?.ok_or_else(|| {
            format!("notice {notice_id} opens run {run_id}, which has no activity record")
        })?;
        records.push(record);
    }

    Ok(OpenedAutoScrubActivity {
        notice_id: notice.id,
        batch_id: notice.batch_id,
        records,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchIdRequest {
    batch_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NoticeIdRequest {
    notice_id: String,
}

/// The direct-invoke surface: `autoscrub_notices_send` builds and sends a
/// batch's notices, `autoscrub_notices_list` reads them back, and
/// `autoscrub_notice_open` activates one and returns the activity it opens.
/// Every reply is a JSON object carrying `ok`, matching the convention the
/// other `crates/ipc` command modules use.
pub fn run_autoscrub_notices_command(command: &str, request_json: &str) -> String {
    match command {
        AUTOSCRUB_NOTICES_SEND_COMMAND => {
            let plan: AutoScrubNoticePlan = match serde_json::from_str(request_json) {
                Ok(plan) => plan,
                Err(err) => return json_error_reply(command, "invalid_request", &err.to_string()),
            };
            match send_autoscrub_notices(&plan) {
                Ok(set) => json_ok_reply(command, &set),
                Err(err) => json_error_reply(command, "bad_request", &err),
            }
        }
        AUTOSCRUB_NOTICES_LIST_COMMAND => {
            let request: BatchIdRequest = match serde_json::from_str(request_json) {
                Ok(request) => request,
                Err(err) => return json_error_reply(command, "invalid_request", &err.to_string()),
            };
            match list_autoscrub_notices(&request.batch_id) {
                Ok(notices) => json_ok_reply(command, &serde_json::json!({ "notices": notices })),
                Err(err) => json_error_reply(command, "bad_request", &err),
            }
        }
        AUTOSCRUB_NOTICE_OPEN_COMMAND => {
            let request: NoticeIdRequest = match serde_json::from_str(request_json) {
                Ok(request) => request,
                Err(err) => return json_error_reply(command, "invalid_request", &err.to_string()),
            };
            match open_autoscrub_notice(&request.notice_id) {
                Ok(opened) => json_ok_reply(command, &opened),
                Err(err) => json_error_reply(command, "not_found", &err),
            }
        }
        _ => json_error_reply(
            command,
            "unknown_command",
            &format!("unknown command '{command}'"),
        ),
    }
}

fn json_ok_reply<T: Serialize>(command: &str, result: &T) -> String {
    let result = serde_json::to_value(result).expect("AutoScrub notices always serialize");
    serde_json::json!({
        "command": command,
        "ok": true,
        "result": result,
    })
    .to_string()
}

fn json_error_reply(command: &str, error_code: &str, message: &str) -> String {
    serde_json::json!({
        "command": command,
        "ok": false,
        "errorCode": error_code,
        "error": message,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autoscrub_activity::{
        record_autoscrub_activity, AutoScrubRunLocation, AutoScrubRunOutcome,
    };

    fn outcome(
        run_id: &str,
        account_id: &str,
        matched: u32,
        deleted: u32,
        failed: u32,
    ) -> AutoScrubRunOutcome {
        AutoScrubRunOutcome {
            run_id: run_id.to_owned(),
            account_id: account_id.to_owned(),
            start_unix_secs: 1_786_300_000,
            end_unix_secs: 1_786_300_600,
            matched_count: matched,
            deleted_count: deleted,
            failed_count: failed,
            location: AutoScrubRunLocation::Local,
        }
    }

    /// Three scheduled accounts, all three finished: 5 + 8 + 2 = 15 deletions,
    /// one of them with failures.
    fn fixture_plan(batch_id: &str) -> AutoScrubNoticePlan {
        let runs = [
            outcome(
                &format!("{batch_id}-run-a"),
                &format!("{batch_id}-account-a"),
                7,
                5,
                0,
            ),
            outcome(
                &format!("{batch_id}-run-b"),
                &format!("{batch_id}-account-b"),
                9,
                8,
                1,
            ),
            outcome(
                &format!("{batch_id}-run-c"),
                &format!("{batch_id}-account-c"),
                2,
                2,
                0,
            ),
        ];
        for run in &runs {
            record_autoscrub_activity(run).expect("fixture run records");
        }
        AutoScrubNoticePlan {
            batch_id: batch_id.to_owned(),
            scheduled_account_ids: runs.iter().map(|r| r.account_id.clone()).collect(),
            finished_run_ids: runs.iter().map(|r| r.run_id.clone()).collect(),
            now_unix_secs: 1_786_301_000,
        }
    }

    #[test]
    fn a_batch_sends_all_four_kinds_of_notice() {
        reset_notice_outbox_for_test();
        let set = send_autoscrub_notices(&fixture_plan("unit-1476-kinds")).expect("sends");

        assert_eq!(set.before_run.kind, AutoScrubNoticeKind::BeforeRun);
        assert_eq!(set.per_account.len(), 3);
        assert!(set
            .per_account
            .iter()
            .all(|n| n.kind == AutoScrubNoticeKind::Account));
        assert_eq!(set.deletion_count.kind, AutoScrubNoticeKind::DeletionCount);
        assert_eq!(set.failures.len(), 1);
        assert_eq!(set.failures[0].kind, AutoScrubNoticeKind::Failure);
        assert_eq!(set.all().len(), 6);
    }

    #[test]
    fn the_before_run_notice_names_the_scheduled_account_count() {
        reset_notice_outbox_for_test();
        let set = send_autoscrub_notices(&fixture_plan("unit-1476-before")).expect("sends");
        assert_eq!(set.scheduled_account_count, 3);
        assert_eq!(set.before_run.scheduled_account_count, Some(3));
        assert_eq!(
            set.before_run.body,
            "AutoScrub is scheduled to run for 3 accounts."
        );
    }

    #[test]
    fn the_deletion_count_notice_sums_the_records() {
        reset_notice_outbox_for_test();
        let set = send_autoscrub_notices(&fixture_plan("unit-1476-deletions")).expect("sends");
        assert_eq!(set.total_deleted_count, 15);
        assert_eq!(set.deletion_count.deleted_count, Some(15));
        assert_eq!(set.deletion_count.title, "AutoScrub deleted 15 messages");
        assert_eq!(
            set.deletion_count.body,
            "AutoScrub finished 3 of 3 scheduled accounts and deleted 15 messages."
        );
    }

    #[test]
    fn a_single_account_and_a_single_deletion_read_as_singular() {
        reset_notice_outbox_for_test();
        let run = outcome("unit-1476-one-run", "unit-1476-one-account", 1, 1, 1);
        record_autoscrub_activity(&run).expect("records");
        let set = send_autoscrub_notices(&AutoScrubNoticePlan {
            batch_id: "unit-1476-one".to_owned(),
            scheduled_account_ids: vec![run.account_id.clone()],
            finished_run_ids: vec![run.run_id.clone()],
            now_unix_secs: 1_786_301_000,
        })
        .expect("sends");
        assert_eq!(
            set.before_run.body,
            "AutoScrub is scheduled to run for 1 account."
        );
        assert_eq!(set.deletion_count.title, "AutoScrub deleted 1 message");
        assert_eq!(
            set.failures[0].body,
            "unit-1476-one-account: 1 message could not be deleted."
        );
    }

    #[test]
    fn only_accounts_with_failures_get_a_failure_notice() {
        reset_notice_outbox_for_test();
        let set = send_autoscrub_notices(&fixture_plan("unit-1476-failures")).expect("sends");
        assert_eq!(set.failures.len(), 1);
        assert_eq!(
            set.failures[0].account_id.as_deref(),
            Some("unit-1476-failures-account-b")
        );
        assert_eq!(
            set.failures[0].body,
            "unit-1476-failures-account-b: 1 message could not be deleted."
        );
    }

    #[test]
    fn every_notice_opens_activity() {
        reset_notice_outbox_for_test();
        let set = send_autoscrub_notices(&fixture_plan("unit-1476-open")).expect("sends");
        for notice in set.all() {
            let opened = open_autoscrub_notice(&notice.id).expect("notice opens activity");
            assert_eq!(opened.notice_id, notice.id);
            let expected = if notice.opens.run_id.is_some() { 1 } else { 3 };
            assert_eq!(
                opened.records.len(),
                expected,
                "notice {} opened the wrong number of records",
                notice.id
            );
        }
    }

    #[test]
    fn a_run_with_no_activity_record_is_refused() {
        reset_notice_outbox_for_test();
        let mut plan = fixture_plan("unit-1476-missing");
        plan.finished_run_ids
            .push("unit-1476-missing-run-never-recorded".to_owned());
        let err = send_autoscrub_notices(&plan).expect_err("must be refused");
        assert_eq!(
            err,
            "no AutoScrub activity record for run unit-1476-missing-run-never-recorded; \
             cannot send a notice about it"
        );
    }

    #[test]
    fn a_record_for_an_unscheduled_account_is_refused() {
        reset_notice_outbox_for_test();
        let mut plan = fixture_plan("unit-1476-stranger");
        plan.scheduled_account_ids
            .retain(|id| id != "unit-1476-stranger-account-b");
        let err = send_autoscrub_notices(&plan).expect_err("must be refused");
        assert_eq!(
            err,
            "run unit-1476-stranger-run-b recorded account unit-1476-stranger-account-b, \
             which is not scheduled in batch unit-1476-stranger"
        );
    }

    #[test]
    fn a_batch_with_no_scheduled_accounts_is_refused() {
        reset_notice_outbox_for_test();
        let err = send_autoscrub_notices(&AutoScrubNoticePlan {
            batch_id: "unit-1476-empty".to_owned(),
            scheduled_account_ids: vec![],
            finished_run_ids: vec![],
            now_unix_secs: 1,
        })
        .expect_err("must be refused");
        assert_eq!(err, "AutoScrub notices need at least one scheduled account");
    }

    #[test]
    fn sending_a_batch_twice_replaces_the_earlier_notices() {
        reset_notice_outbox_for_test();
        let mut plan = fixture_plan("unit-1476-resend");
        send_autoscrub_notices(&plan).expect("first send");
        assert_eq!(list_autoscrub_notices(&plan.batch_id).unwrap().len(), 6);

        plan.finished_run_ids.truncate(1);
        let set = send_autoscrub_notices(&plan).expect("second send");
        // before-run + one per-account + deletion-count; run "a" had no
        // failures, so no failure notice.
        let listed = list_autoscrub_notices(&plan.batch_id).unwrap();
        assert_eq!(listed.len(), 3);
        assert_eq!(set.total_deleted_count, 5);
        assert_eq!(set.finished_account_count, 1);
        assert_eq!(set.scheduled_account_count, 3);
    }

    #[test]
    fn opening_a_notice_that_was_never_sent_is_refused() {
        reset_notice_outbox_for_test();
        let err = open_autoscrub_notice("autoscrub-account-never-sent").expect_err("refused");
        assert_eq!(
            err,
            "no AutoScrub notice with id autoscrub-account-never-sent has been sent"
        );
    }

    #[test]
    fn direct_invoke_unknown_command_is_refused() {
        let reply = run_autoscrub_notices_command("bogus", "{}");
        let value: serde_json::Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(value["errorCode"], "unknown_command");
    }
}
