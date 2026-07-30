//! Minimal authority model for hosted ports.
//!
//! A scan-only hosted port may observe content-free row metadata, but it must
//! never inherit deletion authority from the generic hosted-port shape.

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum HostedPortMode {
    ScanOnly,
    DeleteCapable,
}

impl HostedPortMode {
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::ScanOnly => "scan_only",
            Self::DeleteCapable => "delete_capable",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum HostedPortOperation {
    ScanOwnItems,
    DeleteOwnItem,
}

impl HostedPortMode {
    pub const fn allows(self, operation: HostedPortOperation) -> bool {
        match (self, operation) {
            (Self::ScanOnly, HostedPortOperation::DeleteOwnItem) => false,
            (_, _) => true,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum HostedContentKind {
    DirectMessage,
    Post,
}

impl HostedContentKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "direct-message" => Some(Self::DirectMessage),
            "post" => Some(Self::Post),
            _ => None,
        }
    }

    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::DirectMessage => "direct-message",
            Self::Post => "post",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum HostedSessionCommand {
    ScanOwnContent {
        mode: HostedPortMode,
        content_kind: HostedContentKind,
    },
    DeleteOwnContent {
        mode: HostedPortMode,
        content_kind: HostedContentKind,
        delete_authority: DeleteAuthorityGrant,
    },
}

impl HostedSessionCommand {
    pub const fn mode(self) -> HostedPortMode {
        match self {
            Self::ScanOwnContent { mode, .. } | Self::DeleteOwnContent { mode, .. } => mode,
        }
    }

    pub const fn content_kind(self) -> HostedContentKind {
        match self {
            Self::ScanOwnContent { content_kind, .. }
            | Self::DeleteOwnContent { content_kind, .. } => content_kind,
        }
    }

    pub const fn operation(self) -> HostedPortOperation {
        match self {
            Self::ScanOwnContent { .. } => HostedPortOperation::ScanOwnItems,
            Self::DeleteOwnContent { .. } => HostedPortOperation::DeleteOwnItem,
        }
    }

