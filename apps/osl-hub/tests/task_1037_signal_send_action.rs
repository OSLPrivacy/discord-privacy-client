#![cfg_attr(task1037_direct, allow(dead_code))]

#[cfg(task1037_direct)]
mod adapters {
    use std::collections::BTreeSet;

    pub type CapabilitySet = BTreeSet<adapter_profile::Capability>;
}

#[cfg(task1037_direct)]
#[path = "../examples/task_3406_place_text.rs"]
mod shared_place_text;

#[cfg(task1037_direct)]
#[path = "../src/signal_send_action.rs"]
mod signal_send_action;

#[cfg(task1037_direct)]
use crate::shared_place_text::SharedPlaceTextActions;
#[cfg(task1037_direct)]
use crate::signal_send_action::{
    send_signal_cover_direct_command, SelectedSignalSendRoute, SignalCoverInsertion,
    SignalSendActionError, SignalSendTrigger,
};
#[cfg(not(task1037_direct))]
use osl_privacy_hub::shared_place_text::SharedPlaceTextActions;
#[cfg(not(task1037_direct))]
use osl_privacy_hub::signal_send_action::{
    send_signal_cover_direct_command, SelectedSignalSendRoute, SignalCoverInsertion,
    SignalSendActionError, SignalSendTrigger,
};

use adapter_profile::Capability;
use std::collections::BTreeSet;

#[derive(Default)]
struct OpenSignalConversation {
    composer: String,
    sent_messages: Vec<String>,
    place_calls: usize,
    send_calls: usize,
    real_send_capability: bool,
}

impl OpenSignalConversation {
    fn real_route() -> Self {
        Self {
            real_send_capability: true,
            ..Self::default()
        }
    }
}

impl SharedPlaceTextActions for OpenSignalConversation {
    fn read_back_text(&mut self) -> Result<String, String> {
        Ok(self.composer.clone())
    }

    fn paste_text(&mut self, text: &str) -> Result<(), String> {
        self.place_calls += 1;
        self.composer = text.to_owned();
        Ok(())
    }

    fn editor_accepts_message(&mut self) -> Result<bool, String> {
        Ok(!self.composer.is_empty())
    }
}

impl SelectedSignalSendRoute for OpenSignalConversation {
    fn route_name(&self) -> &'static str {
        "SIGNAL-DESKTOP-SCREEN-ROUTE"
    }

    fn capabilities(&self) -> BTreeSet<Capability> {
        let mut capabilities = BTreeSet::new();
        capabilities.insert(Capability::PlaceProtectedPayload);
        if self.real_send_capability {
            capabilities.insert(Capability::SendProtectedPayload);
        }
        capabilities
    }

    fn open_conversation_sent_count(&self) -> Result<usize, String> {
        Ok(self.sent_messages.len())
    }

    fn send_open_conversation(&mut self) -> Result<(), String> {
        assert!(
            self.real_send_capability,
            "a route without send capability must never reach send"
        );
        if self.composer.is_empty() {
            return Err("Signal composer is empty".to_owned());
        }
        self.send_calls += 1;
        self.sent_messages.push(std::mem::take(&mut self.composer));
        Ok(())
    }
}

