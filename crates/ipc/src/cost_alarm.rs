//! Whole-service cost projection and named early warnings.

use crate::usage_counters::{ServiceUsageCounters, UsageCounterError, UsageCounterStore};
use thiserror::Error;

/// Integer price inputs supplied by the service money ceiling policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoneyCeiling {
    pub ceiling_micros: u64,
    pub stored_byte_micros: u64,
    pub transferred_byte_micros: u64,
}

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

pub fn check_cost_alarm(
    store: &UsageCounterStore,
    ceiling: MoneyCeiling,
    unix_seconds: i64,
) -> Result<CostAlarmReport, CostAlarmError> {
    if ceiling.ceiling_micros == 0 {
        return Err(CostAlarmError::ZeroCeiling);
    }
    evaluate_cost_alarm(store.read_service_at(unix_seconds)?, ceiling)
}

pub fn evaluate_cost_alarm(
    usage: ServiceUsageCounters,
    ceiling: MoneyCeiling,
) -> Result<CostAlarmReport, CostAlarmError> {
    if ceiling.ceiling_micros == 0 {
        return Err(CostAlarmError::ZeroCeiling);
    }
    let projected_cost_micros = usage
        .stored_bytes
        .checked_mul(ceiling.stored_byte_micros)
        .and_then(|stored| {
            usage
                .transferred_bytes_today
                .checked_mul(ceiling.transferred_byte_micros)
                .and_then(|transferred| stored.checked_add(transferred))
        })
        .ok_or(CostAlarmError::Overflow)?;
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
