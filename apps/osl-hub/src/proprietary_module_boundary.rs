//! Narrow contract for an optional proprietary product module.
//!
//! This file defines types only. It does not load, discover, register, or call
//! any closed-source implementation from production code. The default open
//! side is absence, and absence is an ordinary "no advice" outcome.

use serde::Serialize;
use std::error::Error;
use std::fmt;
use std::num::NonZeroU64;

pub const PROPRIETARY_MODULE_CONTRACT_VERSION: u16 = 1;
pub const MAX_EXPLICIT_PLAINTEXT_BYTES: usize = 16 * 1024;

const PERMISSION_LOCAL_RISK_ADVICE: u16 = 1 << 0;
const PERMISSION_SERVICE_LAYOUT_ADVICE: u16 = 1 << 1;
const PERMISSION_RELIABILITY_SUMMARY: u16 = 1 << 2;

pub trait ProprietaryModule {
    fn evaluate(
        &self,
        request: ProprietaryModuleRequest<'_>,
    ) -> Result<ProprietaryModuleAdvice, ProprietaryModuleError>;
}

pub enum ProprietaryModuleSlot<M> {
    Absent,
    Present(M),
}

impl<M> Default for ProprietaryModuleSlot<M> {
    fn default() -> Self {
        Self::Absent
    }
}

impl<M> ProprietaryModuleSlot<M> {
    pub fn present(module: M) -> Self {
        Self::Present(module)
    }

    pub fn is_absent(&self) -> bool {
        matches!(self, Self::Absent)
    }
}

impl<M: ProprietaryModule> ProprietaryModuleSlot<M> {
    pub fn evaluate(
        &self,
        request: ProprietaryModuleRequest<'_>,
    ) -> Result<BoundaryOutcome, ProprietaryModuleError> {
        match self {
            Self::Absent => Ok(BoundaryOutcome::ModuleAbsent),
            Self::Present(module) => module.evaluate(request).map(BoundaryOutcome::Advice),
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum BoundaryOperation {
    LocalRiskAdvice,
    ServiceLayoutAdvice,
    ReliabilitySummary,
}

impl BoundaryOperation {
    fn required_permission(self) -> OpenPermission {
        match self {
            Self::LocalRiskAdvice => OpenPermission::LocalRiskAdvice,
            Self::ServiceLayoutAdvice => OpenPermission::ServiceLayoutAdvice,
            Self::ReliabilitySummary => OpenPermission::ReliabilitySummary,
        }
    }
}

impl fmt::Debug for BoundaryOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::LocalRiskAdvice => "BoundaryOperation::LocalRiskAdvice",
            Self::ServiceLayoutAdvice => "BoundaryOperation::ServiceLayoutAdvice",
            Self::ReliabilitySummary => "BoundaryOperation::ReliabilitySummary",
        })
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BoundaryService {
    Discord,
    BrowserCompanion,
    NativeApp,
    OslHub,
}

impl fmt::Debug for BoundaryService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Discord => "BoundaryService::Discord",
            Self::BrowserCompanion => "BoundaryService::BrowserCompanion",
            Self::NativeApp => "BoundaryService::NativeApp",
            Self::OslHub => "BoundaryService::OslHub",
        })
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct BoundaryContext {
    service: BoundaryService,
    account_binding_digest: [u8; 32],
    scope_binding_digest: [u8; 32],
}

impl BoundaryContext {
    pub fn new(
        service: BoundaryService,
        account_binding_digest: [u8; 32],
        scope_binding_digest: [u8; 32],
    ) -> Result<Self, BoundaryError> {
        if all_zero(&account_binding_digest) {
            return Err(BoundaryError::MissingBinding);
        }
        if all_zero(&scope_binding_digest) {
            return Err(BoundaryError::MissingBinding);
        }
        Ok(Self {
            service,
            account_binding_digest,
            scope_binding_digest,
        })
    }

    pub fn service(&self) -> BoundaryService {
        self.service
    }

    pub fn account_binding_digest(&self) -> [u8; 32] {
        self.account_binding_digest
    }

    pub fn scope_binding_digest(&self) -> [u8; 32] {
        self.scope_binding_digest
    }
}

