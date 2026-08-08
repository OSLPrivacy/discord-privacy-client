//! Whole-service cost projection and named early warnings.
//!
//! The ceiling is expressed in integer micro-currency units so the alarm never
//! depends on floating-point rounding. Storage is a live-byte charge and
//! transfer is a UTC-day charge, exactly as `usage_counters` records them.

use crate::usage_counters::{ServiceUsageCounters, UsageCounterError, UsageCounterStore};
use thiserror::Error;

/// Integer price inputs supplied by the service money ceiling policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoneyCeiling {
    /// Maximum projected cost, in micro-currency units.
    pub ceiling_micros: u64,
    /// Cost per stored byte, in micro-currency units.
    pub stored_byte_micros: u64,
    /// Cost per transferred byte, in micro-currency units.
    pub transferred_byte_micros: u64,
}

/// A warning is named so operators can route it without parsing prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostWarning {
    HalfCostCeiling,
    ThreeQuarterCostCeiling,
}

impl CostWarning {
    pub const fn name(self) -> &'static str {
        match self {
            Self::HalfCostCeiling => "half_cost_ceiling",
            Self::ThreeQuarterCostCeiling => "three_quarter_cost_ceiling",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CostAlarmReport {
    pub usage: ServiceUsageCounters,
    pub projected_cost_micros: u64,
    pub warning: Option<CostWarning>,
}

#[derive(Debug, Error)]
pub enum CostAlarmError {
    #[error("usage counter read failed: {0}")]
    Usage(#[from] UsageCounterError),
    #[error("money ceiling must be positive")]
    ZeroCeiling,
    #[error("cost projection overflow")]
    Overflow,
}

/// Read the whole ledger and classify its projected cost against the ceiling.
pub fn check_cost_alarm(
    store: &UsageCounterStore,
    ceiling: MoneyCeiling,
    unix_seconds: i64,
) -> Result<CostAlarmReport, CostAlarmError> {
    if ceiling.ceiling_micros == 0 {
        return Err(CostAlarmError::ZeroCeiling);
    }
    let usage = store.read_service_at(unix_seconds)?;
    evaluate_cost_alarm(usage, ceiling)
}

/// Pure classifier retained for callers that already hold a service snapshot.
pub fn evaluate_cost_alarm(
    usage: ServiceUsageCounters,
    ceiling: MoneyCeiling,
) -> Result<CostAlarmReport, CostAlarmError> {
    if ceiling.ceiling_micros == 0 {
        return Err(CostAlarmError::ZeroCeiling);
    }
    let stored_cost = usage
        .stored_bytes
        .checked_mul(ceiling.stored_byte_micros)
        .ok_or(CostAlarmError::Overflow)?;
    let transferred_cost = usage
        .transferred_bytes_today
        .checked_mul(ceiling.transferred_byte_micros)
        .ok_or(CostAlarmError::Overflow)?;
    let projected_cost_micros = stored_cost
        .checked_add(transferred_cost)
        .ok_or(CostAlarmError::Overflow)?;

    // Compare products to avoid a lossy percentage conversion. The higher
    // threshold wins, so a 76% fixture has exactly one actionable warning.
    let warning = if projected_cost_micros
        .checked_mul(4)
        .ok_or(CostAlarmError::Overflow)?
        >= ceiling
            .ceiling_micros
            .checked_mul(3)
            .ok_or(CostAlarmError::Overflow)?
    {
        Some(CostWarning::ThreeQuarterCostCeiling)
    } else if projected_cost_micros
        .checked_mul(2)
        .ok_or(CostAlarmError::Overflow)?
        >= ceiling.ceiling_micros
    {
        Some(CostWarning::HalfCostCeiling)
    } else {
        None
    };

    Ok(CostAlarmReport {
        usage,
        projected_cost_micros,
        warning,
    })
}
