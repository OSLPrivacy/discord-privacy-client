//! Install/license/uninstall lifecycle for the optional proprietary module.
//!
//! The open side owns the lifecycle state. A closed module can only receive
//! typed boundary inputs after install, consent, binding, authority, brokered
//! HTTPS policy, and an explicit license-check step have all succeeded.

use crate::proprietary_module_boundary::{
    AuthorityGrant, BindingGrant, BoundaryError, BrokeredNetworkBinding, ConsentGrant,
    OptionalModuleInstallGrant, ProprietaryNetworkEgressRequest, ProprietaryNetworkEndpoint,
    VerifiedOptionalModuleAccess,
};
use std::error::Error;
use std::fmt;

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ProprietaryModuleLifecyclePhase {
    BaseAppOnly,
    InstalledAwaitingLicenseCheck,
    Licensed,
    Uninstalled,
}

impl fmt::Debug for ProprietaryModuleLifecyclePhase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::BaseAppOnly => "ProprietaryModuleLifecyclePhase::BaseAppOnly",
            Self::InstalledAwaitingLicenseCheck => {
                "ProprietaryModuleLifecyclePhase::InstalledAwaitingLicenseCheck"
            }
            Self::Licensed => "ProprietaryModuleLifecyclePhase::Licensed",
            Self::Uninstalled => "ProprietaryModuleLifecyclePhase::Uninstalled",
        })
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct ProprietaryModuleLifecycle {
    phase: ProprietaryModuleLifecyclePhase,
    access: Option<VerifiedOptionalModuleAccess>,
}

impl Default for ProprietaryModuleLifecycle {
    fn default() -> Self {
        Self {
            phase: ProprietaryModuleLifecyclePhase::BaseAppOnly,
            access: None,
        }
    }
}

impl ProprietaryModuleLifecycle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn phase(&self) -> ProprietaryModuleLifecyclePhase {
        self.phase
    }

    pub fn base_app_available(&self) -> bool {
        true
    }

    pub fn module_access(
        &self,
    ) -> Result<&VerifiedOptionalModuleAccess, ProprietaryModuleLifecycleError> {
        match (self.phase, self.access.as_ref()) {
            (ProprietaryModuleLifecyclePhase::Licensed, Some(access)) => Ok(access),
            (ProprietaryModuleLifecyclePhase::InstalledAwaitingLicenseCheck, Some(_)) => {
                Err(ProprietaryModuleLifecycleError::LicenseCheckRequired)
            }
            _ => Err(ProprietaryModuleLifecycleError::ModuleNotInstalled),
        }
    }

    pub fn install(
        &mut self,
        install: OptionalModuleInstallGrant,
        consent: ConsentGrant,
        binding: BindingGrant,
        authority: AuthorityGrant,
        network_binding: BrokeredNetworkBinding,
    ) -> Result<ProprietaryNetworkEgressRequest, ProprietaryModuleLifecycleError> {
        let access = VerifiedOptionalModuleAccess::authorize(install, consent, binding, authority)?;
        let egress = ProprietaryNetworkEgressRequest::brokered_https(
            &access,
            ProprietaryNetworkEndpoint::PackageInstall,
            network_binding,
        )?;
        self.access = Some(access);
        self.phase = ProprietaryModuleLifecyclePhase::InstalledAwaitingLicenseCheck;
        Ok(egress)
    }

    pub fn license_check(
        &mut self,
        network_binding: BrokeredNetworkBinding,
    ) -> Result<ProprietaryNetworkEgressRequest, ProprietaryModuleLifecycleError> {
        let access = self
            .access
            .as_ref()
            .ok_or(ProprietaryModuleLifecycleError::ModuleNotInstalled)?;
        let egress = ProprietaryNetworkEgressRequest::brokered_https(
            access,
            ProprietaryNetworkEndpoint::LicenseCheck,
            network_binding,
        )?;
        self.phase = ProprietaryModuleLifecyclePhase::Licensed;
        Ok(egress)
    }

    pub fn uninstall(
        &mut self,
        network_binding: BrokeredNetworkBinding,
    ) -> Result<ProprietaryNetworkEgressRequest, ProprietaryModuleLifecycleError> {
        let access = self
            .access
            .as_ref()
            .ok_or(ProprietaryModuleLifecycleError::ModuleNotInstalled)?;
        let egress = ProprietaryNetworkEgressRequest::brokered_https(
            access,
            ProprietaryNetworkEndpoint::PackageUninstall,
            network_binding,
        )?;
        self.access = None;
        self.phase = ProprietaryModuleLifecyclePhase::Uninstalled;
        Ok(egress)
    }
}

