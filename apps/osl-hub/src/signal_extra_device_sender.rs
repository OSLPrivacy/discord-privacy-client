//! Signal linked-device sender route.
//!
//! This route is intentionally separate from the Signal Desktop accessibility
//! adapter. It models the same user-approved linked-device authority Signal
//! Desktop receives from a phone QR scan, then sends through that linked device
//! without a screen, focused window, composer, keyboard input, or foreground UI.

use serde::Serialize;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

const MAX_SIGNAL_MESSAGE_BYTES: usize = 4_096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalLinkedDevicePairing {
    pub owner_account_name: String,
    pub linked_device_name: String,
    pub scan_marker: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalDirectSendRequest {
    pub owner_account_name: String,
    pub recipient_account_name: String,
    pub marked_message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalDirectSendReceipt {
    pub ok: bool,
    pub route: &'static str,
    pub owner_account_name: String,
    pub recipient_account_name: String,
    pub sent_count: usize,
    pub refused_by_name: Option<String>,
    pub delivered_words: Option<String>,
}

#[derive(Default)]
pub struct SignalExtraDeviceSender {
    pairings: BTreeMap<String, SignalLinkedDevicePairing>,
}

pub trait SignalLinkedDeviceTransport {
    fn deliver_exact_words(
        &mut self,
        linked_device: &SignalLinkedDevicePairing,
        recipient_account_name: &str,
        words: &str,
    ) -> Result<(), SignalSendRefusal>;
}

/// The command line needed to operate a linked Signal device.
///
/// Keeping the executable and arguments as data makes the production route
/// reviewable: it is a direct `signal-cli` invocation, never a shell string,
/// desktop accessibility operation, keyboard event, or foreground window.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalCliCommand {
    executable: PathBuf,
    arguments: Vec<OsString>,
}

impl SignalCliCommand {
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
    }
}

/// A real linked-device transport backed by `signal-cli`.
///
/// Pair this data directory using [`Self::link_command`], render the returned
/// `sgnl://linkdevice` URI as a QR code, and let the account holder scan it in
/// Signal exactly as they would scan the Signal Desktop pairing code.  Once
/// linked, [`SignalLinkedDeviceTransport::deliver_exact_words`] uses the
/// linked-device keys held by `signal-cli`; it does not open or drive Signal
/// Desktop.
///
/// `signal-cli` is an external, account-holder-provisioned dependency.  A
/// missing executable, failed command, or response without the Signal-issued
/// timestamp is a refusal, never a claimed send.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalCliLinkedDeviceTransport {
    executable: PathBuf,
    data_directory: PathBuf,
}

impl SignalCliLinkedDeviceTransport {
    pub fn new(executable: impl Into<PathBuf>, data_directory: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            data_directory: data_directory.into(),
        }
    }

    /// Start Signal's normal linked-device pairing flow.  The caller owns QR
    /// rendering and must wait for the user's scan before storing a pairing.
    pub fn link_command(
        &self,
        linked_device_name: &str,
    ) -> Result<SignalCliCommand, SignalSendRefusal> {
        if !valid_name(linked_device_name) {
            return Err(SignalSendRefusal::InvalidPairing);
        }
        Ok(self.command(["link", "--name", linked_device_name]))
    }

    /// Build the exact non-interactive send command.  This is public for the
    /// live QA harness so its emitted command can be audited without revealing
    /// the linked-device data directory or message contents in ordinary logs.
    pub fn send_command(
        &self,
        linked_device: &SignalLinkedDevicePairing,
        recipient_account_name: &str,
        words: &str,
    ) -> Result<SignalCliCommand, SignalSendRefusal> {
        if !valid_name(&linked_device.owner_account_name)
            || !valid_name(recipient_account_name)
            || !valid_message(words)
        {
            return Err(SignalSendRefusal::InvalidMessage);
        }
        Ok(self.command([
            "--account",
            linked_device.owner_account_name.as_str(),
            "send",
            "--message",
            words,
            recipient_account_name,
        ]))
    }

    fn command<const N: usize>(&self, command_arguments: [&str; N]) -> SignalCliCommand {
        let mut arguments = vec![
            OsString::from("--data-dir"),
            self.data_directory.as_os_str().to_owned(),
            OsString::from("--output"),
            OsString::from("json"),
        ];
        arguments.extend(command_arguments.into_iter().map(OsString::from));
        SignalCliCommand {
            executable: self.executable.clone(),
            arguments,
        }
    }
}

