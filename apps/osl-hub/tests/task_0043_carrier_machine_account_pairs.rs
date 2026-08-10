//! TASK 0043: every active carrier claim must have a unique machine/account pair.

use std::collections::HashSet;

#[derive(Clone, Copy)]
struct CarrierClaim {
    carrier: &'static str,
    machine: &'static str,
    account: &'static str,
}

const CARRIER_CLAIMS: &[CarrierClaim] = &[
    CarrierClaim {
        carrier: "discord-alpha",
        machine: "machine-a",
        account: "discord-account-a",
    },
    CarrierClaim {
        carrier: "discord-beta",
        machine: "machine-b",
        account: "discord-account-b",
    },
    CarrierClaim {
        carrier: "discord-gamma",
        machine: "machine-c",
        account: "discord-account-c",
    },
];

const ROSTER: &[(&str, &str, &str)] = &[
    ("discord-alpha", "machine-a", "discord-account-a"),
    ("discord-beta", "machine-b", "discord-account-b"),
    ("discord-gamma", "machine-c", "discord-account-c"),
];

#[test]
fn every_claim_matches_roster_and_uses_unique_machine_and_account() {
    let mut accounts = HashSet::new();
    let mut machines = HashSet::new();
    let mut discord_pairs = 0;

    assert_eq!(CARRIER_CLAIMS.len(), ROSTER.len(), "claim/roster count mismatch");
    for claim in CARRIER_CLAIMS {
        println!(
            "claimed carrier={} machine={} account={}",
            claim.carrier, claim.machine, claim.account
        );
        assert!(
            ROSTER.contains(&(claim.carrier, claim.machine, claim.account)),
            "carrier claim is absent or differs in roster: {}",
            claim.carrier
        );
        assert!(
            machines.insert(claim.machine),
            "machine reused between carrier claims: {}",
            claim.machine
        );
        assert!(
            accounts.insert(claim.account),
            "account reused between carrier claims: {}",
            claim.account
        );
        if claim.carrier.starts_with("discord-") {
            discord_pairs += 1;
        }
    }

    println!("carrier_claim_count={}", CARRIER_CLAIMS.len());
    println!("account_count={}", accounts.len());
    println!("machine_count={}", machines.len());
    println!("discord_pair_count={discord_pairs}");
    assert_eq!(accounts.len(), CARRIER_CLAIMS.len(), "every account must appear exactly once");
    assert_eq!(machines.len(), CARRIER_CLAIMS.len(), "machines must not overlap");
    assert_eq!(discord_pairs, 3, "Discord must hold three pairs");
}
