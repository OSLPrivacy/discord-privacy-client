//! Provider-neutral cleanup receipt projection.
//!
//! A requested delete is not a verified delete. The only statuses this module
//! can project are the post-action verification result: gone, still present, or
//! unknown.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrubReceiptStatus {
    VerifiedGone,
    StillPresent,
    Unknown,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScrubReceipt {
    pub item_ordinal: u16,
    pub status: ScrubReceiptStatus,
    pub verified_at_unix_ms: Option<u64>,
}

impl ScrubReceipt {
    pub const fn verified_gone(item_ordinal: u16, verified_at_unix_ms: u64) -> Self {
        Self {
            item_ordinal,
            status: ScrubReceiptStatus::VerifiedGone,
            verified_at_unix_ms: Some(verified_at_unix_ms),
        }
    }

    pub const fn still_present(item_ordinal: u16, verified_at_unix_ms: u64) -> Self {
        Self {
            item_ordinal,
            status: ScrubReceiptStatus::StillPresent,
            verified_at_unix_ms: Some(verified_at_unix_ms),
        }
    }

    pub const fn unknown(item_ordinal: u16) -> Self {
        Self {
            item_ordinal,
            status: ScrubReceiptStatus::Unknown,
            verified_at_unix_ms: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ScrubReceiptProjectionError {
    MissingVerificationTime,
    UnknownMustNotClaimVerificationTime,
}

pub fn project_scrub_receipts(
    receipts: impl IntoIterator<Item = ScrubReceipt>,
) -> Result<Vec<ScrubReceipt>, ScrubReceiptProjectionError> {
    let mut projected = Vec::new();
    for receipt in receipts {
        match (receipt.status, receipt.verified_at_unix_ms) {
            (ScrubReceiptStatus::VerifiedGone | ScrubReceiptStatus::StillPresent, None) => {
                return Err(ScrubReceiptProjectionError::MissingVerificationTime)
            }
            (ScrubReceiptStatus::Unknown, Some(_)) => {
                return Err(ScrubReceiptProjectionError::UnknownMustNotClaimVerificationTime)
            }
            _ => projected.push(receipt),
        }
    }
    projected.sort_by_key(|receipt| receipt.item_ordinal);
    Ok(projected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_receipt_projects_verified_gone_still_present_and_unknown_statuses() {
        let projected = project_scrub_receipts([
            ScrubReceipt::unknown(3),
            ScrubReceipt::still_present(2, 1_785_283_199_000),
            ScrubReceipt::verified_gone(1, 1_785_283_198_000),
        ])
        .expect("valid receipt statuses project");

        assert_eq!(
            projected,
            [
                ScrubReceipt::verified_gone(1, 1_785_283_198_000),
                ScrubReceipt::still_present(2, 1_785_283_199_000),
                ScrubReceipt::unknown(3),
            ]
        );
        assert_eq!(
            project_scrub_receipts([ScrubReceipt {
                item_ordinal: 4,
                status: ScrubReceiptStatus::VerifiedGone,
                verified_at_unix_ms: None,
            }]),
            Err(ScrubReceiptProjectionError::MissingVerificationTime)
        );
        assert_eq!(
            project_scrub_receipts([ScrubReceipt {
                item_ordinal: 5,
                status: ScrubReceiptStatus::Unknown,
                verified_at_unix_ms: Some(1),
            }]),
            Err(ScrubReceiptProjectionError::UnknownMustNotClaimVerificationTime)
        );
    }
}
