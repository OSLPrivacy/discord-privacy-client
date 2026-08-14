//! Durable retry of one Discord message deletion when Discord is closed.
//!
//! A timer persists Discord's *single-message* target before the first delete
//! is due.  A closed client therefore cannot turn a due delete into a
//! count-only operation, a refreshed search, or an abandoned job.  The retry
//! window is derived solely from the carrier eligibility evidence: Discord's
//! documented single-message delete operation currently has no finite age
//! limit, so the shipping record uses [`CarrierEligibility::NoFiniteLimit`].
//!
//! This module intentionally has no positive retry-lifetime constant.  Backoff
//! is bounded to protect the local machine; lifetime is not bounded unless the
//! independently frozen carrier evidence supplies a finite deadline.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

pub const DISCORD_SINGLE_DELETE_SOURCE_ID: &str =
    "discord-message-resource-single-delete-2026-08-13";
pub const DISCORD_BOUNDARY_PROBE_ID: &str =
    "discord-single-delete-no-finite-limit-probe-2026-08-13";
pub const RETRY_BASE_SECONDS: i64 = 5;
pub const RETRY_MAX_BACKOFF_SECONDS: i64 = 60;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscordDeleteTarget {
    pub account_id: String,
    pub channel_id: String,
    pub message_id: String,
}

impl DiscordDeleteTarget {
    fn valid(&self) -> bool {
        [&self.account_id, &self.channel_id, &self.message_id]
            .into_iter()
            .all(|value| {
                !value.is_empty()
                    && value.len() <= 256
                    && value.bytes().all(|byte| !byte.is_ascii_control())
            })
    }

    pub fn label(&self) -> String {
        format!(
            "discord/{}/{}/{}",
            self.account_id, self.channel_id, self.message_id
        )
    }
}

/// Eligibility is carrier evidence, never a timer author's preference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CarrierEligibility {
    /// Discord publishes no finite deadline for its one-message delete route.
    NoFiniteLimit {
        source_id: String,
        boundary_probe_id: String,
    },
    /// For a carrier that independently publishes a precise final instant.
    FiniteDeadline { eligible_through_unix_seconds: i64 },
}

impl CarrierEligibility {
    pub fn frozen_discord_no_finite_limit() -> Self {
        Self::NoFiniteLimit {
            source_id: DISCORD_SINGLE_DELETE_SOURCE_ID.to_owned(),
            boundary_probe_id: DISCORD_BOUNDARY_PROBE_ID.to_owned(),
        }
    }

