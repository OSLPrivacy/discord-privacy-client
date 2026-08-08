//! Task 1463: schedule storage.
//!
//! Saves the schedule choice for an AutoScrub account: daily, weekly,
//! monthly, or "only when I choose" (no automatic run at all), and computes
//! the next run time that choice implies.
//!
//! Kept process-local and Tauri-free (same boundary every other `crates/ipc`
//! module holds to; see the crate-level comment in `Cargo.toml`). `now` is
//! always taken as an explicit parameter rather than read from the clock
//! internally, so "the expected next run" is something a test can assert on
//! exactly rather than a moving target.
//!
//! Calendar math (weekday, day-of-month, month length) is done with the
//! Howard Hinnant `civil_from_days` / `days_from_civil` algorithm rather than
//! pulling in a date crate — this crate has no date dependency today and the
//! algorithm is a few lines of pure integer arithmetic.

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::time::SystemTime;

pub const SCHEDULE_SAVE_COMMAND: &str = "schedule_save";
pub const SCHEDULE_GET_COMMAND: &str = "schedule_get";
pub const SCHEDULE_LIST_COMMAND: &str = "schedule_list";

pub const UNKNOWN_COMMAND: &str = "unknown_command";
pub const INVALID_ACCOUNT_ID: &str = "invalid_account_id";
pub const INVALID_SCHEDULE_TIME: &str = "invalid_schedule_time";
pub const INVALID_SCHEDULE_DAY: &str = "invalid_schedule_day";
pub const SCHEDULE_NOT_FOUND: &str = "schedule_not_found";

/// Longest account id this store accepts. Matches the bound
/// `autoscrub_account_switches` (Task 1455) and `preferences::is_scrub_account_id`
/// use for the same field, so an id that round-trips through the Scrub
/// account list also round-trips here.
pub const MAX_ACCOUNT_ID_LEN: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Weekday {
    Sun,
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
}

