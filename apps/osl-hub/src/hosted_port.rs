//! Minimal authority model for hosted ports.
//!
//! A scan-only hosted port may observe content-free row metadata, but it must
//! never inherit deletion authority from the generic hosted-port shape.

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum HostedPortMode {
    ScanOnly,
    DeleteCapable,
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
}
