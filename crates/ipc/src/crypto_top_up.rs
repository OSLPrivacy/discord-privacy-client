//! Applies a verified Bitcoin or Monero invoice observation to one local
//! anonymous allowance account.
//!
//! The watcher/keyserver path remains responsible for authenticating payment
//! observations.  This adapter mirrors its replay, confirmation, coin, and
//! amount gates before turning a settled invoice into one opaque top-up
//! redemption.  No transaction id or account identifier enters the byte
//! meter.

use crate::metered_bytes::{
    MonthlyAllowanceAccount, MonthlyAllowanceSnapshot, TopUpPack, TopUpRedemptionError,
};
use std::{collections::HashSet, fmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CryptoCoin {
    Bitcoin,
    Monero,
}

impl fmt::Display for CryptoCoin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Bitcoin => "Bitcoin",
            Self::Monero => "Monero",
        })
    }
}

/// Immutable terms created by the existing crypto invoice path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CryptoTopUpInvoice {
    invoice_id: String,
    coin: CryptoCoin,
    pack: TopUpPack,
    amount_atomic: u128,
    confirmations_required: u32,
}

impl CryptoTopUpInvoice {
    pub fn new(
        invoice_id: impl Into<String>,
        coin: CryptoCoin,
        pack: TopUpPack,
        amount_atomic: u128,
        confirmations_required: u32,
    ) -> Self {
        Self {
            invoice_id: invoice_id.into(),
            coin,
            pack,
            amount_atomic,
            confirmations_required,
        }
    }

    pub fn invoice_id(&self) -> &str {
        &self.invoice_id
    }

    pub const fn coin(&self) -> CryptoCoin {
        self.coin
    }

    pub const fn pack(&self) -> TopUpPack {
        self.pack
    }

    pub const fn amount_atomic(&self) -> u128 {
        self.amount_atomic
    }

    pub const fn confirmations_required(&self) -> u32 {
        self.confirmations_required
    }
}

/// Node-verified settlement observation delivered for an invoice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CryptoInvoicePayment {
    invoice_id: String,
    coin: CryptoCoin,
    amount_atomic: u128,
    confirmations: u32,
}

impl CryptoInvoicePayment {
    pub fn new(
        invoice_id: impl Into<String>,
        coin: CryptoCoin,
        amount_atomic: u128,
        confirmations: u32,
    ) -> Self {
        Self {
            invoice_id: invoice_id.into(),
            coin,
            amount_atomic,
            confirmations,
        }
    }
}

/// Minimal receipt retained after a top-up invoice is accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CryptoTopUpPaymentRecord {
    invoice_id: String,
    coin: CryptoCoin,
    amount_atomic: u128,
}

impl CryptoTopUpPaymentRecord {
    pub fn invoice_id(&self) -> &str {
        &self.invoice_id
    }

    pub const fn coin(&self) -> CryptoCoin {
        self.coin
    }

    pub const fn amount_atomic(&self) -> u128 {
        self.amount_atomic
    }
}

/// One-account invoice consumer.  Successful invoice ids are remembered so a
/// replay cannot mint a second voucher or payment record.
#[derive(Debug, Default)]
pub struct CryptoTopUpInvoiceProcessor {
    handled_invoice_ids: HashSet<String>,
    payment_records: Vec<CryptoTopUpPaymentRecord>,
}

impl CryptoTopUpInvoiceProcessor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply_at(
        &mut self,
        account: &mut MonthlyAllowanceAccount,
        unix_seconds: i64,
        invoice: &CryptoTopUpInvoice,
        payment: &CryptoInvoicePayment,
    ) -> Result<MonthlyAllowanceSnapshot, CryptoTopUpInvoiceError> {
        // Match the settlement endpoint's replay-first behavior.  Once an
        // invoice succeeded, later messages can never reach any minting path.
        if self.handled_invoice_ids.contains(invoice.invoice_id()) {
            return Err(CryptoTopUpInvoiceError::AlreadyHandled);
        }
        if invoice.invoice_id().is_empty()
            || invoice.amount_atomic() == 0
            || invoice.confirmations_required() == 0
        {
            return Err(CryptoTopUpInvoiceError::MalformedInvoice);
        }
        if payment.invoice_id != invoice.invoice_id || payment.coin != invoice.coin() {
            return Err(CryptoTopUpInvoiceError::PaymentDoesNotSatisfyInvoice);
        }
        if payment.confirmations < invoice.confirmations_required() {
            return Err(CryptoTopUpInvoiceError::InsufficientConfirmations);
        }
        if payment.amount_atomic < invoice.amount_atomic() {
            return Err(CryptoTopUpInvoiceError::AmountTooLow {
                expected: invoice.amount_atomic(),
                got: payment.amount_atomic,
            });
        }

        let snapshot = account
            .redeem_top_up_at(unix_seconds, invoice.pack(), invoice.invoice_id())
            .map_err(|error| match error {
                TopUpRedemptionError::AlreadyRedeemed => CryptoTopUpInvoiceError::AlreadyHandled,
                other => CryptoTopUpInvoiceError::TopUp(other),
            })?;
        self.handled_invoice_ids
            .insert(invoice.invoice_id().to_owned());
        self.payment_records.push(CryptoTopUpPaymentRecord {
            invoice_id: invoice.invoice_id().to_owned(),
            coin: payment.coin,
            amount_atomic: payment.amount_atomic,
        });
        Ok(snapshot)
    }

    pub fn payment_records(&self) -> &[CryptoTopUpPaymentRecord] {
        &self.payment_records
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoTopUpInvoiceError {
    AlreadyHandled,
    MalformedInvoice,
    PaymentDoesNotSatisfyInvoice,
    InsufficientConfirmations,
    AmountTooLow { expected: u128, got: u128 },
    TopUp(TopUpRedemptionError),
}

impl fmt::Display for CryptoTopUpInvoiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyHandled => formatter.write_str("payment message already handled"),
            Self::MalformedInvoice => formatter.write_str("invoice fields malformed"),
            Self::PaymentDoesNotSatisfyInvoice => {
                formatter.write_str("payment does not satisfy this invoice")
            }
            Self::InsufficientConfirmations => {
                formatter.write_str("payment does not have enough confirmations")
            }
            Self::AmountTooLow { expected, got } => {
                write!(
                    formatter,
                    "payment amount too low: expected {expected}, got {got}"
                )
            }
            Self::TopUp(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for CryptoTopUpInvoiceError {}