impl SignalLinkedDeviceTransport for SignalCliLinkedDeviceTransport {
    fn deliver_exact_words(
        &mut self,
        linked_device: &SignalLinkedDevicePairing,
        recipient_account_name: &str,
        words: &str,
    ) -> Result<(), SignalSendRefusal> {
        let command = self.send_command(linked_device, recipient_account_name, words)?;
        let output = Command::new(command.executable)
            .args(command.arguments)
            .output()
            .map_err(|_| SignalSendRefusal::TransportUnavailable)?;
        if !output.status.success() || !signal_cli_issued_timestamp(&output.stdout) {
            return Err(SignalSendRefusal::TransportUnavailable);
        }
        Ok(())
    }
}

/// A successful `signal-cli --output json send` response carries a Signal
/// server timestamp.  Require that evidence as well as process success: a
/// local process exit alone is not a send receipt.
fn signal_cli_issued_timestamp(stdout: &[u8]) -> bool {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(stdout) else {
        return false;
    };
    let Some(object) = value.as_object() else {
        return false;
    };
    if object.contains_key("error") {
        return false;
    }
    object
        .get("timestamp")
        .or_else(|| object.get("result")?.get("timestamp"))
        .and_then(serde_json::Value::as_u64)
        .is_some_and(|timestamp| timestamp > 0)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignalSendRefusal {
    InvalidPairing,
    InvalidRecipient,
    InvalidMessage,
    TransportUnavailable,
}

impl SignalSendRefusal {
    fn user_message(self) -> &'static str {
        match self {
            Self::InvalidPairing => "Signal extra-device pairing is invalid",
            Self::InvalidRecipient => "Signal recipient account is invalid",
            Self::InvalidMessage => "Signal direct message is invalid",
            Self::TransportUnavailable => "Signal extra-device transport is unavailable",
        }
    }
}

impl SignalExtraDeviceSender {
    pub fn pair_from_user_scan(
        &mut self,
        pairing: SignalLinkedDevicePairing,
    ) -> Result<(), SignalSendRefusal> {
        if !valid_name(&pairing.owner_account_name)
            || !valid_name(&pairing.linked_device_name)
            || !valid_name(&pairing.scan_marker)
        {
            return Err(SignalSendRefusal::InvalidPairing);
        }
        self.pairings
            .insert(pairing.owner_account_name.clone(), pairing);
        Ok(())
    }

    pub fn remove_pairing(&mut self, owner_account_name: &str) -> bool {
        self.pairings.remove(owner_account_name).is_some()
    }

    pub fn send_direct_command(
        &self,
        transport: &mut dyn SignalLinkedDeviceTransport,
        request: SignalDirectSendRequest,
    ) -> SignalDirectSendReceipt {
        if !valid_name(&request.recipient_account_name) || !valid_message(&request.marked_message) {
            return refused_receipt(
                request,
                SignalSendRefusal::InvalidMessage.user_message().to_owned(),
            );
        }
        let Some(pairing) = self.pairings.get(&request.owner_account_name) else {
            let refused_by_name = format!(
                "Signal extra-device pairing removed: {}",
                request.owner_account_name
            );
            return refused_receipt(request, refused_by_name);
        };
        match transport.deliver_exact_words(
            pairing,
            &request.recipient_account_name,
            &request.marked_message,
        ) {
            Ok(()) => SignalDirectSendReceipt {
                ok: true,
                route: "signal-extra-device",
                owner_account_name: request.owner_account_name,
                recipient_account_name: request.recipient_account_name,
                sent_count: 1,
                refused_by_name: None,
                delivered_words: Some(request.marked_message),
            },
            Err(error) => refused_receipt(request, error.user_message().to_owned()),
        }
    }
}

