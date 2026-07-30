//! Audience ordering for hosted-session capabilities.
//!
//! Wider audiences must be considered before narrower ones so a hosted scan or
//! policy projection cannot grant a direct-message scope while public-server or
//! group-visible material is still in play.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HostedAudience {
    PublicServerGroupDirect,
    PublicServer,
    GroupDirect,
    Group,
    Direct,
}

impl HostedAudience {
    pub const fn width_rank(self) -> u8 {
        match self {
            Self::PublicServerGroupDirect => 0,
            Self::PublicServer => 1,
            Self::GroupDirect => 2,
            Self::Group => 3,
            Self::Direct => 4,
        }
    }

    pub const fn widest_first_rank(self) -> u8 {
        self.width_rank()
    }
}

pub fn audiences_widest_first(
    audiences: impl IntoIterator<Item = HostedAudience>,
) -> Vec<HostedAudience> {
    let mut audiences: Vec<_> = audiences.into_iter().collect();
    audiences.sort_by_key(|audience| audience.width_rank());
    audiences.dedup();
    audiences
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
            HostedAudience::PublicServerGroupDirect,
            HostedAudience::GroupDirect,
            HostedAudience::PublicServer,
            HostedAudience::Direct,
        ]);

        assert_eq!(
            ordered,
            vec![
                HostedAudience::PublicServerGroupDirect,
                HostedAudience::PublicServer,
                HostedAudience::GroupDirect,
                HostedAudience::Group,
                HostedAudience::Direct,
            ]
        );
        assert_eq!(HostedAudience::PublicServerGroupDirect.width_rank(), 0);
        assert!(
            HostedAudience::PublicServer.widest_first_rank()
                < HostedAudience::Group.widest_first_rank()
        );
        assert!(
            HostedAudience::Group.widest_first_rank() < HostedAudience::Direct.widest_first_rank()
        );
    }
}
