//! Local, honest tracking for statutory erasure requests.
//!
//! Sending a request is evidence only that the request was sent.  It is never
//! evidence that the provider deleted anything; that remains `Unknown` until a
//! later re-scan supplies verification.

#[cfg(not(test))]
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg_attr(not(test), derive(Deserialize, Serialize))]
#[cfg_attr(not(test), serde(rename_all = "snake_case"))]
pub enum ErasureVerification {
    Unknown,
    VerifiedGone,
    StillPresent,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg_attr(not(test), derive(Deserialize, Serialize))]
#[cfg_attr(not(test), serde(rename_all = "snake_case"))]
pub enum ErasureRequestPhase {
    RequestedAwaitingResponse,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
#[cfg_attr(not(test), derive(Deserialize, Serialize))]
#[cfg_attr(not(test), serde(deny_unknown_fields))]
pub struct CivilDate {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ErasureTrackerError {
    InvalidDate,
    ExtensionAlreadyRecorded,
}

impl CivilDate {
    pub fn new(year: i32, month: u8, day: u8) -> Result<Self, ErasureTrackerError> {
        let date = Self { year, month, day };
        if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return Err(ErasureTrackerError::InvalidDate);
        }
        Ok(date)
    }

    /// Adds calendar months, clamping to the final day of a shorter month.
    pub fn add_months(self, months: u8) -> Self {
        let zero_based_month = i32::from(self.month - 1) + i32::from(months);
        let year = self.year + zero_based_month.div_euclid(12);
        let month = zero_based_month.rem_euclid(12) as u8 + 1;
        Self {
            year,
            month,
            day: self.day.min(days_in_month(year, month)),
        }
    }
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year.rem_euclid(4) == 0
            && (year.rem_euclid(100) != 0 || year.rem_euclid(400) == 0) =>
        {
            29
        }
        2 => 28,
        _ => 0,
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(not(test), derive(Deserialize, Serialize))]
#[cfg_attr(not(test), serde(deny_unknown_fields))]
pub struct ErasureRequestTracking {
    pub request_id: String,
    pub requested_on: CivilDate,
    pub response_due_on: CivilDate,
    pub phase: ErasureRequestPhase,
    pub verification: ErasureVerification,
    extension_recorded: bool,
}

impl ErasureRequestTracking {
    /// Begins tracking after the user has sent the request from their own mailbox.
    pub fn sent(request_id: impl Into<String>, requested_on: CivilDate) -> Self {
        Self {
            request_id: request_id.into(),
            requested_on,
            response_due_on: requested_on.add_months(1),
            phase: ErasureRequestPhase::RequestedAwaitingResponse,
            verification: ErasureVerification::Unknown,
            extension_recorded: false,
        }
    }

    /// Records timely notice of GDPR Art. 12(3)'s up-to-two-month extension.
    pub fn record_extension_notice(&mut self) -> Result<(), ErasureTrackerError> {
        if self.extension_recorded {
            return Err(ErasureTrackerError::ExtensionAlreadyRecorded);
        }
        self.response_due_on = self.requested_on.add_months(3);
        self.extension_recorded = true;
        Ok(())
    }

    /// A provider request never establishes deletion; only a re-scan may do so.
    pub fn record_rescan(&mut self, verification: ErasureVerification) {
        self.verification = verification;
    }

    pub fn status_line(&self) -> String {
        format!(
            "requested, awaiting response, due {:04}-{:02}-{:02}",
            self.response_due_on.year, self.response_due_on.month, self.response_due_on.day
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sent_request_is_awaiting_response_unknown_until_rescan_and_has_one_month_due_date() {
        let sent_on = CivilDate::new(2028, 1, 31).unwrap();
        let tracking = ErasureRequestTracking::sent("request-42", sent_on);

        assert_eq!(
            tracking.phase,
            ErasureRequestPhase::RequestedAwaitingResponse
        );
        assert_eq!(tracking.verification, ErasureVerification::Unknown);
        assert_eq!(
            tracking.response_due_on,
            CivilDate::new(2028, 2, 29).unwrap()
        );
        assert_eq!(
            tracking.status_line(),
            "requested, awaiting response, due 2028-02-29"
        );
    }

    #[test]
    fn extension_is_representable_and_rescan_is_the_only_route_to_verified_gone() {
        let mut tracking =
            ErasureRequestTracking::sent("request-43", CivilDate::new(2027, 11, 30).unwrap());

        tracking.record_extension_notice().unwrap();
        assert_eq!(
            tracking.response_due_on,
            CivilDate::new(2028, 2, 29).unwrap()
        );
        assert_eq!(tracking.verification, ErasureVerification::Unknown);
        assert_eq!(
            tracking.record_extension_notice(),
            Err(ErasureTrackerError::ExtensionAlreadyRecorded)
        );

        tracking.record_rescan(ErasureVerification::VerifiedGone);
        assert_eq!(tracking.verification, ErasureVerification::VerifiedGone);
    }
}
