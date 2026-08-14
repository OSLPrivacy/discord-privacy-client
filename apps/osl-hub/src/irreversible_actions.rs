//! Shipping ask-step adapters for the hub's irreversible actions.

use ipc::irreversible_action::{run_irreversible_action, IrreversibleActionAnswer};

use crate::core_bridge::HubCoreState;
use crate::security::{self, HubSecurityState, ResetEverySettingRecord};
use crate::shared_mail_deleter::{
    delete_marked_mail_message, SharedMailDeleteError, SharedMailDeleteReceipt,
    SharedMailDeleteRequest, SharedMailTrashSurface,
};
use crate::shared_marked_message_deleter::{
    delete_marked_messages, SharedMarkedDeletionError, SharedMarkedDeletionReport,
    SharedMarkedDeletionRequest, SharedMarkedMessageRemover,
};

/// Gate the service-neutral Scrub mutation before the remover is called.
pub fn delete_scrub_marked_messages<R>(
    state: &ipc::AppState,
    remover: &mut R,
    request: SharedMarkedDeletionRequest,
    confirmed: bool,
) -> Result<IrreversibleActionAnswer<SharedMarkedDeletionReport>, SharedMarkedDeletionError>
where
    R: SharedMarkedMessageRemover + ?Sized,
{
    run_irreversible_action(state, confirmed, || {
        delete_marked_messages(remover, request)
    })
}

/// Gate the mail Scrub mutation before either move-to-trash step runs.
pub fn delete_scrub_marked_mail_message(
    state: &ipc::AppState,
    surface: &mut impl SharedMailTrashSurface,
    request: &SharedMailDeleteRequest,
    confirmed: bool,
) -> Result<IrreversibleActionAnswer<SharedMailDeleteReceipt>, SharedMailDeleteError> {
    run_irreversible_action(state, confirmed, || {
        delete_marked_mail_message(surface, request)
    })
}

/// Gate the complete settings reset before it obtains any reset or file lock.
pub fn reset_every_setting(
    core: &HubCoreState,
    security_state: &HubSecurityState,
    confirmed: bool,
) -> Result<IrreversibleActionAnswer<ResetEverySettingRecord>, String> {
    run_irreversible_action(&core.osl, confirmed, || {
        security::reset_every_setting(core, security_state)
    })
}
