use crate::service_host::ActiveServiceHost;

pub const ALLOWED_PLACE_CHECK_STAGE: &str = "allowed-place-check";
pub const ALLOWED_PLACE_CONFIRMED_STAGE: &str = "allowed-place-confirmed";
pub const PROTECTED_MESSAGE_PATH_STAGE: &str = "protected-message-path";

pub const OPEN_NATIVE_DISCORD_OVERLAY_TEXT_COMMAND: &str = "open_native_discord_overlay_text";
pub const NATIVE_DISCORD_OVERLAY_TEXT_COMMAND_CALLER_LABEL: &str = "native-discord-overlay";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeDiscordOverlayTextCommandResult<Opened> {
    pub command_called: &'static str,
    pub opened: Opened,
}

pub fn with_allowed_place_before_protected_message_path<Allowed, Output, CheckAllowed, Protected>(
    trace: &mut Vec<&'static str>,
    check_allowed_place: CheckAllowed,
    protected_message_path: Protected,
) -> Result<Output, String>
where
    CheckAllowed: FnOnce() -> Result<Allowed, String>,
    Protected: FnOnce(Allowed) -> Result<Output, String>,
{
    trace.push(ALLOWED_PLACE_CHECK_STAGE);
    let allowed = check_allowed_place()?;
    trace.push(ALLOWED_PLACE_CONFIRMED_STAGE);
    trace.push(PROTECTED_MESSAGE_PATH_STAGE);
    protected_message_path(allowed)
}

pub fn open_native_discord_overlay_text_command_flow<
    Opened,
    SnapshotContext,
    Drain,
    RecordPoll,
    RecheckContext,
    RecordOpened,
>(
    caller_label: &str,
    snapshot_context: SnapshotContext,
    drain: Drain,
    record_poll: RecordPoll,
    mut recheck_context: RecheckContext,
    record_opened: RecordOpened,
) -> Result<NativeDiscordOverlayTextCommandResult<Opened>, String>
where
    SnapshotContext: FnOnce() -> Result<(u64, ActiveServiceHost), String>,
    Drain: FnOnce() -> Result<Opened, String>,
    RecordPoll: FnOnce(Result<&Opened, &str>) -> Result<(), String>,
    RecheckContext: FnMut(u64, &ActiveServiceHost) -> Result<(), String>,
    RecordOpened: FnOnce(&Opened) -> Result<(), String>,
{
    if caller_label != NATIVE_DISCORD_OVERLAY_TEXT_COMMAND_CALLER_LABEL {
        return Err("Only the trusted native Discord overlay may receive text".to_owned());
    }
    let (context_epoch, host) = snapshot_context()?;
    let opened = drain();
    record_poll(opened.as_ref().map_err(String::as_str))?;
    let opened = opened?;
    recheck_context(context_epoch, &host)?;
    record_opened(&opened)?;
    Ok(NativeDiscordOverlayTextCommandResult {
        command_called: OPEN_NATIVE_DISCORD_OVERLAY_TEXT_COMMAND,
        opened,
    })
}