    pub const fn authorize(self) -> Result<(), RunDeleteAuthorityError> {
        match self {
            Self::ScanOwnContent { mode, .. } => {
                RunDeleteAuthority::new(mode, None).authorize(HostedPortOperation::ScanOwnItems)
            }
            Self::DeleteOwnContent {
                mode,
                delete_authority,
                ..
            } => RunDeleteAuthority::new(mode, Some(delete_authority))
                .authorize(HostedPortOperation::DeleteOwnItem),
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum HostedSurfaceWidth {
    Narrow,
    Medium,
    Wide,
}

impl HostedSurfaceWidth {
    pub const fn rank(self) -> u8 {
        match self {
            Self::Narrow => 1,
            Self::Medium => 2,
            Self::Wide => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct HostedSurface {
    pub surface_id: &'static str,
    pub content_kind: HostedContentKind,
    pub width: HostedSurfaceWidth,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct HostedSurfaceManifest {
    pub surfaces: &'static [HostedSurface],
}

impl HostedSurfaceManifest {
    pub const fn surfaces(&self) -> &'static [HostedSurface] {
        self.surfaces
    }
}

pub const HOSTED_SURFACES_WIDE_FIRST: &[HostedSurface] = &[
    HostedSurface {
        surface_id: "gmail-web-conversation",
        content_kind: HostedContentKind::DirectMessage,
        width: HostedSurfaceWidth::Wide,
    },
    HostedSurface {
        surface_id: "discord-web-channel",
        content_kind: HostedContentKind::Post,
        width: HostedSurfaceWidth::Wide,
    },
    HostedSurface {
        surface_id: "telegram-web-chat",
        content_kind: HostedContentKind::DirectMessage,
        width: HostedSurfaceWidth::Medium,
    },
];

pub const HOSTED_SURFACE_MANIFEST: HostedSurfaceManifest = HostedSurfaceManifest {
    surfaces: HOSTED_SURFACES_WIDE_FIRST,
};

pub const fn hosted_surface_manifest() -> HostedSurfaceManifest {
    HOSTED_SURFACE_MANIFEST
}

pub const fn hosted_surfaces_wide_first() -> &'static [HostedSurface] {
    HOSTED_SURFACES_WIDE_FIRST
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DeleteAuthorityGrant {
    ExplicitUserAuthority,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum HostedPortOpenError {
    DeleteAuthorityRequired,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum HostedPort {
    ScanOnly,
    DeleteCapable {
        delete_authority: DeleteAuthorityGrant,
    },
}

impl HostedPort {
    pub const fn mode(self) -> HostedPortMode {
        match self {
            Self::ScanOnly => HostedPortMode::ScanOnly,
            Self::DeleteCapable { .. } => HostedPortMode::DeleteCapable,
        }
    }
}

pub fn open_hosted_port(
    mode: HostedPortMode,
    delete_authority: Option<DeleteAuthorityGrant>,
) -> Result<HostedPort, HostedPortOpenError> {
    match mode {
        HostedPortMode::ScanOnly => Ok(HostedPort::ScanOnly),
        HostedPortMode::DeleteCapable => {
            let delete_authority =
                delete_authority.ok_or(HostedPortOpenError::DeleteAuthorityRequired)?;
            Ok(HostedPort::DeleteCapable { delete_authority })
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RunDeleteAuthorityError {
    ScanOnlyMode,
    DeleteAuthorityRequired,
}

pub struct RunDeleteAuthority {
    mode: HostedPortMode,
    delete_authority: Option<DeleteAuthorityGrant>,
}

impl RunDeleteAuthority {
    pub const fn new(mode: HostedPortMode, delete_authority: Option<DeleteAuthorityGrant>) -> Self {
        Self {
            mode,
            delete_authority,
        }
    }

    pub const fn authorize(
        &self,
        operation: HostedPortOperation,
    ) -> Result<(), RunDeleteAuthorityError> {
        if !self.mode.allows(operation) {
            return Err(RunDeleteAuthorityError::ScanOnlyMode);
        }
        match operation {
            HostedPortOperation::ScanOwnItems => Ok(()),
            HostedPortOperation::DeleteOwnItem => {
                if self.delete_authority.is_some() {
                    Ok(())
                } else {
                    Err(RunDeleteAuthorityError::DeleteAuthorityRequired)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosted_port_mode_scan_only_refuses_delete_own_item() {
        assert!(HostedPortMode::ScanOnly.allows(HostedPortOperation::ScanOwnItems));
        assert!(HostedPortMode::DeleteCapable.allows(HostedPortOperation::ScanOwnItems));
        assert!(HostedPortMode::DeleteCapable.allows(HostedPortOperation::DeleteOwnItem));

        assert!(!HostedPortMode::ScanOnly.allows(HostedPortOperation::DeleteOwnItem));
    }

    #[test]
    fn open_delete_capable_hosted_port_requires_explicit_delete_authority() {
        assert_eq!(
            open_hosted_port(HostedPortMode::DeleteCapable, None),
            Err(HostedPortOpenError::DeleteAuthorityRequired)
        );

        let port = open_hosted_port(
            HostedPortMode::DeleteCapable,
            Some(DeleteAuthorityGrant::ExplicitUserAuthority),
        )
        .expect("explicit delete authority opens a delete-capable port");
        assert_eq!(port.mode(), HostedPortMode::DeleteCapable);
    }

    #[test]
    fn run_delete_authority_scan_only_kill_switch_blocks_delete_own_item() {
        let scan_only = RunDeleteAuthority::new(
            HostedPortMode::ScanOnly,
            Some(DeleteAuthorityGrant::ExplicitUserAuthority),
        );
        assert_eq!(
            scan_only.authorize(HostedPortOperation::DeleteOwnItem),
            Err(RunDeleteAuthorityError::ScanOnlyMode)
        );
        assert_eq!(
            scan_only.authorize(HostedPortOperation::ScanOwnItems),
            Ok(())
        );

        let delete_capable = RunDeleteAuthority::new(
            HostedPortMode::DeleteCapable,
            Some(DeleteAuthorityGrant::ExplicitUserAuthority),
        );
        assert_eq!(
            delete_capable.authorize(HostedPortOperation::DeleteOwnItem),
            Ok(())
        );
    }

    #[test]
    fn hosted_content_kind_parses_only_direct_message_and_post() {
        assert_eq!(
            HostedContentKind::parse("direct-message"),
            Some(HostedContentKind::DirectMessage)
        );
        assert_eq!(
            HostedContentKind::parse("post"),
            Some(HostedContentKind::Post)
        );

        for rejected in [
            "direct_message",
            "dm",
            "message",
            "thread",
            "comment",
            "post ",
            "DirectMessage",
        ] {
            assert_eq!(HostedContentKind::parse(rejected), None);
        }
        assert_eq!(
            HostedContentKind::DirectMessage.wire_name(),
            "direct-message"
        );
        assert_eq!(HostedContentKind::Post.wire_name(), "post");
    }

    #[test]
    fn hosted_port_mode_and_session_command_contract_is_fixed() {
        assert_eq!(HostedPortMode::ScanOnly.wire_name(), "scan_only");
        assert_eq!(HostedPortMode::DeleteCapable.wire_name(), "delete_capable");

        let scan = HostedSessionCommand::ScanOwnContent {
            mode: HostedPortMode::ScanOnly,
            content_kind: HostedContentKind::DirectMessage,
        };
        assert_eq!(scan.mode(), HostedPortMode::ScanOnly);
        assert_eq!(scan.content_kind(), HostedContentKind::DirectMessage);
        assert_eq!(scan.operation(), HostedPortOperation::ScanOwnItems);
        assert_eq!(scan.authorize(), Ok(()));

        let delete = HostedSessionCommand::DeleteOwnContent {
            mode: HostedPortMode::DeleteCapable,
            content_kind: HostedContentKind::Post,
            delete_authority: DeleteAuthorityGrant::ExplicitUserAuthority,
        };
        assert_eq!(delete.mode(), HostedPortMode::DeleteCapable);
        assert_eq!(delete.content_kind(), HostedContentKind::Post);
        assert_eq!(delete.operation(), HostedPortOperation::DeleteOwnItem);
        assert_eq!(delete.authorize(), Ok(()));

        let refused = HostedSessionCommand::DeleteOwnContent {
            mode: HostedPortMode::ScanOnly,
            content_kind: HostedContentKind::Post,
            delete_authority: DeleteAuthorityGrant::ExplicitUserAuthority,
        };
        assert_eq!(
            refused.authorize(),
            Err(RunDeleteAuthorityError::ScanOnlyMode)
        );
    }

    #[test]
    fn hosted_surface_manifest_lists_surfaces_widest_first() {
        let manifest = hosted_surface_manifest();
        assert_eq!(manifest.surfaces(), hosted_surfaces_wide_first());
        assert_eq!(manifest.surfaces().len(), 3);

        assert_eq!(
            manifest
                .surfaces()
                .iter()
                .map(|surface| surface.surface_id)
                .collect::<Vec<_>>(),
            vec![
                "gmail-web-conversation",
                "discord-web-channel",
                "telegram-web-chat"
            ]
        );
        assert_eq!(
            manifest
                .surfaces()
                .iter()
                .map(|surface| surface.width.rank())
                .collect::<Vec<_>>(),
            vec![3, 3, 2]
        );
        assert!(manifest
            .surfaces()
            .windows(2)
            .all(|pair| pair[0].width.rank() >= pair[1].width.rank()));
        assert!(manifest
            .surfaces()
            .iter()
            .any(|surface| surface.content_kind == HostedContentKind::DirectMessage));
        assert!(manifest
            .surfaces()
            .iter()
            .any(|surface| surface.content_kind == HostedContentKind::Post));
    }
}
