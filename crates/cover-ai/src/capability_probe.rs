//! Admission control for the optional local cover model.
//!
//! The word-bank carrier is always available.  This probe exists solely to
//! keep an optional model from turning an underpowered device into a paging
//! machine or a stalled composer.

/// The machine facts needed to decide whether an optional model may start.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MachineCapabilities {
    pub avx2: bool,
    pub physical_cores: u16,
    pub free_memory_bytes: u64,
}

/// A model's measured minimum working set and CPU floor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelRequirements {
    pub minimum_physical_cores: u16,
    /// Includes the model's measured peak RSS, not just the artifact size.
    pub peak_working_set_bytes: u64,
}

/// Reserve for Tauri/WebView2 and the active desktop session.  A model is not
/// offered when loading it would consume this reserve.
pub const DESKTOP_MEMORY_RESERVE_BYTES: u64 = 300 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelAdmission {
    Allowed,
    MissingAvx2,
    InsufficientCores,
    InsufficientFreeMemory,
}

/// Decides whether the optional model may be offered or loaded.
///
/// This is deliberately pure: platform-specific collection belongs at the UI
/// boundary, while tests and callers can replay the exact measured machine
/// facts.  Any refusal selects the word-bank fallback; it never blocks send.
pub fn admit_model(
    machine: MachineCapabilities,
    requirements: ModelRequirements,
) -> ModelAdmission {
    if !machine.avx2 {
        return ModelAdmission::MissingAvx2;
    }
    if machine.physical_cores < requirements.minimum_physical_cores {
        return ModelAdmission::InsufficientCores;
    }
    if machine.free_memory_bytes
        < requirements
            .peak_working_set_bytes
            .saturating_add(DESKTOP_MEMORY_RESERVE_BYTES)
    {
        return ModelAdmission::InsufficientFreeMemory;
    }
    ModelAdmission::Allowed
}

#[cfg(test)]
mod tests {
    use super::{admit_model, MachineCapabilities, ModelAdmission, ModelRequirements};

    const REQUIREMENTS: ModelRequirements = ModelRequirements {
        minimum_physical_cores: 4,
        peak_working_set_bytes: 700 * 1024 * 1024,
    };

    #[test]
    fn t13_td7_refuses_a_machine_that_cannot_carry_the_model() {
        let admission = admit_model(
            MachineCapabilities {
                avx2: true,
                physical_cores: 4,
                free_memory_bytes: 512 * 1024 * 1024,
            },
            REQUIREMENTS,
        );

        assert_eq!(admission, ModelAdmission::InsufficientFreeMemory);
    }

    #[test]
    fn requires_avx2_and_the_measured_core_floor() {
        assert_eq!(
            admit_model(
                MachineCapabilities { avx2: false, physical_cores: 8, free_memory_bytes: u64::MAX },
                REQUIREMENTS,
            ),
            ModelAdmission::MissingAvx2
        );
        assert_eq!(
            admit_model(
                MachineCapabilities { avx2: true, physical_cores: 2, free_memory_bytes: u64::MAX },
                REQUIREMENTS,
            ),
            ModelAdmission::InsufficientCores
        );
    }

    #[test]
    fn admits_only_when_model_and_desktop_reserve_fit() {
        assert_eq!(
            admit_model(
                MachineCapabilities {
                    avx2: true,
                    physical_cores: 4,
                    free_memory_bytes: REQUIREMENTS.peak_working_set_bytes + super::DESKTOP_MEMORY_RESERVE_BYTES,
                },
                REQUIREMENTS,
            ),
            ModelAdmission::Allowed
        );
    }
}
