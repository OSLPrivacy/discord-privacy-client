//! License redemption HTTP client.
//!
//! Redemption is deliberately a separate operation from validation: only this
//! call may start a prepaid entitlement period. Validation remains read-only
//! so background refreshes cannot consume a code.

use super::{check_2xx, KeyServerClient};
use crate::Result;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct LicenseRedeemRequest<'a> {
    license_key: &'a str,
}

/// Response body for `POST /v1/license/redeem`.
///
/// The timestamps are optional because `UNKNOWN` and `UNREDEEMED` do not have
/// an entitlement period. A retried redemption returns the original pair.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct LicenseRedeemResponse {
    pub status: String,
    #[serde(default)]
    pub redeemed_at: Option<i64>,
    #[serde(default)]
    pub expires_at: Option<i64>,
    pub checksum_ok: bool,
}

impl KeyServerClient {
    /// Redeem a prepaid license code exactly once.
    ///
    /// This is the only client operation that may start the entitlement clock.
    /// Later checks must use [`KeyServerClient::validate_license`] instead.
    pub fn redeem_license(&self, license_plaintext: &str) -> Result<LicenseRedeemResponse> {
        let body = LicenseRedeemRequest {
            license_key: license_plaintext,
        };
        let body_json = serde_json::to_vec(&body)?;
        let response = self.send_request(
            "POST",
            "/v1/license/redeem",
            Some(("application/json", &body_json)),
        )?;
        check_2xx(&response)?;
        Ok(serde_json::from_slice(&response.body)?)
    }
}
