//! Audience ordering for hosted-session capabilities.
//!
//! The order is intentionally widest first so any policy projection can stop at
//! the first matching audience without accidentally granting a narrower direct
//! audience before a public-server group rule has been considered.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HostedAudience {
    PublicServerGroupDirect,
    PublicServer,
    GroupDirect,
    Direct,
}

impl HostedAudience {
    pub const fn width_rank(self) -> u8 {
        match self {
            Self::PublicServerGroupDirect => 0,
            Self::PublicServer => 1,
            Self::GroupDirect => 2,
            Self::Direct => 3,
        }
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
            HostedAudience::PublicServer,
            HostedAudience::PublicServerGroupDirect,
            HostedAudience::GroupDirect,
            HostedAudience::PublicServer,
        ]);

        assert_eq!(
            ordered,
            [
                HostedAudience::PublicServerGroupDirect,
                HostedAudience::PublicServer,
                HostedAudience::GroupDirect,
                HostedAudience::Direct,
            ]
        );
        assert_eq!(HostedAudience::PublicServerGroupDirect.width_rank(), 0);
    }
}
