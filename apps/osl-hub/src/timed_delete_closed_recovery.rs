//! Recovery of a timed carrier deletion after OSL was closed at its due time.
//!
//! This policy deliberately owns no delete gesture.  It asks the same
//! carrier-specific action used by the timed-delete sweep to refresh the
//! provider row, and only then asks that action to delete the exact provider
//! target.  The persisted caller keeps [`ClosedDueRecord`] after every outcome.
//! In particular, a failed no-limit carrier action is *not* a reason to drop a
//! job merely because an earlier attempt failed.

use std::fmt;

pub const MAX_RECOVERY_LATENESS_SECONDS: i64 = 900;
pub const COULD_NOT_DELETE_WARNING: &str =
    "OSL could not delete this timed message before the carrier's deletion limit.";

/// The four carrier actions shipping with timed deletion.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum TimedDeleteCarrier {
    Discord,
    Telegram,
    Signal,
    WhatsApp,
}

impl TimedDeleteCarrier {
    pub const ALL: [Self; 4] = [Self::Discord, Self::Telegram, Self::Signal, Self::WhatsApp];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Discord => "discord",
            Self::Telegram => "telegram",
            Self::Signal => "signal",
            Self::WhatsApp => "whatsapp",
        }
    }
}

impl fmt::Display for TimedDeleteCarrier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Evidence obtained from the carrier's own published/documented limit, not a
/// local timeout or retry counter.  A carrier that has no finite deletion age
/// must say so explicitly; it is never assigned a made-up positive ceiling.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CarrierDeletionLimit {
    Finite {
        delete_not_after_unix_seconds: i64,
        source: String,
    },
    NoFiniteLimit {
        source: String,
    },
}

impl CarrierDeletionLimit {
    pub fn source(&self) -> &str {
        match self {
            Self::Finite { source, .. } | Self::NoFiniteLimit { source } => source,
        }
    }

    fn validate(&self) -> Result<(), String> {
        if self.source().trim().is_empty() {
            return Err("carrier deletion-limit source is required".to_owned());
        }
        if matches!(self, Self::Finite { delete_not_after_unix_seconds, .. } if *delete_not_after_unix_seconds < 0)
        {
            return Err("carrier finite deletion limit is invalid".to_owned());
        }
        Ok(())
    }
}

/// A target and a nearby non-target read directly from the provider.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderMessageRead {
    pub exists: bool,
    pub bytes: Vec<u8>,
}

/// Durable state for a record which missed its due time while OSL was absent.
/// `keep_bytes` is the provider read made before arming; recovery compares it
/// byte-for-byte before and after every action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClosedDueRecord {
    pub carrier: TimedDeleteCarrier,
    pub provider_target_id: String,
    pub provider_keep_id: String,
    pub keep_bytes: Vec<u8>,
    pub due_at_unix_seconds: i64,
    pub osl_process_absent_observed_at_unix_seconds: i64,
    pub carrier_unavailable_observed_at_unix_seconds: i64,
    pub state: ClosedDueState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClosedDueState {
    Armed,
    Deleted {
        deleted_at_unix_seconds: i64,
    },
    CouldNotDeleteFiniteLimit {
        observed_at_unix_seconds: i64,
        limit: CarrierDeletionLimit,
        warning: String,
    },
    ProviderConfirmedAbsent {
        observed_at_unix_seconds: i64,
    },
}

impl ClosedDueRecord {
    fn validate(&self) -> Result<(), String> {
        if self.provider_target_id.trim().is_empty() || self.provider_keep_id.trim().is_empty() {
            return Err("provider target and keep identities are required".to_owned());
        }
        if self.provider_target_id == self.provider_keep_id {
            return Err("provider target and keep identities must differ".to_owned());
        }
        if self.due_at_unix_seconds < 0
            || self.osl_process_absent_observed_at_unix_seconds > self.due_at_unix_seconds
            || self.carrier_unavailable_observed_at_unix_seconds > self.due_at_unix_seconds
        {
            return Err("closed-due record has invalid due/absence observations".to_owned());
        }
        Ok(())
    }
}

