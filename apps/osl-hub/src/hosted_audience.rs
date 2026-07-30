//! Audience ordering for hosted scans.
//!
//! Wider audiences must be considered before narrower ones so a hosted scan
//! cannot stop at a direct-message scope while public-server or group-visible
//! material is still in play.

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum HostedAudience {
    PublicServer,
    Group,
    Direct,
}

impl HostedAudience {
    pub const fn widest_first_rank(self) -> u8 {
        match self {
            Self::PublicServer => 0,
            Self::Group => 1,
            Self::Direct => 2,
        }
    }
}

pub fn audiences_widest_first(
    audiences: impl IntoIterator<Item = HostedAudience>,
) -> Vec<HostedAudience> {
    let mut ordered: Vec<HostedAudience> = audiences.into_iter().collect();
    ordered.sort_by_key(|audience| audience.widest_first_rank());
    ordered.dedup();
    ordered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosted_audience_widest_first_orders_public_server_group_direct() {
        let ordered = audiences_widest_first([
            HostedAudience::Direct,
            HostedAudience::Group,
            HostedAudience::PublicServer,
            HostedAudience::Direct,
        ]);

        assert_eq!(
            ordered,
            vec![
                HostedAudience::PublicServer,
                HostedAudience::Group,
                HostedAudience::Direct,
            ]
        );
        assert!(
            HostedAudience::PublicServer.widest_first_rank()
                < HostedAudience::Group.widest_first_rank()
        );
        assert!(
            HostedAudience::Group.widest_first_rank() < HostedAudience::Direct.widest_first_rank()
        );
    }
}
