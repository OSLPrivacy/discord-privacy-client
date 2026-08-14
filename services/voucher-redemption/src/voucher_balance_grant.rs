//! Mixing-queue projection for a redeemed voucher.  It retains only aud, exp, jti.

pub const MIXING_QUEUE_VOUCHER_GRANT_FIELDS: [&str; 3] = ["aud", "exp", "jti"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MixingQueueVoucherGrant {
    pub aud: &'static str,
    pub exp: u64,
    pub jti: String,
}

impl MixingQueueVoucherGrant {
    pub fn new(exp: u64, jti: String) -> Result<Self, &'static str> {
        if exp == 0 || jti.len() < 16 || !jti.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-') {
            return Err("voucher grant refused");
        }
        Ok(Self { aud: "osl-capacity-voucher", exp, jti })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_grant_has_only_the_three_relay_fields() {
        let grant = MixingQueueVoucherGrant::new(1, "voucher_jti_00001".into()).unwrap();
        assert_eq!(MIXING_QUEUE_VOUCHER_GRANT_FIELDS, ["aud", "exp", "jti"]);
        assert_eq!(grant.aud, "osl-capacity-voucher");
    }
}