fn refused_receipt(
    request: SignalDirectSendRequest,
    refused_by_name: String,
) -> SignalDirectSendReceipt {
    SignalDirectSendReceipt {
        ok: false,
        route: "signal-extra-device",
        owner_account_name: request.owner_account_name,
        recipient_account_name: request.recipient_account_name,
        sent_count: 0,
        refused_by_name: Some(refused_by_name),
        delivered_words: None,
    }
}

fn valid_name(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(char::is_control)
}

fn valid_message(value: &str) -> bool {
    valid_name(value) && value.len() <= MAX_SIGNAL_MESSAGE_BYTES
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;

    #[derive(Default)]
    struct FakeSignalAccountNetwork {
        inboxes: BTreeMap<String, Vec<String>>,
        sent_total: usize,
    }

    impl FakeSignalAccountNetwork {
        fn account(&mut self, account_name: &str) {
            self.inboxes.entry(account_name.to_owned()).or_default();
        }

        fn received_exact_words(&self, account_name: &str) -> Option<&str> {
            self.inboxes
                .get(account_name)
                .and_then(|messages| messages.last())
                .map(String::as_str)
        }
    }

    impl SignalLinkedDeviceTransport for FakeSignalAccountNetwork {
        fn deliver_exact_words(
            &mut self,
            linked_device: &SignalLinkedDevicePairing,
            recipient_account_name: &str,
            words: &str,
        ) -> Result<(), SignalSendRefusal> {
            if !valid_name(&linked_device.scan_marker) {
                return Err(SignalSendRefusal::InvalidPairing);
            }
            let inbox = self
                .inboxes
                .get_mut(recipient_account_name)
                .ok_or(SignalSendRefusal::InvalidRecipient)?;
            inbox.push(words.to_owned());
            self.sent_total += 1;
            Ok(())
        }
    }

    #[test]
    fn task1037a_extra_device_signal_sender_sends_after_pairing_and_refuses_after_removal() {
        let owner = "Signal Owner TASK1037a";
        let recipient = "Signal Recipient TASK1037a";
        let marker = rand::thread_rng().next_u64();
        let words = format!("OSL-1037a-MARKED-{marker:016x}");

        let mut sender = SignalExtraDeviceSender::default();
        let mut network = FakeSignalAccountNetwork::default();
        network.account(recipient);

        sender
            .pair_from_user_scan(SignalLinkedDevicePairing {
                owner_account_name: owner.to_owned(),
                linked_device_name: "OSL extra linked device TASK1037a".to_owned(),
                scan_marker: "phone-scan-same-as-signal-desktop TASK1037a".to_owned(),
            })
            .expect("user scan links the extra device");

        let sent = sender.send_direct_command(
            &mut network,
            SignalDirectSendRequest {
                owner_account_name: owner.to_owned(),
                recipient_account_name: recipient.to_owned(),
                marked_message: words.clone(),
            },
        );

        assert!(sent.ok);
        assert_eq!(sent.route, "signal-extra-device");
        assert_eq!(sent.sent_count, 1);
        assert_eq!(sent.delivered_words.as_deref(), Some(words.as_str()));
        assert_eq!(
            network.received_exact_words(recipient),
            Some(words.as_str())
        );
        assert_eq!(network.sent_total, 1);
        println!(
            "TASK1037a paired route={} owner=\"{}\" recipient=\"{}\" sent_count={} message=\"{}\" received_exact=\"{}\"",
            sent.route,
            sent.owner_account_name,
            sent.recipient_account_name,
            sent.sent_count,
            words,
            network.received_exact_words(recipient).unwrap()
        );

        assert!(sender.remove_pairing(owner));
        let refused = sender.send_direct_command(
            &mut network,
            SignalDirectSendRequest {
                owner_account_name: owner.to_owned(),
                recipient_account_name: recipient.to_owned(),
                marked_message: words.clone(),
            },
        );

        assert!(!refused.ok);
        assert_eq!(refused.sent_count, 0);
        assert_eq!(
            refused.refused_by_name.as_deref(),
            Some("Signal extra-device pairing removed: Signal Owner TASK1037a")
        );
        assert_eq!(network.sent_total, 1);
        println!(
            "TASK1037a removed_pairing refused_by_name=\"{}\" sent_count={} transport_sent_total={}",
            refused.refused_by_name.as_deref().unwrap(),
            refused.sent_count,
            network.sent_total
        );
    }

    #[test]
    fn task1037a_signal_cli_route_is_noninteractive_and_requires_signal_timestamp() {
        let transport = SignalCliLinkedDeviceTransport::new(
            "/opt/osl/signal-cli/bin/signal-cli",
            "/var/lib/osl/signal-extra-device",
        );
        let pairing = SignalLinkedDevicePairing {
            owner_account_name: "+15550001037".to_owned(),
            linked_device_name: "OSL extra linked device TASK1037a".to_owned(),
            scan_marker: "phone-scan-same-as-signal-desktop TASK1037a".to_owned(),
        };
        let link = transport
            .link_command(&pairing.linked_device_name)
            .expect("a named linked-device pairing command");
        let command = transport
            .send_command(&pairing, "+15550002037", "OSL-1037a-MARKED-CLI")
            .expect("a direct linked-device command");

        let link_arguments: Vec<_> = link
            .arguments()
            .iter()
            .map(|argument| argument.to_string_lossy())
            .collect();
        let arguments: Vec<_> = command
            .arguments()
            .iter()
            .map(|argument| argument.to_string_lossy())
            .collect();
        assert_eq!(
            link_arguments,
            [
                "--data-dir",
                "/var/lib/osl/signal-extra-device",
                "--output",
                "json",
                "link",
                "--name",
                "OSL extra linked device TASK1037a",
            ]
        );
        assert_eq!(
            arguments,
            [
                "--data-dir",
                "/var/lib/osl/signal-extra-device",
                "--output",
                "json",
                "--account",
                "+15550001037",
                "send",
                "--message",
                "OSL-1037a-MARKED-CLI",
                "+15550002037",
            ]
        );
        assert!(!arguments.iter().any(|argument| {
            argument.contains("stdin")
                || argument.contains("window")
                || argument.contains("keyboard")
                || argument.contains("desktop")
        }));
        assert!(signal_cli_issued_timestamp(
            br#"{"result":{"timestamp":1037}}"#
        ));
        assert!(!signal_cli_issued_timestamp(br#"{"result":{}}"#));
        assert!(!signal_cli_issued_timestamp(
            br#"{"error":{"timestamp":1037}}"#
        ));
        assert!(!signal_cli_issued_timestamp(b"not Signal JSON"));
        println!(
            "TASK1037a signal_cli route=signal-extra-device link_args={} send_args={} signal_timestamp=true no_ui_arguments=true",
            link_arguments.len(),
            arguments.len()
        );
    }

    #[test]
    fn task1037a_signal_cli_missing_transport_refuses_without_counting_a_send() {
        let owner = "+15550001037";
        let mut sender = SignalExtraDeviceSender::default();
        sender
            .pair_from_user_scan(SignalLinkedDevicePairing {
                owner_account_name: owner.to_owned(),
                linked_device_name: "OSL extra linked device TASK1037a".to_owned(),
                scan_marker: "phone-scan-same-as-signal-desktop TASK1037a".to_owned(),
            })
            .expect("the pairing record is present before the transport check");
        let mut missing_transport = SignalCliLinkedDeviceTransport::new(
            "/definitely-not-installed/osl-signal-cli",
            "/var/lib/osl/signal-extra-device",
        );

        let receipt = sender.send_direct_command(
            &mut missing_transport,
            SignalDirectSendRequest {
                owner_account_name: owner.to_owned(),
                recipient_account_name: "+15550002037".to_owned(),
                marked_message: "OSL-1037a-MARKED-MISSING-TRANSPORT".to_owned(),
            },
        );

        assert!(!receipt.ok);
        assert_eq!(receipt.sent_count, 0);
        assert_eq!(
            receipt.refused_by_name.as_deref(),
            Some("Signal extra-device transport is unavailable")
        );
        println!(
            "TASK1037a missing_signal_cli refused_by_name=\"{}\" sent_count={}",
            receipt.refused_by_name.as_deref().unwrap(),
            receipt.sent_count
        );
    }
}