    fn valid_for_target(&self, target: &DiscordDeleteTarget, delete_at: i64) -> bool {
        match self {
            Self::NoFiniteLimit {
                source_id,
                boundary_probe_id,
            } => {
                target.valid()
                    && source_id == DISCORD_SINGLE_DELETE_SOURCE_ID
                    && boundary_probe_id == DISCORD_BOUNDARY_PROBE_ID
            }
            Self::FiniteDeadline {
                eligible_through_unix_seconds,
            } => target.valid() && *eligible_through_unix_seconds >= delete_at,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::NoFiniteLimit { .. } => "no-finite-limit",
            Self::FiniteDeadline { .. } => "finite-deadline",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeleteJobState {
    Armed,
    Retrying,
    Deleted,
    ConfirmedAbsent,
    Missed,
}

impl DeleteJobState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Armed => "armed",
            Self::Retrying => "retrying",
            Self::Deleted => "deleted",
            Self::ConfirmedAbsent => "confirmed_absent",
            Self::Missed => "missed",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "armed" => Self::Armed,
            "retrying" => Self::Retrying,
            "deleted" => Self::Deleted,
            "confirmed_absent" => Self::ConfirmedAbsent,
            "missed" => Self::Missed,
            _ => return None,
        })
    }

    pub fn is_armed(self) -> bool {
        matches!(self, Self::Armed | Self::Retrying)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscordDeleteRetryJob {
    pub target: DiscordDeleteTarget,
    /// Exactly two rows explicitly kept while this target is removed.
    pub keep_message_ids: [String; 2],
    pub delete_at_unix_seconds: i64,
    pub eligibility: CarrierEligibility,
    pub state: DeleteJobState,
    pub retry_count: u32,
    pub next_attempt_unix_seconds: i64,
    pub missed_recorded_at_unix_seconds: Option<i64>,
}

impl DiscordDeleteRetryJob {
    pub fn arm(
        target: DiscordDeleteTarget,
        keep_message_ids: [String; 2],
        delete_at_unix_seconds: i64,
        eligibility: CarrierEligibility,
    ) -> Result<Self, String> {
        if delete_at_unix_seconds < 0
            || !eligibility.valid_for_target(&target, delete_at_unix_seconds)
            || keep_message_ids
                .iter()
                .any(|id| id.is_empty() || id.len() > 256)
            || keep_message_ids[0] == keep_message_ids[1]
            || keep_message_ids.iter().any(|id| id == &target.message_id)
        {
            return Err("OSL Discord delete retry job is invalid".to_owned());
        }
        Ok(Self {
            target,
            keep_message_ids,
            delete_at_unix_seconds,
            eligibility,
            state: DeleteJobState::Armed,
            retry_count: 0,
            next_attempt_unix_seconds: delete_at_unix_seconds,
            missed_recorded_at_unix_seconds: None,
        })
    }

    pub fn visible_status(&self) -> String {
        match self.state {
            DeleteJobState::Armed => format!(
                "Deletion armed for {} ({})",
                self.target.label(),
                self.eligibility.label()
            ),
            DeleteJobState::Retrying => format!(
                "Discord unavailable; retrying exact target {} at {} ({})",
                self.target.label(),
                self.next_attempt_unix_seconds,
                self.eligibility.label()
            ),
            DeleteJobState::Deleted => {
                format!("Deleted exact Discord target {}", self.target.label())
            }
            DeleteJobState::ConfirmedAbsent => format!(
                "Discord confirmed exact target already absent {}",
                self.target.label()
            ),
            DeleteJobState::Missed => format!(
                "Warning: Discord deletion window ended for exact target {}",
                self.target.label()
            ),
        }
    }

    fn validate(&self) -> Result<(), String> {
        if !self
            .eligibility
            .valid_for_target(&self.target, self.delete_at_unix_seconds)
            || self.keep_message_ids[0].is_empty()
            || self.keep_message_ids[1].is_empty()
            || self.keep_message_ids[0] == self.keep_message_ids[1]
            || self
                .keep_message_ids
                .iter()
                .any(|id| id == &self.target.message_id)
            || self.next_attempt_unix_seconds < self.delete_at_unix_seconds
            || (self.state == DeleteJobState::Missed)
                != self.missed_recorded_at_unix_seconds.is_some()
        {
            return Err("OSL Discord delete retry record is malformed".to_owned());
        }
        Ok(())
    }
}

pub trait DiscordExactDeleteProvider {
    /// Delete this exact persisted message, never a result count or a refreshed
    /// search.  The two keep ids are supplied so a provider can independently
    /// reject an overbroad operation.
    fn delete_exact(
        &mut self,
        target: &DiscordDeleteTarget,
        keep_message_ids: &[String; 2],
    ) -> ProviderDeleteResult;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderDeleteResult {
    Deleted,
    ConfirmedAbsent,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchedulerOutcome {
    NotDue,
    Retried {
        next_attempt_unix_seconds: i64,
        retry_count: u32,
    },
    Deleted,
    ConfirmedAbsent,
    Missed {
        recorded_at_unix_seconds: i64,
    },
    Finished,
}

/// Run a persisted job once.  A no-limit job remains armed forever; a finite
/// carrier deadline is the only reason this code can record `Missed`.
pub fn run_due_discord_delete<P: DiscordExactDeleteProvider>(
    job: &mut DiscordDeleteRetryJob,
    now_unix_seconds: i64,
    provider: &mut P,
) -> Result<SchedulerOutcome, String> {
    job.validate()?;
    if !job.state.is_armed() {
        return Ok(SchedulerOutcome::Finished);
    }
    if now_unix_seconds < job.next_attempt_unix_seconds {
        return Ok(SchedulerOutcome::NotDue);
    }
    if let CarrierEligibility::FiniteDeadline {
        eligible_through_unix_seconds,
    } = job.eligibility
    {
        if now_unix_seconds > eligible_through_unix_seconds {
            job.state = DeleteJobState::Missed;
            job.missed_recorded_at_unix_seconds = Some(now_unix_seconds);
            return Ok(SchedulerOutcome::Missed {
                recorded_at_unix_seconds: now_unix_seconds,
            });
        }
    }
    match provider.delete_exact(&job.target, &job.keep_message_ids) {
        ProviderDeleteResult::Deleted => {
            job.state = DeleteJobState::Deleted;
            Ok(SchedulerOutcome::Deleted)
        }
        ProviderDeleteResult::ConfirmedAbsent => {
            job.state = DeleteJobState::ConfirmedAbsent;
            Ok(SchedulerOutcome::ConfirmedAbsent)
        }
        ProviderDeleteResult::Unavailable => {
            job.state = DeleteJobState::Retrying;
            job.retry_count = job.retry_count.saturating_add(1);
            let shift = job.retry_count.saturating_sub(1).min(30);
            let wait = RETRY_BASE_SECONDS
                .saturating_mul(1_i64 << shift)
                .min(RETRY_MAX_BACKOFF_SECONDS);
            job.next_attempt_unix_seconds = now_unix_seconds.saturating_add(wait);
            Ok(SchedulerOutcome::Retried {
                next_attempt_unix_seconds: job.next_attempt_unix_seconds,
                retry_count: job.retry_count,
            })
        }
    }
}

const RECORD_VERSION: &str = "osl-discord-delete-retry-v1";

/// Store with an integrity tag, so a target/keep/provider/eligibility refresh
/// in the durable record is rejected rather than silently becoming a new job.
pub fn store_job_at_path(path: &Path, job: &DiscordDeleteRetryJob) -> Result<(), String> {
    job.validate()?;
    let fields = record_fields(job);
    let body = fields.join("\n");
    let tagged = format!("{body}\n{:016x}\n", integrity_tag(&body));
    let parent = path
        .parent()
        .ok_or_else(|| "OSL Discord delete retry path is invalid".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|_| "OSL Discord delete retry cannot create storage".to_owned())?;
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, tagged)
        .map_err(|_| "OSL Discord delete retry cannot save storage".to_owned())?;
    fs::rename(&temporary, path)
        .map_err(|_| "OSL Discord delete retry cannot commit storage".to_owned())
}

pub fn load_job_at_path(path: &Path) -> Result<DiscordDeleteRetryJob, String> {
    let raw = fs::read_to_string(path)
        .map_err(|_| "OSL Discord delete retry record is unavailable".to_owned())?;
    let lines = raw.lines().collect::<Vec<_>>();
    if lines.len() != 16 || lines[0] != RECORD_VERSION {
        return Err("OSL Discord delete retry record is malformed".to_owned());
    }
    let body = lines[..15].join("\n");
    if u64::from_str_radix(lines[15], 16).ok() != Some(integrity_tag(&body)) {
        return Err("OSL Discord delete retry record integrity check failed".to_owned());
    }
    let eligibility = match lines[7] {
        "no_finite_limit" => CarrierEligibility::NoFiniteLimit {
            source_id: unhex(lines[8])?,
            boundary_probe_id: unhex(lines[9])?,
        },
        "finite_deadline" => CarrierEligibility::FiniteDeadline {
            eligible_through_unix_seconds: parse_i64(lines[8])?,
        },
        _ => return Err("OSL Discord delete retry record is malformed".to_owned()),
    };
    let missed = match lines[14] {
        "-" => None,
        value => Some(parse_i64(value)?),
    };
    let job = DiscordDeleteRetryJob {
        target: DiscordDeleteTarget {
            account_id: unhex(lines[1])?,
            channel_id: unhex(lines[2])?,
            message_id: unhex(lines[3])?,
        },
        keep_message_ids: [unhex(lines[4])?, unhex(lines[5])?],
        delete_at_unix_seconds: parse_i64(lines[6])?,
        eligibility,
        state: DeleteJobState::parse(lines[10])
            .ok_or_else(|| "OSL Discord delete retry record is malformed".to_owned())?,
        retry_count: lines[11]
            .parse()
            .map_err(|_| "OSL Discord delete retry record is malformed".to_owned())?,
        next_attempt_unix_seconds: parse_i64(lines[12])?,
        missed_recorded_at_unix_seconds: missed,
    };
    job.validate()?;
    Ok(job)
}

fn record_fields(job: &DiscordDeleteRetryJob) -> Vec<String> {
    let (kind, source, probe) = match &job.eligibility {
        CarrierEligibility::NoFiniteLimit {
            source_id,
            boundary_probe_id,
        } => ("no_finite_limit", hex(source_id), hex(boundary_probe_id)),
        CarrierEligibility::FiniteDeadline {
            eligible_through_unix_seconds,
        } => (
            "finite_deadline",
            eligible_through_unix_seconds.to_string(),
            "-".to_owned(),
        ),
    };
    vec![
        RECORD_VERSION.to_owned(),
        hex(&job.target.account_id),
        hex(&job.target.channel_id),
        hex(&job.target.message_id),
        hex(&job.keep_message_ids[0]),
        hex(&job.keep_message_ids[1]),
        job.delete_at_unix_seconds.to_string(),
        kind.to_owned(),
        source,
        probe,
        job.state.as_str().to_owned(),
        job.retry_count.to_string(),
        job.next_attempt_unix_seconds.to_string(),
        "provider=discord-single-message-delete".to_owned(),
        job.missed_recorded_at_unix_seconds
            .map_or_else(|| "-".to_owned(), |time| time.to_string()),
    ]
}

fn integrity_tag(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

fn parse_i64(value: &str) -> Result<i64, String> {
    value
        .parse()
        .map_err(|_| "OSL Discord delete retry record is malformed".to_owned())
}

fn hex(value: &str) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(value.len() * 2);
    for byte in value.bytes() {
        out.push(DIGITS[usize::from(byte >> 4)] as char);
        out.push(DIGITS[usize::from(byte & 15)] as char);
    }
    out
}

fn unhex(value: &str) -> Result<String, String> {
    if value.len() % 2 != 0 {
        return Err("OSL Discord delete retry record is malformed".to_owned());
    }
    let bytes = value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| Ok((hex_digit(pair[0])? << 4) | hex_digit(pair[1])?))
        .collect::<Result<Vec<_>, String>>()?;
    String::from_utf8(bytes).map_err(|_| "OSL Discord delete retry record is malformed".to_owned())
}

fn hex_digit(value: u8) -> Result<u8, String> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err("OSL Discord delete retry record is malformed".to_owned()),
    }
}

/// The target and both keep ids as a set are useful to provider adapters that
/// need a deterministic exact-operation assertion.
pub fn protected_message_ids(job: &DiscordDeleteRetryJob) -> BTreeSet<String> {
    [
        job.target.message_id.clone(),
        job.keep_message_ids[0].clone(),
        job.keep_message_ids[1].clone(),
    ]
    .into_iter()
    .collect()
}
