//! TASK 3505a: OSL Chats reaches the one shipping receive job through the
//! common arrived-row shape.  This test has no saved conversation and never
//! calls an opener directly; its only arrival is the session route below.

use osl_privacy_hub::osl_chat_delivery::route_osl_chat_arrival;
use osl_privacy_hub::shipping_receive::{ShippingReceiveJournal, ShippingService};

struct LiveOslChatsSession<'a> {
    journal: &'a mut ShippingReceiveJournal,
}

impl LiveOslChatsSession<'_> {
    fn arrived(&mut self, carrier_row_id: &str, wire: Vec<u8>) -> Result<(), String> {
        route_osl_chat_arrival(carrier_row_id, wire, self.journal, |_row| Ok(()))
    }
}

#[test]
fn task_3505a_live_osl_chats_arrival_uses_the_single_instrumented_job() {
    let mut journal = ShippingReceiveJournal::default();
    let mut session = LiveOslChatsSession {
        journal: &mut journal,
    };

    session
        .arrived("live-osl-chats-row-3505a", b"authenticated wire".to_vec())
        .expect("a live OSL Chats arrival reaches the shipping job");

    assert_eq!(journal.services(), &[ShippingService::OslChats]);
    assert_eq!(journal.carrier_row_ids(), &["live-osl-chats-row-3505a"]);

    let source = include_str!("../src/osl_chat_delivery.rs");
    assert!(
        source.contains("ArrivedMessageRow::osl_chats")
            && source.contains("shipping_receive::receive_arrived_message"),
        "OSL Chats must shape its row and call the single shipping entry point"
    );
    assert!(
        !source.contains("fn receive_osl_chat"),
        "an OSL Chats-specific receive reader would bypass the shared shipping job"
    );
}