impl Weekday {
    /// 0 = Sunday .. 6 = Saturday, matching `weekday_index_from_days` below.
    fn to_index(self) -> i64 {
        match self {
            Weekday::Sun => 0,
            Weekday::Mon => 1,
            Weekday::Tue => 2,
            Weekday::Wed => 3,
            Weekday::Thu => 4,
            Weekday::Fri => 5,
            Weekday::Sat => 6,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScheduleKind {
    Daily { hour: u8, minute: u8 },
    Weekly { weekday: Weekday, hour: u8, minute: u8 },
    /// `day` is 1..=31. A month shorter than `day` runs on that month's last
    /// day instead (e.g. `day: 31` in April runs on April 30th).
    Monthly { day: u8, hour: u8, minute: u8 },
    /// No automatic run at all; the account only runs on an explicit "Run
    /// now" (Task 1473). `next_run_unix_secs` is always `None` for this kind.
    OnlyWhenChosen,
}

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum ScheduleError {
    #[error("account id must be 1..={MAX_ACCOUNT_ID_LEN} bytes")]
    InvalidAccountId,
    #[error("hour must be 0..=23 and minute must be 0..=59")]
    InvalidTime,
    #[error("day must be 1..=31")]
    InvalidDay,
    #[error("no schedule saved for this account")]
    NotFound,
}

impl ScheduleError {
    fn code(&self) -> &'static str {
        match self {
            ScheduleError::InvalidAccountId => INVALID_ACCOUNT_ID,
            ScheduleError::InvalidTime => INVALID_SCHEDULE_TIME,
            ScheduleError::InvalidDay => INVALID_SCHEDULE_DAY,
            ScheduleError::NotFound => SCHEDULE_NOT_FOUND,
        }
    }
}

fn validate_account_id(account_id: &str) -> Result<(), ScheduleError> {
    if account_id.is_empty() || account_id.len() > MAX_ACCOUNT_ID_LEN {
        return Err(ScheduleError::InvalidAccountId);
    }
    Ok(())
}

impl ScheduleKind {
    fn validate(&self) -> Result<(), ScheduleError> {
        match *self {
            ScheduleKind::Daily { hour, minute } | ScheduleKind::Weekly { hour, minute, .. } => {
                validate_time(hour, minute)
            }
            ScheduleKind::Monthly { day, hour, minute } => {
                validate_time(hour, minute)?;
                if !(1..=31).contains(&day) {
                    return Err(ScheduleError::InvalidDay);
                }
                Ok(())
            }
            ScheduleKind::OnlyWhenChosen => Ok(()),
        }
    }

    /// The next unix-second timestamp this schedule fires at, strictly after
    /// `now`. `None` for `OnlyWhenChosen`.
    fn next_run_after(&self, now: SystemTime) -> Option<u64> {
        let now_secs = now
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        match *self {
            ScheduleKind::Daily { hour, minute } => Some(next_daily(now_secs, hour, minute)),
            ScheduleKind::Weekly { weekday, hour, minute } => {
                Some(next_weekly(now_secs, weekday.to_index(), hour, minute))
            }
            ScheduleKind::Monthly { day, hour, minute } => {
                Some(next_monthly(now_secs, day, hour, minute))
            }
            ScheduleKind::OnlyWhenChosen => None,
        }
    }
}

fn validate_time(hour: u8, minute: u8) -> Result<(), ScheduleError> {
    if hour > 23 || minute > 59 {
        return Err(ScheduleError::InvalidTime);
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleRecord {
    pub account_id: String,
    pub kind: ScheduleKind,
    pub next_run_unix_secs: Option<u64>,
}

#[derive(Default)]
pub struct ScheduleStore {
    schedules: BTreeMap<String, ScheduleRecord>,
}

impl ScheduleStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Saves (creating or replacing) the one schedule an account may hold,
    /// and returns the saved record with its computed next run time.
    pub fn save_schedule(
        &mut self,
        account_id: &str,
        kind: ScheduleKind,
        now: SystemTime,
    ) -> Result<ScheduleRecord, ScheduleError> {
        validate_account_id(account_id)?;
        kind.validate()?;
        let record = ScheduleRecord {
            account_id: account_id.to_string(),
            next_run_unix_secs: kind.next_run_after(now),
            kind,
        };
        self.schedules
            .insert(account_id.to_string(), record.clone());
        Ok(record)
    }

    pub fn get(&self, account_id: &str) -> Result<ScheduleRecord, ScheduleError> {
        self.schedules
            .get(account_id)
            .cloned()
            .ok_or(ScheduleError::NotFound)
    }

    pub fn list(&self) -> Vec<ScheduleRecord> {
        self.schedules.values().cloned().collect()
    }

    /// Remove the one schedule for `account_id` and return the record that was
    /// removed.  The AutoScrub controls use this only after an in-flight run
    /// reaches its safe stopping point; they must not replace deletion with a
    /// fresh `OnlyWhenChosen` record, because that would leave automation
    /// state behind when the person chose "Stop and turn off".
    pub fn remove(&mut self, account_id: &str) -> Result<ScheduleRecord, ScheduleError> {
        self.schedules
            .remove(account_id)
            .ok_or(ScheduleError::NotFound)
    }

    pub fn len(&self) -> usize {
        self.schedules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.schedules.is_empty()
    }
}

// --- Calendar math (Howard Hinnant's civil_from_days / days_from_civil) ---

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = (m + 9) % 12; // [0, 11]
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn is_leap_year(y: i64) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(y) {
                29
            } else {
                28
            }
        }
        _ => unreachable!("month is always 1..=12"),
    }
}

/// 0 = Sunday .. 6 = Saturday for the civil day number `days`
/// (days since 1970-01-01, which was a Thursday).
fn weekday_index_from_days(days: i64) -> i64 {
    (days + 4).rem_euclid(7)
}

fn ymd_hms_to_unix(y: i64, m: i64, d: i64, hour: u8, minute: u8) -> u64 {
    let days = days_from_civil(y, m, d);
    (days * 86_400 + i64::from(hour) * 3_600 + i64::from(minute) * 60) as u64
}

fn now_civil(now_secs: u64) -> (i64, i64, i64) {
    let days = (now_secs / 86_400) as i64;
    civil_from_days(days)
}

fn next_daily(now_secs: u64, hour: u8, minute: u8) -> u64 {
    let (y, m, d) = now_civil(now_secs);
    let candidate = ymd_hms_to_unix(y, m, d, hour, minute);
    if candidate > now_secs {
        return candidate;
    }
    let tomorrow = days_from_civil(y, m, d) + 1;
    let (y, m, d) = civil_from_days(tomorrow);
    ymd_hms_to_unix(y, m, d, hour, minute)
}

fn next_weekly(now_secs: u64, target_weekday: i64, hour: u8, minute: u8) -> u64 {
    let (y, m, d) = now_civil(now_secs);
    let today_days = days_from_civil(y, m, d);
    let current_weekday = weekday_index_from_days(today_days);
    let diff = (target_weekday - current_weekday).rem_euclid(7);
    let candidate_days = today_days + diff;
    let (cy, cm, cd) = civil_from_days(candidate_days);
    let candidate = ymd_hms_to_unix(cy, cm, cd, hour, minute);
    if candidate > now_secs {
        return candidate;
    }
    // diff == 0 and today's time has already passed: push a full week out.
    let (ny, nm, nd) = civil_from_days(candidate_days + 7);
    ymd_hms_to_unix(ny, nm, nd, hour, minute)
}

