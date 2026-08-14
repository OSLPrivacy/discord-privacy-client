use osl_privacy_hub::native_whatsapp_adapter::{
    DiscoveredWhatsAppPair, TrustedWhatsAppContentRoot, WhatsAppBounds, WhatsAppComposerBinding,
    WhatsAppTranscriptBinding,
};
use osl_privacy_hub::whatsapp_place_reader::{
    inspect_whatsapp_place, WhatsAppGroupAllowance, WhatsAppPlace, WhatsAppPlaceKind,
};

fn discovered_whatsapp_surface() -> DiscoveredWhatsAppPair {
    DiscoveredWhatsAppPair {
        content_root: TrustedWhatsAppContentRoot {
            app_root_id: "whatsapp-root-1089c".to_owned(),
            webview_ancestor_id: "whatsapp-webview-1089c".to_owned(),
            content_root_id: "whatsapp-content-1089c".to_owned(),
            content_runtime_hash: "runtime-1089c".to_owned(),
        },
        composer: WhatsAppComposerBinding {
            node_id: "whatsapp-composer-1089c".to_owned(),
            runtime_hash: "composer-runtime-1089c".to_owned(),
            bounds: WhatsAppBounds {
                x: 20,
                y: 400,
                width: 600,
                height: 40,
            },
        },
        transcript: WhatsAppTranscriptBinding {
            node_id: "whatsapp-transcript-1089c".to_owned(),
            runtime_hash: "transcript-runtime-1089c".to_owned(),
            bounds: WhatsAppBounds {
                x: 20,
                y: 20,
                width: 600,
                height: 360,
            },
        },
    }
}

#[test]
fn task_1089c_broadcast_list_stays_separate_from_group_allowances() {
    let surface = discovered_whatsapp_surface();
    let no_op_reader = std::env::var("TASK1089C_WHATSAPP_PLACE_READER").as_deref() == Ok("noop");
    let mut reader_calls = 0usize;
    let mut reader = |_: &_| {
        reader_calls += 1;
        (!no_op_reader).then(|| WhatsAppPlace {
            kind: WhatsAppPlaceKind::BroadcastList,
            // Deliberately collide with an allowed group identifier. Kind
            // separation, not a convenient identifier mismatch, must keep the
            // group allowance from leaking into the broadcast list.
            stable_conversation_id: "whatsapp-conversation-1089c".to_owned(),
        })
    };
    let group_allowances = vec![WhatsAppGroupAllowance {
        stable_conversation_id: "whatsapp-conversation-1089c".to_owned(),
    }];
    let allowance_count_before = group_allowances.len();
    let mut group_reader = |_: &_| {
        Some(WhatsAppPlace {
            kind: WhatsAppPlaceKind::GroupChat,
            stable_conversation_id: "whatsapp-conversation-1089c".to_owned(),
        })
    };
    let group = inspect_whatsapp_place(&surface, &mut group_reader, &group_allowances)
        .expect("positive control must inspect the allowed group");
    assert!(group.group_allowed);

    let inspected = inspect_whatsapp_place(&surface, &mut reader, &group_allowances)
        .expect("the WhatsApp place reader must directly return a broadcast list");

    assert_eq!(reader_calls, 1);
    assert_eq!(inspected.kind, WhatsAppPlaceKind::BroadcastList);
    assert!(!inspected.group_allowed);
    assert_eq!(group_allowances.len(), allowance_count_before);
    println!(
        "TASK1089C whatsapp_place_reader_calls={reader_calls} kind={} group_allowance_matched={} group_allowance_inherited={} allowlist_entries_before={} allowlist_entries_after={}",
        inspected.kind.as_str(),
        group.group_allowed,
        inspected.group_allowed,
        allowance_count_before,
        group_allowances.len(),
    );
}