impl fmt::Debug for ProprietaryModuleLifecycle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProprietaryModuleLifecycle")
            .field("phase", &self.phase)
            .field(
                "access",
                &self.access.as_ref().map(|_| "[redacted; verified]"),
            )
            .finish()
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ProprietaryModuleLifecycleError {
    Boundary(BoundaryError),
    LicenseCheckRequired,
    ModuleNotInstalled,
}

impl fmt::Debug for ProprietaryModuleLifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Boundary(error) => formatter
                .debug_tuple("ProprietaryModuleLifecycleError::Boundary")
                .field(error)
                .finish(),
            Self::LicenseCheckRequired => {
                formatter.write_str("ProprietaryModuleLifecycleError::LicenseCheckRequired")
            }
            Self::ModuleNotInstalled => {
                formatter.write_str("ProprietaryModuleLifecycleError::ModuleNotInstalled")
            }
        }
    }
}

impl fmt::Display for ProprietaryModuleLifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Boundary(error) => fmt::Display::fmt(error, formatter),
            Self::LicenseCheckRequired => {
                formatter.write_str("proprietary module license check is required")
            }
            Self::ModuleNotInstalled => {
                formatter.write_str("optional proprietary module is not installed")
            }
        }
    }
}

impl Error for ProprietaryModuleLifecycleError {}