impl fmt::Debug for BoundaryContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BoundaryContext")
            .field("service", &self.service)
            .field("account_binding_digest", &"[redacted; sha256]")
            .field("scope_binding_digest", &"[redacted; sha256]")
            .finish()
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ConsentGrant {
    Absent,
    Present { revision: NonZeroU64 },
}

impl Default for ConsentGrant {
    fn default() -> Self {
        Self::Absent
    }
}

impl fmt::Debug for ConsentGrant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absent => formatter.write_str("ConsentGrant::Absent"),
            Self::Present { .. } => formatter.write_str("ConsentGrant::Present { revision: ... }"),
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum BindingGrant {
    Absent,
    Bound { digest: [u8; 32] },
}

impl Default for BindingGrant {
    fn default() -> Self {
        Self::Absent
    }
}

impl fmt::Debug for BindingGrant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absent => formatter.write_str("BindingGrant::Absent"),
            Self::Bound { .. } => formatter.write_str("BindingGrant::Bound { digest: ... }"),
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum AuthorityGrant {
    Absent,
    Verified { revision: NonZeroU64 },
}

impl Default for AuthorityGrant {
    fn default() -> Self {
        Self::Absent
    }
}

impl fmt::Debug for AuthorityGrant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absent => formatter.write_str("AuthorityGrant::Absent"),
            Self::Verified { .. } => {
                formatter.write_str("AuthorityGrant::Verified { revision: ... }")
            }
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum OpenPermission {
    LocalRiskAdvice,
    ServiceLayoutAdvice,
    ReliabilitySummary,
}

impl OpenPermission {
    fn bit(self) -> u16 {
        match self {
            Self::LocalRiskAdvice => PERMISSION_LOCAL_RISK_ADVICE,
            Self::ServiceLayoutAdvice => PERMISSION_SERVICE_LAYOUT_ADVICE,
            Self::ReliabilitySummary => PERMISSION_RELIABILITY_SUMMARY,
        }
    }
}

impl fmt::Debug for OpenPermission {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::LocalRiskAdvice => "OpenPermission::LocalRiskAdvice",
            Self::ServiceLayoutAdvice => "OpenPermission::ServiceLayoutAdvice",
            Self::ReliabilitySummary => "OpenPermission::ReliabilitySummary",
        })
    }
}

#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub struct OpenPermissionSet {
    bits: u16,
}

impl OpenPermissionSet {
    pub fn empty() -> Self {
        Self { bits: 0 }
    }

    pub fn allows(&self, permission: OpenPermission) -> bool {
        self.bits & permission.bit() != 0
    }

    pub fn is_empty(&self) -> bool {
        self.bits == 0
    }

    #[cfg(test)]
    fn from_verified_permissions(permissions: &[OpenPermission]) -> Self {
        let bits = permissions
            .iter()
            .fold(0u16, |bits, permission| bits | permission.bit());
        Self { bits }
    }
}

impl fmt::Debug for OpenPermissionSet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut set = formatter.debug_set();
        if self.allows(OpenPermission::LocalRiskAdvice) {
            set.entry(&OpenPermission::LocalRiskAdvice);
        }
        if self.allows(OpenPermission::ServiceLayoutAdvice) {
            set.entry(&OpenPermission::ServiceLayoutAdvice);
        }
        if self.allows(OpenPermission::ReliabilitySummary) {
            set.entry(&OpenPermission::ReliabilitySummary);
        }
        set.finish()
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct VerifiedOpenAccess {
    consent_revision: NonZeroU64,
    authority_revision: NonZeroU64,
    binding_digest: [u8; 32],
    permissions: OpenPermissionSet,
}

impl VerifiedOpenAccess {
    pub fn new(
        consent: ConsentGrant,
        binding: BindingGrant,
        authority: AuthorityGrant,
        permissions: OpenPermissionSet,
    ) -> Result<Self, BoundaryError> {
        let ConsentGrant::Present {
            revision: consent_revision,
        } = consent
        else {
            return Err(BoundaryError::MissingConsent);
        };
        let BindingGrant::Bound {
            digest: binding_digest,
        } = binding
        else {
            return Err(BoundaryError::MissingBinding);
        };
        if all_zero(&binding_digest) {
            return Err(BoundaryError::MissingBinding);
        }
        let AuthorityGrant::Verified {
            revision: authority_revision,
        } = authority
        else {
            return Err(BoundaryError::MissingAuthority);
        };
        Ok(Self {
            consent_revision,
            authority_revision,
            binding_digest,
            permissions,
        })
    }

