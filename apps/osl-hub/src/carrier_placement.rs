//! Carrier bytes and placement timing are separate concerns.
//!
//! The Free Atomic path may change *when* a public carrier is entered, but it
//! must never change the carrier itself. Keeping that boundary explicit makes
//! the product rule executable without involving a native Discord window.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CarrierPlacementTiming {
    Atomic,
    Compatibility,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CarrierPlacement<'a> {
    payload: &'a str,
    timing: CarrierPlacementTiming,
}

impl<'a> CarrierPlacement<'a> {
    pub fn new(payload: &'a str, timing: CarrierPlacementTiming) -> Self {
        Self { payload, timing }
    }

    pub fn payload(self) -> &'a str {
        self.payload
    }

    pub fn timing(self) -> CarrierPlacementTiming {
        self.timing
    }
}