impl From<BoundaryError> for ProprietaryModuleLifecycleError {
    fn from(error: BoundaryError) -> Self {
        Self::Boundary(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proprietary_module_boundary::{
        OpenPermission, OpenPermissionSet, ProprietaryLicenseTier, ProprietaryModuleManifest,
        ProprietaryNetworkPolicy,
    };
    use std::num::NonZeroU64;

    fn nonzero_revision(value: u64) -> NonZeroU64 {
        NonZeroU64::new(value).expect("test revision is nonzero")
    }

    fn permissions() -> OpenPermissionSet {
        OpenPermissionSet::from_verified_permissions(&[OpenPermission::LocalRiskAdvice])
    }

    fn manifest(network_policy: ProprietaryNetworkPolicy) -> ProprietaryModuleManifest {
        ProprietaryModuleManifest::new(
            "risk-advice.module",
            "Risk Advice Module",
            ProprietaryLicenseTier::Pro,
            network_policy,
            permissions(),
        )
        .expect("manifest is valid")
    }

    fn install_grant(network_policy: ProprietaryNetworkPolicy) -> OptionalModuleInstallGrant {
        OptionalModuleInstallGrant::Installed {
            manifest: manifest(network_policy),
        }
    }

    fn consent() -> ConsentGrant {
        ConsentGrant::Present {
            revision: nonzero_revision(2),
        }
    }

    fn binding() -> BindingGrant {
        BindingGrant::Bound { digest: [0x22; 32] }
    }

    fn authority() -> AuthorityGrant {
        AuthorityGrant::Verified {
            revision: nonzero_revision(3),
        }
    }

    fn network_binding() -> BrokeredNetworkBinding {
        BrokeredNetworkBinding::new([0xB0; 32], nonzero_revision(11))
            .expect("network binding is verified")
    }

    #[test]
    fn proprietary_module_lifecycle_install_license_check_uninstall() {
        let mut lifecycle = ProprietaryModuleLifecycle::new();

        assert_eq!(
            lifecycle.phase(),
            ProprietaryModuleLifecyclePhase::BaseAppOnly
        );
        assert!(lifecycle.base_app_available());
        assert_eq!(
            lifecycle.module_access(),
            Err(ProprietaryModuleLifecycleError::ModuleNotInstalled)
        );
        assert_eq!(
            lifecycle.license_check(network_binding()),
            Err(ProprietaryModuleLifecycleError::ModuleNotInstalled)
        );
        assert_eq!(
            lifecycle.uninstall(network_binding()),
            Err(ProprietaryModuleLifecycleError::ModuleNotInstalled)
        );
        assert_eq!(
            lifecycle.install(
                OptionalModuleInstallGrant::Absent,
                consent(),
                binding(),
                authority(),
                network_binding(),
            ),
            Err(ProprietaryModuleLifecycleError::Boundary(
                BoundaryError::ModuleNotInstalled
            ))
        );
        assert_eq!(
            lifecycle.phase(),
            ProprietaryModuleLifecyclePhase::BaseAppOnly
        );

        assert_eq!(
            lifecycle.install(
                install_grant(ProprietaryNetworkPolicy::BrokeredHttpsOnly),
                ConsentGrant::Absent,
                binding(),
                authority(),
                network_binding(),
            ),
            Err(ProprietaryModuleLifecycleError::Boundary(
                BoundaryError::MissingConsent
            ))
        );
        assert_eq!(
            lifecycle.install(
                install_grant(ProprietaryNetworkPolicy::NoNetwork),
                consent(),
                binding(),
                authority(),
                network_binding(),
            ),
            Err(ProprietaryModuleLifecycleError::Boundary(
                BoundaryError::NetworkPolicyRefused
            ))
        );

        let install_request = lifecycle
            .install(
                install_grant(ProprietaryNetworkPolicy::BrokeredHttpsOnly),
                consent(),
                binding(),
                authority(),
                network_binding(),
            )
            .expect("install egress is allowed after all grants");
        assert_eq!(
            install_request.endpoint(),
            ProprietaryNetworkEndpoint::PackageInstall
        );
        assert_eq!(
            lifecycle.phase(),
            ProprietaryModuleLifecyclePhase::InstalledAwaitingLicenseCheck
        );
        assert!(lifecycle.base_app_available());
        assert_eq!(
            lifecycle.module_access(),
            Err(ProprietaryModuleLifecycleError::LicenseCheckRequired)
        );

        let license_request = lifecycle
            .license_check(network_binding())
            .expect("license-check egress is allowed after install");
        assert_eq!(
            license_request.endpoint(),
            ProprietaryNetworkEndpoint::LicenseCheck
        );
        assert_eq!(lifecycle.phase(), ProprietaryModuleLifecyclePhase::Licensed);
        assert!(lifecycle.module_access().is_ok());

        let uninstall_request = lifecycle
            .uninstall(network_binding())
            .expect("uninstall egress is allowed while installed");
        assert_eq!(
            uninstall_request.endpoint(),
            ProprietaryNetworkEndpoint::PackageUninstall
        );
        assert_eq!(
            lifecycle.phase(),
            ProprietaryModuleLifecyclePhase::Uninstalled
        );
        assert!(lifecycle.base_app_available());
        assert_eq!(
            lifecycle.module_access(),
            Err(ProprietaryModuleLifecycleError::ModuleNotInstalled)
        );
        assert_eq!(
            lifecycle.license_check(network_binding()),
            Err(ProprietaryModuleLifecycleError::ModuleNotInstalled)
        );

        let rendered = format!("{lifecycle:?}\n{install_request:?}\n{license_request:?}");
        assert!(!rendered.contains("risk-advice.module"));
        assert!(!rendered.contains("Risk Advice Module"));
        assert!(!rendered.contains("https://"));
        assert!(!rendered.contains("B0B0"));
        assert!(rendered.contains("ProprietaryNetworkEndpoint::PackageInstall"));
        assert!(rendered.contains("ProprietaryNetworkEndpoint::LicenseCheck"));
    }
}