    pub fn allows(&self, permission: OpenPermission) -> bool {
        self.permissions.allows(permission)
    }

    pub fn permissions(&self) -> OpenPermissionSet {
        self.permissions
    }

    pub fn consent_revision(&self) -> NonZeroU64 {
        self.consent_revision
    }

    pub fn authority_revision(&self) -> NonZeroU64 {
        self.authority_revision
    }

    pub fn binding_digest(&self) -> [u8; 32] {
        self.binding_digest
    }
}

impl fmt::Debug for VerifiedOpenAccess {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VerifiedOpenAccess")
            .field("consent_revision", &"[redacted]")
            .field("authority_revision", &"[redacted]")
            .field("binding_digest", &"[redacted; sha256]")
            .field("permissions", &self.permissions)
            .finish()
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ExplicitPlaintext<'a> {
    bytes: &'a [u8],
}

impl<'a> ExplicitPlaintext<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, BoundaryError> {
        if bytes.len() > MAX_EXPLICIT_PLAINTEXT_BYTES {
            return Err(BoundaryError::PlaintextTooLarge);
        }
        Ok(Self { bytes })
    }

    pub fn as_bytes(&self) -> &'a [u8] {
        self.bytes
    }
}

impl fmt::Debug for ExplicitPlaintext<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExplicitPlaintext")
            .field("len", &self.bytes.len())
            .field("bytes", &"[redacted]")
            .finish()
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ProprietaryModuleRequest<'a> {
    operation: BoundaryOperation,
    context: BoundaryContext,
    access: VerifiedOpenAccess,
    ciphertext_digest: [u8; 32],
    explicit_plaintext: Option<ExplicitPlaintext<'a>>,
}

impl<'a> ProprietaryModuleRequest<'a> {
    pub fn sealed_only(
        operation: BoundaryOperation,
        context: BoundaryContext,
        access: VerifiedOpenAccess,
        ciphertext_digest: [u8; 32],
    ) -> Result<Self, BoundaryError> {
        Self::new(operation, context, access, ciphertext_digest, None)
    }

    pub fn with_explicit_plaintext(
        operation: BoundaryOperation,
        context: BoundaryContext,
        access: VerifiedOpenAccess,
        ciphertext_digest: [u8; 32],
        explicit_plaintext: ExplicitPlaintext<'a>,
    ) -> Result<Self, BoundaryError> {
        Self::new(
            operation,
            context,
            access,
            ciphertext_digest,
            Some(explicit_plaintext),
        )
    }

    fn new(
        operation: BoundaryOperation,
        context: BoundaryContext,
        access: VerifiedOpenAccess,
        ciphertext_digest: [u8; 32],
        explicit_plaintext: Option<ExplicitPlaintext<'a>>,
    ) -> Result<Self, BoundaryError> {
        if !access.allows(operation.required_permission()) {
            return Err(BoundaryError::PermissionRefused);
        }
        if all_zero(&ciphertext_digest) {
            return Err(BoundaryError::MissingBinding);
        }
        Ok(Self {
            operation,
            context,
            access,
            ciphertext_digest,
            explicit_plaintext,
        })
    }

    pub fn contract_version(&self) -> u16 {
        PROPRIETARY_MODULE_CONTRACT_VERSION
    }

    pub fn operation(&self) -> BoundaryOperation {
        self.operation
    }

    pub fn context(&self) -> BoundaryContext {
        self.context
    }

    pub fn permissions(&self) -> OpenPermissionSet {
        self.access.permissions()
    }

    pub fn ciphertext_digest(&self) -> [u8; 32] {
        self.ciphertext_digest
    }

    pub fn explicit_plaintext(&self) -> Option<ExplicitPlaintext<'a>> {
        self.explicit_plaintext
    }
}

