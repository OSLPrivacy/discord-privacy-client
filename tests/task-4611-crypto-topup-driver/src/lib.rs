#[cfg(test)]
mod tests {
    use ipc::{
        crypto_top_up::{
            CryptoCoin, CryptoInvoicePayment, CryptoTopUpInvoice, CryptoTopUpInvoiceProcessor,
        },
        metered_bytes::{
            MonthlyAllowanceAccount, TopUpPack, FREE_MONTHLY_ALLOWANCE_BYTES,
            TINY_TEST_TOP_UP_ALLOWANCE_BYTES,
        },
    };

    const ACCOUNT_RESET: i64 = 1_785_585_600;
    const SETTLED_AT: i64 = 1_786_363_200;
    const MONERO_AMOUNT_ATOMIC: u128 = 1_250_000_000_000;
    const XMR_CONFIRMATIONS: u32 = 10;

    fn invoice(id_byte: char) -> CryptoTopUpInvoice {
        CryptoTopUpInvoice::new(
            format!("cpay_{}", id_byte.to_string().repeat(32)),
            CryptoCoin::Monero,
            TopUpPack::TinyTest,
            MONERO_AMOUNT_ATOMIC,
            XMR_CONFIRMATIONS,
        )
    }

    fn payment(
        invoice: &CryptoTopUpInvoice,
        amount_atomic: u128,
        confirmations: u32,
    ) -> CryptoInvoicePayment {
        CryptoInvoicePayment::new(
            invoice.invoice_id(),
            CryptoCoin::Monero,
            amount_atomic,
            confirmations,
        )
    }

    fn decimal_gb(bytes: u64) -> String {
        let whole = bytes / 1_000_000_000;
        let hundredths = bytes % 1_000_000_000 / 10_000_000;
        format!("{whole}.{hundredths:02} GB")
    }

    #[test]
    fn task_4611_crypto_invoice_mints_exactly_one_top_up_credit() {
        let mut account = MonthlyAllowanceAccount::new(ACCOUNT_RESET);
        let mut processor = CryptoTopUpInvoiceProcessor::new();

        let before = account.snapshot_at(SETTLED_AT).expect("read initial cap");
        println!(
            "TASK4611_BEFORE top_up_credits={} cap={}",
            account.redeemed_top_up_count(),
            decimal_gb(before.cap_bytes())
        );
        assert_eq!(account.redeemed_top_up_count(), 0);
        assert_eq!(before.cap_bytes(), FREE_MONTHLY_ALLOWANCE_BYTES);

        let confirmed_invoice = invoice('a');
        let confirmed_payment =
            payment(&confirmed_invoice, MONERO_AMOUNT_ATOMIC, XMR_CONFIRMATIONS);
        let after_confirmed = processor
            .apply_at(
                &mut account,
                SETTLED_AT,
                &confirmed_invoice,
                &confirmed_payment,
            )
            .expect("confirmed exact Monero invoice mints a top-up");
        println!(
            "TASK4611_CONFIRMED pack={} top_up_credits={} cap={}",
            TopUpPack::TinyTest.code(),
            account.redeemed_top_up_count(),
            decimal_gb(after_confirmed.cap_bytes())
        );
        assert_eq!(account.redeemed_top_up_count(), 1);
        assert_eq!(
            after_confirmed.cap_bytes(),
            FREE_MONTHLY_ALLOWANCE_BYTES + TINY_TEST_TOP_UP_ALLOWANCE_BYTES
        );

        let unconfirmed_invoice = invoice('b');
        let unconfirmed = processor
            .apply_at(
                &mut account,
                SETTLED_AT,
                &unconfirmed_invoice,
                &payment(
                    &unconfirmed_invoice,
                    MONERO_AMOUNT_ATOMIC,
                    XMR_CONFIRMATIONS - 1,
                ),
            )
            .expect_err("unconfirmed invoice must not mint");
        println!(
            "TASK4611_UNCONFIRMED error={unconfirmed} top_up_credits={}",
            account.redeemed_top_up_count()
        );
        assert!(unconfirmed.to_string().contains("confirmations"));
        assert_eq!(account.redeemed_top_up_count(), 1);

        let underpaid_invoice = invoice('c');
        let underpaid = processor
            .apply_at(
                &mut account,
                SETTLED_AT,
                &underpaid_invoice,
                &payment(
                    &underpaid_invoice,
                    MONERO_AMOUNT_ATOMIC - 1,
                    XMR_CONFIRMATIONS,
                ),
            )
            .expect_err("underpaid invoice must not mint");
        println!(
            "TASK4611_UNDERPAID error={underpaid} top_up_credits={}",
            account.redeemed_top_up_count()
        );
        assert!(underpaid.to_string().contains("amount"));
        assert_eq!(account.redeemed_top_up_count(), 1);

        let replay = processor
            .apply_at(
                &mut account,
                SETTLED_AT,
                &confirmed_invoice,
                &confirmed_payment,
            )
            .expect_err("confirmed invoice replay must not mint");
        println!(
            "TASK4611_REPLAY error={replay} top_up_credits={}",
            account.redeemed_top_up_count()
        );
        assert!(replay.to_string().contains("already handled"));
        assert_eq!(account.redeemed_top_up_count(), 1);

        assert_eq!(processor.payment_records().len(), 1);
        let record = &processor.payment_records()[0];
        println!(
            "TASK4611_PAYMENT_RECORD coin={} amount_atomic={}",
            record.coin(),
            record.amount_atomic()
        );
        assert_eq!(record.coin(), CryptoCoin::Monero);
        assert_eq!(record.amount_atomic(), MONERO_AMOUNT_ATOMIC);
    }
}