/// The exact carrier-specific boundary.  Implementations must call the
/// provider's normal delete surface; no sender-only cache or local counter can
/// implement these reads.
pub trait ClosedDueCarrierAction {
    fn carrier(&self) -> TimedDeleteCarrier;
    fn independently_sourced_deletion_limit(&mut self) -> Result<CarrierDeletionLimit, String>;
    fn refresh_provider_message(
        &mut self,
        provider_id: &str,
    ) -> Result<ProviderMessageRead, String>;
    fn delete_provider_target(&mut self, provider_target_id: &str) -> Result<(), String>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClosedDueRecoveryOutcome {
    AlreadyTerminal,
    ProviderConfirmedAbsent,
    CouldNotDeleteFiniteLimit {
        limit: CarrierDeletionLimit,
        warning: String,
    },
    RetryArmed {
        reason: String,
        limit: CarrierDeletionLimit,
    },
    Deleted {
        deleted_at_unix_seconds: i64,
        reference_at_unix_seconds: i64,
        lateness_seconds: i64,
    },
    DeletedLate {
        deleted_at_unix_seconds: i64,
        reference_at_unix_seconds: i64,
        lateness_seconds: i64,
    },
}

fn assert_keep_exact(record: &ClosedDueRecord, keep: ProviderMessageRead) -> Result<(), String> {
    if !keep.exists || keep.bytes != record.keep_bytes {
        return Err(format!(
            "{} keep target {} changed during closed-due recovery",
            record.carrier, record.provider_keep_id
        ));
    }
    Ok(())
}

/// Run one post-reopen/post-availability recovery attempt.
///
/// `next_available_at_unix_seconds` is independently observed at the carrier
/// boundary.  It, rather than a local retry count, is the lateness reference
/// for a carrier that was unavailable at due time.
pub fn recover_closed_due(
    record: &mut ClosedDueRecord,
    next_available_at_unix_seconds: i64,
    recovery_attempt_at_unix_seconds: i64,
    action: &mut dyn ClosedDueCarrierAction,
) -> Result<ClosedDueRecoveryOutcome, String> {
    record.validate()?;
    if next_available_at_unix_seconds < record.due_at_unix_seconds {
        return Err("independent next-available observation predates the due time".to_owned());
    }
    if recovery_attempt_at_unix_seconds < next_available_at_unix_seconds {
        return Err(
            "closed-due recovery attempt predates the independent availability observation"
                .to_owned(),
        );
    }
    if action.carrier() != record.carrier {
        return Err(format!(
            "carrier action mismatch: record={} action={}",
            record.carrier,
            action.carrier()
        ));
    }
    if !matches!(record.state, ClosedDueState::Armed) {
        return Ok(ClosedDueRecoveryOutcome::AlreadyTerminal);
    }

    // Refresh both records before looking at eligibility or issuing a delete.
    // An already-absent provider target is a provider-confirmed terminal state,
    // never an invented successful deletion.
    let target_before = action.refresh_provider_message(&record.provider_target_id)?;
    assert_keep_exact(
        record,
        action.refresh_provider_message(&record.provider_keep_id)?,
    )?;
    if !target_before.exists {
        record.state = ClosedDueState::ProviderConfirmedAbsent {
            observed_at_unix_seconds: next_available_at_unix_seconds,
        };
        return Ok(ClosedDueRecoveryOutcome::ProviderConfirmedAbsent);
    }

    let limit = action.independently_sourced_deletion_limit()?;
    limit.validate()?;
    if matches!(&limit, CarrierDeletionLimit::Finite { delete_not_after_unix_seconds, .. } if recovery_attempt_at_unix_seconds > *delete_not_after_unix_seconds)
    {
        let warning = format!(
            "{COULD_NOT_DELETE_WARNING} Carrier: {}; limit: {}; source: {}",
            record.carrier,
            match &limit {
                CarrierDeletionLimit::Finite {
                    delete_not_after_unix_seconds,
                    ..
                } => delete_not_after_unix_seconds,
                CarrierDeletionLimit::NoFiniteLimit { .. } => unreachable!(),
            },
            limit.source(),
        );
        record.state = ClosedDueState::CouldNotDeleteFiniteLimit {
            observed_at_unix_seconds: recovery_attempt_at_unix_seconds,
            limit: limit.clone(),
            warning: warning.clone(),
        };
        return Ok(ClosedDueRecoveryOutcome::CouldNotDeleteFiniteLimit { limit, warning });
    }

    if let Err(reason) = action.delete_provider_target(&record.provider_target_id) {
        // Crucially no-limit actions remain armed.  Finite-limit actions do too
        // until the carrier's sourced deadline has actually passed.
        return Ok(ClosedDueRecoveryOutcome::RetryArmed { reason, limit });
    }
    let target_after = action.refresh_provider_message(&record.provider_target_id)?;
    assert_keep_exact(
        record,
        action.refresh_provider_message(&record.provider_keep_id)?,
    )?;
    if target_after.exists {
        return Ok(ClosedDueRecoveryOutcome::RetryArmed {
            reason: "carrier accepted delete but provider refresh still finds the target"
                .to_owned(),
            limit,
        });
    }

    record.state = ClosedDueState::Deleted {
        deleted_at_unix_seconds: recovery_attempt_at_unix_seconds,
    };
    let lateness_seconds = recovery_attempt_at_unix_seconds - next_available_at_unix_seconds;
    let outcome = ClosedDueRecoveryOutcome::Deleted {
        deleted_at_unix_seconds: recovery_attempt_at_unix_seconds,
        reference_at_unix_seconds: next_available_at_unix_seconds,
        lateness_seconds,
    };
    if lateness_seconds > MAX_RECOVERY_LATENESS_SECONDS {
        return Ok(ClosedDueRecoveryOutcome::DeletedLate {
            deleted_at_unix_seconds: recovery_attempt_at_unix_seconds,
            reference_at_unix_seconds: next_available_at_unix_seconds,
            lateness_seconds,
        });
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct Carrier {
        carrier: TimedDeleteCarrier,
        limit: CarrierDeletionLimit,
        messages: BTreeMap<String, ProviderMessageRead>,
        failures_left: usize,
        deletes: Vec<String>,
    }

    impl ClosedDueCarrierAction for Carrier {
        fn carrier(&self) -> TimedDeleteCarrier {
            self.carrier
        }
        fn independently_sourced_deletion_limit(&mut self) -> Result<CarrierDeletionLimit, String> {
            Ok(self.limit.clone())
        }
        fn refresh_provider_message(&mut self, id: &str) -> Result<ProviderMessageRead, String> {
            self.messages
                .get(id)
                .cloned()
                .ok_or_else(|| format!("provider omitted {id}"))
        }
        fn delete_provider_target(&mut self, id: &str) -> Result<(), String> {
            self.deletes.push(id.to_owned());
            if self.failures_left > 0 {
                self.failures_left -= 1;
                return Err("carrier temporarily unavailable".to_owned());
            }
            let message = self
                .messages
                .get_mut(id)
                .ok_or_else(|| format!("provider omitted {id}"))?;
            message.exists = false;
            Ok(())
        }
    }

    fn record(carrier: TimedDeleteCarrier) -> ClosedDueRecord {
        let name = carrier.as_str();
        ClosedDueRecord {
            carrier,
            provider_target_id: format!("{name}-provider-target-3315"),
            provider_keep_id: format!("{name}-provider-keep-3315"),
            keep_bytes: format!("{name} keep bytes remain exact").into_bytes(),
            due_at_unix_seconds: 2_000,
            osl_process_absent_observed_at_unix_seconds: 1_990,
            carrier_unavailable_observed_at_unix_seconds: 1_995,
            state: ClosedDueState::Armed,
        }
    }

    fn carrier_for(record: &ClosedDueRecord, limit: CarrierDeletionLimit) -> Carrier {
        Carrier {
            carrier: record.carrier,
            limit,
            messages: BTreeMap::from([
                (
                    record.provider_target_id.clone(),
                    ProviderMessageRead {
                        exists: true,
                        bytes: b"target".to_vec(),
                    },
                ),
                (
                    record.provider_keep_id.clone(),
                    ProviderMessageRead {
                        exists: true,
                        bytes: record.keep_bytes.clone(),
                    },
                ),
            ]),
            failures_left: 0,
            deletes: Vec::new(),
        }
    }

    #[test]
    fn all_four_carriers_recover_their_own_target_once_and_preserve_keep_bytes() {
        for carrier_name in TimedDeleteCarrier::ALL {
            let mut record = record(carrier_name);
            let mut action = carrier_for(
                &record,
                CarrierDeletionLimit::NoFiniteLimit {
                    source: format!("{carrier_name} published deletion policy revision 3315"),
                },
            );
            let result = recover_closed_due(&mut record, 2_010, 2_017, &mut action).unwrap();
            assert_eq!(
                result,
                ClosedDueRecoveryOutcome::Deleted {
                    deleted_at_unix_seconds: 2_017,
                    reference_at_unix_seconds: 2_010,
                    lateness_seconds: 7
                }
            );
            assert_eq!(action.deletes, [record.provider_target_id.clone()]);
            assert_eq!(
                action.messages[&record.provider_keep_id].bytes,
                record.keep_bytes
            );
            assert!(action.messages[&record.provider_keep_id].exists);
            assert!(!action.messages[&record.provider_target_id].exists);
            assert_eq!(
                recover_closed_due(&mut record, 2_011, 2_011, &mut action).unwrap(),
                ClosedDueRecoveryOutcome::AlreadyTerminal
            );
            assert_eq!(
                action.deletes.len(),
                1,
                "{carrier_name} deleted more than once"
            );
        }
    }

    #[test]
    fn finite_expiry_warns_without_claiming_or_attempting_success() {
        let mut record = record(TimedDeleteCarrier::Telegram);
        let mut action = carrier_for(
            &record,
            CarrierDeletionLimit::Finite {
                delete_not_after_unix_seconds: 2_005,
                source: "Telegram independently recorded finite deletion policy".to_owned(),
            },
        );
        let result = recover_closed_due(&mut record, 2_010, 2_010, &mut action).unwrap();
        match result {
            ClosedDueRecoveryOutcome::CouldNotDeleteFiniteLimit { limit, warning } => {
                assert!(matches!(
                    limit,
                    CarrierDeletionLimit::Finite {
                        delete_not_after_unix_seconds: 2_005,
                        ..
                    }
                ));
                assert!(warning.contains(COULD_NOT_DELETE_WARNING));
                assert!(warning.contains("Telegram independently recorded finite deletion policy"));
            }
            other => panic!("expected finite-limit warning, got {other:?}"),
        }
        assert!(action.deletes.is_empty());
        assert!(matches!(
            record.state,
            ClosedDueState::CouldNotDeleteFiniteLimit { .. }
        ));
    }

    #[test]
    fn no_limit_failure_stays_armed_until_provider_confirms_deletion() {
        let mut record = record(TimedDeleteCarrier::Signal);
        let mut action = carrier_for(
            &record,
            CarrierDeletionLimit::NoFiniteLimit {
                source: "Signal independently recorded: no finite delete age".to_owned(),
            },
        );
        action.failures_left = 2;
        for retry_at in [2_010, 2_020] {
            assert!(matches!(
                recover_closed_due(&mut record, retry_at, retry_at, &mut action).unwrap(),
                ClosedDueRecoveryOutcome::RetryArmed { .. }
            ));
            assert_eq!(record.state, ClosedDueState::Armed);
        }
        assert!(matches!(
            recover_closed_due(&mut record, 2_030, 2_030, &mut action).unwrap(),
            ClosedDueRecoveryOutcome::Deleted { .. }
        ));
        assert_eq!(action.deletes, vec![record.provider_target_id.clone(); 3]);
    }

    #[test]
    fn action_for_a_different_carrier_is_rejected_before_any_provider_call() {
        let mut record = record(TimedDeleteCarrier::Discord);
        let mut action = carrier_for(
            &record,
            CarrierDeletionLimit::NoFiniteLimit {
                source: "Signal source".to_owned(),
            },
        );
        action.carrier = TimedDeleteCarrier::Signal;
        assert!(recover_closed_due(&mut record, 2_010, 2_010, &mut action)
            .unwrap_err()
            .contains("carrier action mismatch"));
        assert!(action.deletes.is_empty());
    }

    #[test]
    fn a_recovery_later_than_the_advertised_window_never_reads_as_timely() {
        let mut record = record(TimedDeleteCarrier::WhatsApp);
        let mut action = carrier_for(
            &record,
            CarrierDeletionLimit::NoFiniteLimit {
                source: "WhatsApp independently recorded: no finite delete age".to_owned(),
            },
        );
        let result = recover_closed_due(&mut record, 2_010, 2_911, &mut action).unwrap();
        assert_eq!(
            result,
            ClosedDueRecoveryOutcome::DeletedLate {
                deleted_at_unix_seconds: 2_911,
                reference_at_unix_seconds: 2_010,
                lateness_seconds: 901,
            }
        );
        assert_eq!(action.deletes, [record.provider_target_id.clone()]);
    }
}