impl fmt::Debug for ProprietaryModuleRequest<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProprietaryModuleRequest")
            .field("contract_version", &self.contract_version())
            .field("operation", &self.operation)
            .field("context", &self.context)
            .field("access", &self.access)
            .field("ciphertext_digest", &"[redacted; sha256]")
            .field("explicit_plaintext", &self.explicit_plaintext)
            .finish()
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AdvisoryDisposition {
    NoOpinion,
    ProceedWithOpenSourceDecision,
    SuggestRefusal,
    SuggestManualReview,
}

impl fmt::Debug for AdvisoryDisposition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NoOpinion => "AdvisoryDisposition::NoOpinion",
            Self::ProceedWithOpenSourceDecision => {
                "AdvisoryDisposition::ProceedWithOpenSourceDecision"
            }
            Self::SuggestRefusal => "AdvisoryDisposition::SuggestRefusal",
            Self::SuggestManualReview => "AdvisoryDisposition::SuggestManualReview",
        })
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProprietaryModuleAdvice {
    disposition: AdvisoryDisposition,
    confidence: u8,
    reviewed_items: u16,
}

impl ProprietaryModuleAdvice {
    pub fn new(
        disposition: AdvisoryDisposition,
        confidence: u8,
        reviewed_items: u16,
    ) -> Result<Self, ProprietaryModuleError> {
        if confidence > 100 {
            return Err(ProprietaryModuleError::MalformedAdvice);
        }
        Ok(Self {
            disposition,
            confidence,
            reviewed_items,
        })
    }

    pub fn disposition(&self) -> AdvisoryDisposition {
        self.disposition
    }

    pub fn confidence(&self) -> u8 {
        self.confidence
    }

    pub fn reviewed_items(&self) -> u16 {
        self.reviewed_items
    }
}

impl fmt::Debug for ProprietaryModuleAdvice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProprietaryModuleAdvice")
            .field("disposition", &self.disposition)
            .field("confidence", &self.confidence)
            .field("reviewed_items", &self.reviewed_items)
            .finish()
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum BoundaryOutcome {
    ModuleAbsent,
    Advice(ProprietaryModuleAdvice),
}

impl fmt::Debug for BoundaryOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ModuleAbsent => formatter.write_str("BoundaryOutcome::ModuleAbsent"),
            Self::Advice(advice) => formatter
                .debug_tuple("BoundaryOutcome::Advice")
                .field(advice)
                .finish(),
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum BoundaryError {
    MissingConsent,
    MissingBinding,
    MissingAuthority,
    PermissionRefused,
    PlaintextTooLarge,
}

impl fmt::Debug for BoundaryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MissingConsent => "BoundaryError::MissingConsent",
            Self::MissingBinding => "BoundaryError::MissingBinding",
            Self::MissingAuthority => "BoundaryError::MissingAuthority",
            Self::PermissionRefused => "BoundaryError::PermissionRefused",
            Self::PlaintextTooLarge => "BoundaryError::PlaintextTooLarge",
        })
    }
}

impl fmt::Display for BoundaryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MissingConsent => "consent is absent",
            Self::MissingBinding => "binding is absent",
            Self::MissingAuthority => "authority is absent",
            Self::PermissionRefused => "permission is refused",
            Self::PlaintextTooLarge => "explicit plaintext exceeds the boundary limit",
        })
    }
}

impl Error for BoundaryError {}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ProprietaryModuleError {
    ContractVersionUnsupported,
    Declined,
    ExecutionLimitExceeded,
    MalformedAdvice,
}

impl fmt::Debug for ProprietaryModuleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ContractVersionUnsupported => {
                "ProprietaryModuleError::ContractVersionUnsupported"
            }
            Self::Declined => "ProprietaryModuleError::Declined",
            Self::ExecutionLimitExceeded => "ProprietaryModuleError::ExecutionLimitExceeded",
            Self::MalformedAdvice => "ProprietaryModuleError::MalformedAdvice",
        })
    }
}

impl fmt::Display for ProprietaryModuleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ContractVersionUnsupported => {
                "proprietary module contract version is unsupported"
            }
            Self::Declined => "proprietary module declined to advise",
            Self::ExecutionLimitExceeded => "proprietary module execution limit was exceeded",
            Self::MalformedAdvice => "proprietary module returned malformed advice",
        })
    }
}

impl Error for ProprietaryModuleError {}