#[test]
fn task_1037_exact_triggers_each_send_once_through_selected_real_route() {
    let trigger_names = ["Enter", "Enter x2", "Clipboard"];
    assert_eq!(
        SignalSendTrigger::ALL.map(SignalSendTrigger::name),
        trigger_names,
        "Signal must expose exactly the three reviewed trigger names"
    );
    assert_eq!(
        SignalCoverInsertion::ALL.map(SignalCoverInsertion::name),
        ["Insert on send", "Type naturally"],
        "cover insertion must remain a separate setting"
    );
    println!("TASK1037_TRIGGER_NAMES=Enter|Enter x2|Clipboard trigger_count=3");
    println!("TASK1037_COVER_INSERTION_NAMES=Insert on send|Type naturally insertion_count=2");

    for (index, trigger_name) in trigger_names.into_iter().enumerate() {
        let insertion = SignalCoverInsertion::ALL[index % SignalCoverInsertion::ALL.len()];
        let marked_cover = format!("OSL1.SIGNAL.T1037-{index}-🦊");
        let mut route = OpenSignalConversation::real_route();
        assert_eq!(route.sent_messages.len(), 0);

        let receipt = send_signal_cover_direct_command(
            Some(&mut route),
            trigger_name,
            insertion,
            &marked_cover,
        )
        .unwrap_or_else(|error| panic!("{trigger_name} must send: {error}"));

        assert_eq!(receipt.trigger.name(), trigger_name);
        assert_eq!(receipt.cover_insertion, insertion);
        assert_eq!(receipt.selected_route, "SIGNAL-DESKTOP-SCREEN-ROUTE");
        assert!(receipt.placement.readback_exact);
        assert!(receipt.placement.editor_accepts_message);
        assert_eq!(receipt.sent_count_before, 0);
        assert_eq!(receipt.sent_count_after, 1);
        assert_eq!(route.sent_messages, [marked_cover]);
        assert_eq!(route.place_calls, 1);
        assert_eq!(route.send_calls, 1);
        assert!(route.composer.is_empty());
        println!(
            "TASK1037_TRIGGER={} cover_insertion={} selected_route={} real_send_capability=SendProtectedPayload sent_count={}->{} place_calls={} send_calls={}",
            receipt.trigger.name(),
            receipt.cover_insertion.name(),
            receipt.selected_route,
            receipt.sent_count_before,
            receipt.sent_count_after,
            route.place_calls,
            route.send_calls,
        );
    }
}

#[test]
fn task_1037_rejects_every_non_trigger_and_routes_without_real_send_capability() {
    let refused_names = [
        "Manual",
        "Instant",
        "Match typing",
        "Insert on send",
        "Type naturally",
    ];
    for name in refused_names {
        let mut route = OpenSignalConversation::real_route();
        assert_eq!(
            send_signal_cover_direct_command(
                Some(&mut route),
                name,
                SignalCoverInsertion::InsertOnSend,
                "must not place",
            ),
            Err(SignalSendActionError::RefusedTriggerName(name.to_owned()))
        );
        assert_eq!(route.sent_messages.len(), 0);
        assert_eq!(route.place_calls, 0);
        assert_eq!(route.send_calls, 0);
        println!("TASK1037_REJECTED_TRIGGER={name} category=refused");
    }

    for name in ["", "Double Enter", "Enter ", "clipboard", "Escape"] {
        let mut route = OpenSignalConversation::real_route();
        assert_eq!(
            send_signal_cover_direct_command(
                Some(&mut route),
                name,
                SignalCoverInsertion::TypeNaturally,
                "must not place",
            ),
            Err(SignalSendActionError::UnknownTriggerName(name.to_owned()))
        );
        assert_eq!(route.sent_messages.len(), 0);
        assert_eq!(route.place_calls, 0);
        assert_eq!(route.send_calls, 0);
        println!("TASK1037_REJECTED_TRIGGER={name:?} category=unknown");
    }

    let no_route = send_signal_cover_direct_command(
        None::<&mut OpenSignalConversation>,
        "Enter",
        SignalCoverInsertion::InsertOnSend,
        "must not place",
    );
    assert_eq!(no_route, Err(SignalSendActionError::NoSelectedRoute));

    let mut incapable_route = OpenSignalConversation::default();
    let incapable = send_signal_cover_direct_command(
        Some(&mut incapable_route),
        "Clipboard",
        SignalCoverInsertion::TypeNaturally,
        "must not place or send",
    );
    assert_eq!(
        incapable,
        Err(SignalSendActionError::SelectedRouteCannotSend(
            "SIGNAL-DESKTOP-SCREEN-ROUTE".to_owned()
        ))
    );
    assert_eq!(incapable_route.sent_messages.len(), 0);
    assert_eq!(incapable_route.place_calls, 0);
    assert_eq!(incapable_route.send_calls, 0);
    println!("TASK1037_NO_ROUTE sent_count=0 result=Signal route not selected");
    println!(
        "TASK1037_NO_REAL_CAPABILITY sent_count={} place_calls={} send_calls={} result=no real send capability",
        incapable_route.sent_messages.len(),
        incapable_route.place_calls,
        incapable_route.send_calls,
    );
}
