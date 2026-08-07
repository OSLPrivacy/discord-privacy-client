pub mod claim_state {
    #[derive(Clone, Copy)]
    pub enum Surface {
        OslMail,
    }

    pub struct PublicClaim;

    impl PublicClaim {
        pub fn is_capability_claim(&self) -> bool {
            false
        }
    }

    pub fn public_claim(_surface: Surface) -> PublicClaim {
        PublicClaim
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