fn next_monthly(now_secs: u64, day: u8, hour: u8, minute: u8) -> u64 {
    let (y, m, _) = now_civil(now_secs);
    let clamped_day = i64::from(day).min(days_in_month(y, m));
    let candidate = ymd_hms_to_unix(y, m, clamped_day, hour, minute);
    if candidate > now_secs {
        return candidate;
    }
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    let clamped_day = i64::from(day).min(days_in_month(ny, nm));
    ymd_hms_to_unix(ny, nm, clamped_day, hour, minute)
}

// --- Direct-command surface ---

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScheduleSaveRequest {
    account_id: String,
    #[serde(flatten)]
    kind: ScheduleKind,
    /// Unix-second clock the schedule is computed against. Direct-invoke
    /// callers always pass this explicitly; there is no hidden system-clock
    /// read in this module.
    now_unix_secs: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScheduleGetRequest {
    account_id: String,
}

fn ok_reply(command: &str, data: serde_json::Value) -> String {
    let mut body = json!({ "ok": true, "command": command });
    if let serde_json::Value::Object(map) = data {
        body.as_object_mut().unwrap().extend(map);
    }
    body.to_string()
}

fn error_reply(command: &str, error_code: &str, error: String) -> String {
    json!({
        "ok": false,
        "command": command,
        "errorCode": error_code,
        "error": error,
    })
    .to_string()
}

/// Direct-invoke dispatch for `schedule_save`, `schedule_get`, and
/// `schedule_list`. Every reply is a JSON object carrying `ok`.
pub fn run_schedule_command(store: &mut ScheduleStore, command: &str, request_json: &str) -> String {
    match command {
        SCHEDULE_SAVE_COMMAND => match serde_json::from_str::<ScheduleSaveRequest>(request_json) {
            Ok(request) => {
                let now = std::time::UNIX_EPOCH + std::time::Duration::from_secs(request.now_unix_secs);
                match store.save_schedule(&request.account_id, request.kind, now) {
                    Ok(record) => ok_reply(command, json!({ "schedule": record })),
                    Err(error) => error_reply(command, error.code(), error.to_string()),
                }
            }
            Err(error) => error_reply(command, "bad_request", error.to_string()),
        },
        SCHEDULE_GET_COMMAND => match serde_json::from_str::<ScheduleGetRequest>(request_json) {
            Ok(request) => match store.get(&request.account_id) {
                Ok(record) => ok_reply(command, json!({ "schedule": record })),
                Err(error) => error_reply(command, error.code(), error.to_string()),
            },
            Err(error) => error_reply(command, "bad_request", error.to_string()),
        },
        SCHEDULE_LIST_COMMAND => ok_reply(
            command,
            json!({ "schedules": store.list(), "count": store.len() }),
        ),
        other => error_reply(
            other,
            UNKNOWN_COMMAND,
            format!("{other} is not a schedule storage command."),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn secs(unix: u64) -> SystemTime {
        std::time::UNIX_EPOCH + Duration::from_secs(unix)
    }

    // 2026-08-07 is a Friday. 12:00:00 UTC that day.
    const FRIDAY_NOON: u64 = 1_786_104_000;

    #[test]
    fn daily_later_today() {
        let mut store = ScheduleStore::new();
        let record = store
            .save_schedule(
                "acct-1",
                ScheduleKind::Daily { hour: 18, minute: 0 },
                secs(FRIDAY_NOON),
            )
            .unwrap();
        let expected = ymd_hms_to_unix(2026, 8, 7, 18, 0);
        assert_eq!(record.next_run_unix_secs, Some(expected));
    }

    #[test]
    fn daily_already_passed_rolls_to_tomorrow() {
        let mut store = ScheduleStore::new();
        let record = store
            .save_schedule(
                "acct-1",
                ScheduleKind::Daily { hour: 6, minute: 0 },
                secs(FRIDAY_NOON),
            )
            .unwrap();
        let expected = ymd_hms_to_unix(2026, 8, 8, 6, 0);
        assert_eq!(record.next_run_unix_secs, Some(expected));
    }

    #[test]
    fn weekly_next_occurrence() {
        let mut store = ScheduleStore::new();
        // Friday noon, ask for Monday 09:00 -> the following Monday.
        let record = store
            .save_schedule(
                "acct-1",
                ScheduleKind::Weekly {
                    weekday: Weekday::Mon,
                    hour: 9,
                    minute: 0,
                },
                secs(FRIDAY_NOON),
            )
            .unwrap();
        let expected = ymd_hms_to_unix(2026, 8, 10, 9, 0);
        assert_eq!(record.next_run_unix_secs, Some(expected));
    }

    #[test]
    fn weekly_same_day_later_time() {
        let mut store = ScheduleStore::new();
        let record = store
            .save_schedule(
                "acct-1",
                ScheduleKind::Weekly {
                    weekday: Weekday::Fri,
                    hour: 18,
                    minute: 0,
                },
                secs(FRIDAY_NOON),
            )
            .unwrap();
        let expected = ymd_hms_to_unix(2026, 8, 7, 18, 0);
        assert_eq!(record.next_run_unix_secs, Some(expected));
    }

    #[test]
    fn weekly_same_day_time_passed_rolls_a_full_week() {
        let mut store = ScheduleStore::new();
        let record = store
            .save_schedule(
                "acct-1",
                ScheduleKind::Weekly {
                    weekday: Weekday::Fri,
                    hour: 6,
                    minute: 0,
                },
                secs(FRIDAY_NOON),
            )
            .unwrap();
        let expected = ymd_hms_to_unix(2026, 8, 14, 6, 0);
        assert_eq!(record.next_run_unix_secs, Some(expected));
    }

    #[test]
    fn monthly_later_this_month() {
        let mut store = ScheduleStore::new();
        let record = store
            .save_schedule(
                "acct-1",
                ScheduleKind::Monthly {
                    day: 20,
                    hour: 0,
                    minute: 0,
                },
                secs(FRIDAY_NOON),
            )
            .unwrap();
        let expected = ymd_hms_to_unix(2026, 8, 20, 0, 0);
        assert_eq!(record.next_run_unix_secs, Some(expected));
    }

    #[test]
    fn monthly_already_passed_rolls_to_next_month() {
        let mut store = ScheduleStore::new();
        let record = store
            .save_schedule(
                "acct-1",
                ScheduleKind::Monthly {
                    day: 1,
                    hour: 0,
                    minute: 0,
                },
                secs(FRIDAY_NOON),
            )
            .unwrap();
        let expected = ymd_hms_to_unix(2026, 9, 1, 0, 0);
        assert_eq!(record.next_run_unix_secs, Some(expected));
    }

    #[test]
    fn monthly_clamps_short_month() {
        let mut store = ScheduleStore::new();
        // 2026-08-07 noon; day 31 has not happened yet this month -> Aug 31.
        let record = store
            .save_schedule(
                "acct-1",
                ScheduleKind::Monthly {
                    day: 31,
                    hour: 0,
                    minute: 0,
                },
                secs(FRIDAY_NOON),
            )
            .unwrap();
        assert_eq!(
            record.next_run_unix_secs,
            Some(ymd_hms_to_unix(2026, 8, 31, 0, 0))
        );

        // Now roll past August 31st and ask again from within September: the
        // clamp for day 31 in a 30-day month lands on Sept 30.
        let sept_after_31st = ymd_hms_to_unix(2026, 9, 1, 0, 0);
        let record = store
            .save_schedule(
                "acct-1",
                ScheduleKind::Monthly {
                    day: 31,
                    hour: 0,
                    minute: 0,
                },
                secs(sept_after_31st),
            )
            .unwrap();
        assert_eq!(
            record.next_run_unix_secs,
            Some(ymd_hms_to_unix(2026, 9, 30, 0, 0))
        );
    }

    #[test]
    fn only_when_chosen_has_no_next_run() {
        let mut store = ScheduleStore::new();
        let record = store
            .save_schedule("acct-1", ScheduleKind::OnlyWhenChosen, secs(FRIDAY_NOON))
            .unwrap();
        assert_eq!(record.next_run_unix_secs, None);
    }

    #[test]
    fn one_schedule_of_each_type_direct_commands() {
        let mut store = ScheduleStore::new();
        let now = FRIDAY_NOON;

        let daily = run_schedule_command(
            &mut store,
            SCHEDULE_SAVE_COMMAND,
            &json!({
                "accountId": "acct-daily",
                "kind": "daily",
                "hour": 18,
                "minute": 0,
                "nowUnixSecs": now,
            })
            .to_string(),
        );
        let weekly = run_schedule_command(
            &mut store,
            SCHEDULE_SAVE_COMMAND,
            &json!({
                "accountId": "acct-weekly",
                "kind": "weekly",
                "weekday": "mon",
                "hour": 9,
                "minute": 0,
                "nowUnixSecs": now,
            })
            .to_string(),
        );
        let monthly = run_schedule_command(
            &mut store,
            SCHEDULE_SAVE_COMMAND,
            &json!({
                "accountId": "acct-monthly",
                "kind": "monthly",
                "day": 20,
                "hour": 0,
                "minute": 0,
                "nowUnixSecs": now,
            })
            .to_string(),
        );
        let only_when_chosen = run_schedule_command(
            &mut store,
            SCHEDULE_SAVE_COMMAND,
            &json!({
                "accountId": "acct-manual",
                "kind": "only_when_chosen",
                "nowUnixSecs": now,
            })
            .to_string(),
        );

        let daily: serde_json::Value = serde_json::from_str(&daily).unwrap();
        let weekly: serde_json::Value = serde_json::from_str(&weekly).unwrap();
        let monthly: serde_json::Value = serde_json::from_str(&monthly).unwrap();
        let only_when_chosen: serde_json::Value = serde_json::from_str(&only_when_chosen).unwrap();

        assert_eq!(daily["ok"], true);
        assert_eq!(
            daily["schedule"]["nextRunUnixSecs"],
            ymd_hms_to_unix(2026, 8, 7, 18, 0)
        );
        assert_eq!(weekly["ok"], true);
        assert_eq!(
            weekly["schedule"]["nextRunUnixSecs"],
            ymd_hms_to_unix(2026, 8, 10, 9, 0)
        );
        assert_eq!(monthly["ok"], true);
        assert_eq!(
            monthly["schedule"]["nextRunUnixSecs"],
            ymd_hms_to_unix(2026, 8, 20, 0, 0)
        );
        assert_eq!(only_when_chosen["ok"], true);
        assert_eq!(only_when_chosen["schedule"]["nextRunUnixSecs"], serde_json::Value::Null);

        let list = run_schedule_command(&mut store, SCHEDULE_LIST_COMMAND, "{}");
        let list: serde_json::Value = serde_json::from_str(&list).unwrap();
        assert_eq!(list["count"], 4);
    }

    #[test]
    fn rejects_invalid_account_id() {
        let mut store = ScheduleStore::new();
        let err = store
            .save_schedule("", ScheduleKind::OnlyWhenChosen, secs(FRIDAY_NOON))
            .unwrap_err();
        assert_eq!(err, ScheduleError::InvalidAccountId);
    }

    #[test]
    fn rejects_invalid_time() {
        let mut store = ScheduleStore::new();
        let err = store
            .save_schedule(
                "acct-1",
                ScheduleKind::Daily { hour: 24, minute: 0 },
                secs(FRIDAY_NOON),
            )
            .unwrap_err();
        assert_eq!(err, ScheduleError::InvalidTime);
    }

    #[test]
    fn rejects_invalid_day() {
        let mut store = ScheduleStore::new();
        let err = store
            .save_schedule(
                "acct-1",
                ScheduleKind::Monthly {
                    day: 0,
                    hour: 0,
                    minute: 0,
                },
                secs(FRIDAY_NOON),
            )
            .unwrap_err();
        assert_eq!(err, ScheduleError::InvalidDay);
    }

    #[test]
    fn save_replaces_prior_schedule_for_same_account() {
        let mut store = ScheduleStore::new();
        store
            .save_schedule(
                "acct-1",
                ScheduleKind::Daily { hour: 6, minute: 0 },
                secs(FRIDAY_NOON),
            )
            .unwrap();
        store
            .save_schedule("acct-1", ScheduleKind::OnlyWhenChosen, secs(FRIDAY_NOON))
            .unwrap();
        assert_eq!(store.len(), 1);
        let record = store.get("acct-1").unwrap();
        assert_eq!(record.kind, ScheduleKind::OnlyWhenChosen);
    }

    #[test]
    fn get_missing_schedule_is_not_found() {
        let store = ScheduleStore::new();
        let err = store.get("nope").unwrap_err();
        assert_eq!(err, ScheduleError::NotFound);
    }

    #[test]
    fn unknown_command_is_refused() {
        let mut store = ScheduleStore::new();
        let reply = run_schedule_command(&mut store, "not_a_command", "{}");
        let reply: serde_json::Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(reply["ok"], false);
        assert_eq!(reply["errorCode"], UNKNOWN_COMMAND);
    }

    #[test]
    fn civil_roundtrip_matches_known_date() {
        // 2026-08-07 is a Friday; sanity-check the Hinnant algorithm and the
        // weekday helper against it directly.
        let days = days_from_civil(2026, 8, 7);
        assert_eq!(civil_from_days(days), (2026, 8, 7));
        assert_eq!(weekday_index_from_days(days), Weekday::Fri.to_index());
    }
}