fn all_zero(bytes: &[u8; 32]) -> bool {
    bytes.iter().all(|byte| *byte == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nonzero_revision(value: u64) -> NonZeroU64 {
        NonZeroU64::new(value).expect("test revision is nonzero")
    }

    fn context() -> BoundaryContext {
        BoundaryContext::new(BoundaryService::Discord, [0xA5; 32], [0x5A; 32])
            .expect("test context is bound")
    }

    fn verified_access(permissions: &[OpenPermission]) -> VerifiedOpenAccess {
        VerifiedOpenAccess::new(
            ConsentGrant::Present {
                revision: nonzero_revision(7),
            },
            BindingGrant::Bound { digest: [0x11; 32] },
            AuthorityGrant::Verified {
                revision: nonzero_revision(9),
            },
            OpenPermissionSet::from_verified_permissions(permissions),
        )
        .expect("test access is verified")
    }

    #[test]
    fn absent_module_is_the_default_and_not_an_error() {
        struct MustNotBeCalled;

        impl ProprietaryModule for MustNotBeCalled {
            fn evaluate(
                &self,
                _request: ProprietaryModuleRequest<'_>,
            ) -> Result<ProprietaryModuleAdvice, ProprietaryModuleError> {
                panic!("absent module path called a proprietary implementation");
            }
        }

        let request = ProprietaryModuleRequest::sealed_only(
            BoundaryOperation::LocalRiskAdvice,
            context(),
            verified_access(&[OpenPermission::LocalRiskAdvice]),
            [0xC7; 32],
        )
        .expect("request is authorized by open-side consent");
        let slot: ProprietaryModuleSlot<MustNotBeCalled> = ProprietaryModuleSlot::default();

        assert!(slot.is_absent());
        assert_eq!(slot.evaluate(request), Ok(BoundaryOutcome::ModuleAbsent));
    }

    #[test]
    fn absence_of_consent_binding_authority_or_permission_refuses() {
        let permissions =
            OpenPermissionSet::from_verified_permissions(&[OpenPermission::LocalRiskAdvice]);

        assert_eq!(
            VerifiedOpenAccess::new(
                ConsentGrant::Absent,
                BindingGrant::Bound { digest: [0x11; 32] },
                AuthorityGrant::Verified {
                    revision: nonzero_revision(1),
                },
                permissions,
            ),
            Err(BoundaryError::MissingConsent)
        );
        assert_eq!(
            VerifiedOpenAccess::new(
                ConsentGrant::Present {
                    revision: nonzero_revision(1),
                },
                BindingGrant::Absent,
                AuthorityGrant::Verified {
                    revision: nonzero_revision(1),
                },
                permissions,
            ),
            Err(BoundaryError::MissingBinding)
        );
        assert_eq!(
            VerifiedOpenAccess::new(
                ConsentGrant::Present {
                    revision: nonzero_revision(1),
                },
                BindingGrant::Bound { digest: [0x11; 32] },
                AuthorityGrant::Absent,
                permissions,
            ),
            Err(BoundaryError::MissingAuthority)
        );

        let access_without_permission = verified_access(&[]);
        assert_eq!(
            ProprietaryModuleRequest::sealed_only(
                BoundaryOperation::LocalRiskAdvice,
                context(),
                access_without_permission,
                [0xC7; 32],
            )
            .unwrap_err(),
            BoundaryError::PermissionRefused
        );
    }

    #[test]
    fn sealed_requests_expose_no_plaintext_to_the_module() {
        struct PlaintextProbe;

        impl ProprietaryModule for PlaintextProbe {
            fn evaluate(
                &self,
                request: ProprietaryModuleRequest<'_>,
            ) -> Result<ProprietaryModuleAdvice, ProprietaryModuleError> {
                assert_eq!(
                    request.contract_version(),
                    PROPRIETARY_MODULE_CONTRACT_VERSION
                );
                assert_eq!(request.operation(), BoundaryOperation::LocalRiskAdvice);
                assert!(request.explicit_plaintext().is_none());
                assert_eq!(request.ciphertext_digest(), [0xC7; 32]);
                ProprietaryModuleAdvice::new(AdvisoryDisposition::NoOpinion, 0, 0)
            }
        }

        let request = ProprietaryModuleRequest::sealed_only(
            BoundaryOperation::LocalRiskAdvice,
            context(),
            verified_access(&[OpenPermission::LocalRiskAdvice]),
            [0xC7; 32],
        )
        .expect("sealed request is valid");
        let slot = ProprietaryModuleSlot::present(PlaintextProbe);

        assert_eq!(
            slot.evaluate(request),
            Ok(BoundaryOutcome::Advice(
                ProprietaryModuleAdvice::new(AdvisoryDisposition::NoOpinion, 0, 0).unwrap()
            ))
        );
    }

    #[test]
    fn explicit_plaintext_is_the_only_plaintext_observation_path() {
        let sensitive = b"redaction-sentinel-bytes";
        let explicit = ExplicitPlaintext::new(sensitive).expect("small explicit plaintext");
        let sealed = ProprietaryModuleRequest::sealed_only(
            BoundaryOperation::LocalRiskAdvice,
            context(),
            verified_access(&[OpenPermission::LocalRiskAdvice]),
            [0xC7; 32],
        )
        .expect("sealed request is valid");
        let opened = ProprietaryModuleRequest::with_explicit_plaintext(
            BoundaryOperation::LocalRiskAdvice,
            context(),
            verified_access(&[OpenPermission::LocalRiskAdvice]),
            [0xC7; 32],
            explicit,
        )
        .expect("explicit plaintext request is valid");

        assert!(sealed.explicit_plaintext().is_none());
        assert_eq!(
            opened
                .explicit_plaintext()
                .map(|plaintext| plaintext.as_bytes()),
            Some(sensitive.as_slice())
        );
    }

    #[test]
    fn module_advice_cannot_widen_open_side_permissions() {
        struct PermissionProbe;

        impl ProprietaryModule for PermissionProbe {
            fn evaluate(
                &self,
                request: ProprietaryModuleRequest<'_>,
            ) -> Result<ProprietaryModuleAdvice, ProprietaryModuleError> {
                assert!(request
                    .permissions()
                    .allows(OpenPermission::LocalRiskAdvice));
                assert!(!request
                    .permissions()
                    .allows(OpenPermission::ServiceLayoutAdvice));
                ProprietaryModuleAdvice::new(
                    AdvisoryDisposition::ProceedWithOpenSourceDecision,
                    80,
                    1,
                )
            }
        }

        let request = ProprietaryModuleRequest::sealed_only(
            BoundaryOperation::LocalRiskAdvice,
            context(),
            verified_access(&[OpenPermission::LocalRiskAdvice]),
            [0xC7; 32],
        )
        .expect("request is valid");
        let outcome = ProprietaryModuleSlot::present(PermissionProbe)
            .evaluate(request)
            .expect("module can only return advice");

        assert_eq!(
            outcome,
            BoundaryOutcome::Advice(
                ProprietaryModuleAdvice::new(
                    AdvisoryDisposition::ProceedWithOpenSourceDecision,
                    80,
                    1,
                )
                .unwrap()
            )
        );
        assert!(!request
            .permissions()
            .allows(OpenPermission::ServiceLayoutAdvice));
    }

    #[test]
    fn debug_and_display_outputs_are_redacted() {
        let sensitive = b"redaction-sentinel-bytes";
        let request = ProprietaryModuleRequest::with_explicit_plaintext(
            BoundaryOperation::LocalRiskAdvice,
            context(),
            verified_access(&[OpenPermission::LocalRiskAdvice]),
            [0xC7; 32],
            ExplicitPlaintext::new(sensitive).expect("small explicit plaintext"),
        )
        .expect("request is valid");
        let rendered = [
            format!("{request:?}"),
            format!("{:?}", request.context()),
            format!("{:?}", request.explicit_plaintext().unwrap()),
            format!("{:?}", BoundaryError::MissingConsent),
            format!("{}", BoundaryError::MissingConsent),
            format!("{:?}", ProprietaryModuleError::Declined),
            format!("{}", ProprietaryModuleError::Declined),
        ]
        .join("\n");

        assert!(!rendered.contains("redaction-sentinel-bytes"));
        assert!(!rendered.contains("C7C7"));
        assert!(rendered.contains("[redacted]") || rendered.contains("[redacted; sha256]"));
    }
}
