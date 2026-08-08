// This crate hosts apps/osl-hub/src/osl_mail.rs via #[path] and hand-writes the
// claim_state it needs. osl_mail.rs grew a test that reads claim_of(),
// CarrierEvidence, DeliveryEvidence and PublicClaim::Planned as the lanes
// landed, so the shim has to grow with it -- the same way the selectors
// task_1251 shim had to. Values mirror osl-hub's real claim for OslMail:
// no carrier by construction, not deliverable, Planned.
pub mod claim_state {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum Surface {
        OslMail,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PublicClaim {
        Planned,
    }

    impl PublicClaim {
        pub fn is_capability_claim(&self) -> bool {
            false
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum CarrierEvidence {
        NoCarrierByConstruction,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum DeliveryEvidence {
        NotDeliverable,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct SurfaceClaim {
        pub carrier: CarrierEvidence,
        pub delivery: DeliveryEvidence,
        /// osl_mail.rs asserts this mentions both "burn" and "cannot deliver".
        pub reason: &'static str,
    }

    static OSL_MAIL_CLAIM: SurfaceClaim = SurfaceClaim {
        carrier: CarrierEvidence::NoCarrierByConstruction,
        delivery: DeliveryEvidence::NotDeliverable,
        reason: "OSL Mail can provision an address, accept a send and burn a mailbox against the deployed service, and it cannot deliver: no message body is ever uploaded and there is no retrieval path, so nothing sent through it can be read.",
    };

    pub fn claim_of(_surface: Surface) -> &'static SurfaceClaim {
        &OSL_MAIL_CLAIM
    }

    pub fn public_claim(_surface: Surface) -> PublicClaim {
        PublicClaim::Planned
    }
}

pub mod core_bridge {
    use std::sync::Arc;

    pub struct HubCoreState {
        pub osl: Arc<ipc::AppState>,
    }

    impl Default for HubCoreState {
        fn default() -> Self {
            Self {
                osl: Arc::new(ipc::AppState::default()),
            }
        }
    }
}

#[path = "../../../apps/osl-hub/src/osl_mail.rs"]
pub mod osl_mail;
