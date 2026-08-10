use ipc::metered_bytes::{
    MeteredSendPath, MonthlyAllowanceAccount, TopUpPack, FREE_MONTHLY_ALLOWANCE_BYTES,
    TINY_TEST_TOP_UP_ALLOWANCE_BYTES,
};

const ACCOUNT_RESET: i64 = 1_785_585_600; // 2026-08-01 12:00:00 UTC
const REDEEMED_AT: i64 = 1_786_363_200; // 2026-08-10 12:00:00 UTC
const FREE_SPENT_BYTES: u64 = FREE_MONTHLY_ALLOWANCE_BYTES * 90 / 100;

fn fixture_account() -> MonthlyAllowanceAccount {
    let mut account = MonthlyAllowanceAccount::new(ACCOUNT_RESET);
    account
        .meter_mut()
        .record_send_at(
            REDEEMED_AT,
            MeteredSendPath::Attachments,
            FREE_SPENT_BYTES,
            "fixture-90-percent-transfer",
        )
        .expect("fixture transfer records");
    account
}

#[test]
fn task_4609_top_up_raises_only_this_accounts_current_month_cap() {
    let mut fixture = fixture_account();
    let other = MonthlyAllowanceAccount::new(ACCOUNT_RESET);

    let before = fixture
        .snapshot_at(REDEEMED_AT)
        .expect("read free allowance");
    println!(
        "TASK4609_BEFORE cap={} spent={} remaining={}",
        before.cap_bytes(),
        before.spent_bytes(),
        before.remaining_bytes()
    );
    assert_eq!(before.cap_bytes(), FREE_MONTHLY_ALLOWANCE_BYTES);
    assert_eq!(before.spent_bytes(), FREE_SPENT_BYTES);
    assert_eq!(
        before.remaining_bytes(),
        FREE_MONTHLY_ALLOWANCE_BYTES - FREE_SPENT_BYTES
    );

    let redeemed = fixture
        .redeem_top_up_at(REDEEMED_AT, TopUpPack::TinyTest, "opaque-voucher-4609")
        .expect("redeem one anonymous TINY-TEST voucher");
    println!(
        "TASK4609_REDEEMED pack={} cap={} spent={} remaining={}",
        TopUpPack::TinyTest.code(),
        redeemed.cap_bytes(),
        redeemed.spent_bytes(),
        redeemed.remaining_bytes()
    );
    assert_eq!(
        redeemed.cap_bytes(),
        FREE_MONTHLY_ALLOWANCE_BYTES + TINY_TEST_TOP_UP_ALLOWANCE_BYTES
    );
    assert_eq!(redeemed.spent_bytes(), FREE_SPENT_BYTES);
    assert_eq!(
        redeemed.remaining_bytes(),
        FREE_MONTHLY_ALLOWANCE_BYTES + TINY_TEST_TOP_UP_ALLOWANCE_BYTES - FREE_SPENT_BYTES
    );
    assert_eq!(fixture.redeemed_top_up_count(), 1);

    let duplicate = fixture
        .redeem_top_up_at(REDEEMED_AT, TopUpPack::TinyTest, "opaque-voucher-4609")
        .expect_err("a redeemed voucher cannot be spent twice");
    println!("TASK4609_DUPLICATE error={duplicate}");
    assert_eq!(duplicate.to_string(), "already redeemed");
    assert_eq!(fixture.redeemed_top_up_count(), 1);

    let other_snapshot = other.snapshot_at(REDEEMED_AT).expect("read other account");
    println!(
        "TASK4609_OTHER cap={} spent={} remaining={}",
        other_snapshot.cap_bytes(),
        other_snapshot.spent_bytes(),
        other_snapshot.remaining_bytes()
    );
    assert_eq!(other_snapshot.cap_bytes(), FREE_MONTHLY_ALLOWANCE_BYTES);
    assert_eq!(other_snapshot.spent_bytes(), 0);
    assert_eq!(
        other_snapshot.remaining_bytes(),
        FREE_MONTHLY_ALLOWANCE_BYTES
    );
}
