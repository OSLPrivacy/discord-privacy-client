#![cfg_attr(task1071_direct, allow(dead_code))]

#[cfg(task1071_direct)]
mod adapters {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct Bounds {
        pub x: i32,
        pub y: i32,
        pub width: i32,
        pub height: i32,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PaintConfidence {
        Exact,
        Approximate,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct PaintTarget {
        pub carrier_sha256: String,
        pub rect: Bounds,
        pub clipped_by: Option<Bounds>,
        pub confidence: PaintConfidence,
    }
}

#[cfg(task1071_direct)]
#[path = "../src/native_a11y.rs"]
mod native_a11y;

#[cfg(task1071_direct)]
mod native_apps {
    pub fn whatsapp_store_package_family_name() -> &'static str {
        "5319275A.WhatsAppDesktop_cv1g1gvanyjgm"
    }
}

#[cfg(task1071_direct)]
#[path = "../src/native_whatsapp_adapter.rs"]
mod native_whatsapp_adapter;

#[cfg(task1071_direct)]
use crate::native_whatsapp_adapter::{
    prepare_whatsapp_cover_for_trigger, WhatsAppCoverInsertion, WhatsAppCoverPreparationError,
    WhatsAppSendTrigger,
};
#[cfg(not(task1071_direct))]
use osl_privacy_hub::native_whatsapp_adapter::{
    prepare_whatsapp_cover_for_trigger, WhatsAppCoverInsertion, WhatsAppCoverPreparationError,
    WhatsAppSendTrigger,
};

#[test]
fn task_1071_only_three_whatsapp_triggers_prepare_covers_and_insertion_is_separate() {
    let triggers = ["Enter", "Enter x2", "Clipboard"];
    assert_eq!(
        WhatsAppSendTrigger::ALL.map(WhatsAppSendTrigger::name),
        triggers,
        "WhatsApp has exactly three reviewed trigger names"
    );

    let insertion_modes = [
        WhatsAppCoverInsertion::InsertOnSend,
        WhatsAppCoverInsertion::TypeNaturally,
    ];
    let mut prepared_count = 0usize;
    for (index, trigger) in triggers.into_iter().enumerate() {
        let insertion = insertion_modes[index % insertion_modes.len()];
        let cover = format!("Ordinary WhatsApp cover {} café", index + 1);
        let prepared = prepare_whatsapp_cover_for_trigger(trigger, insertion, &cover)
            .unwrap_or_else(|error| panic!("{trigger} must prepare a cover: {error}"));

        assert!(prepared.prepared);
        assert_eq!(prepared.trigger.name(), trigger);
        assert_eq!(prepared.cover_text.as_bytes(), cover.as_bytes());
        assert_eq!(prepared.cover_bytes, cover.len());
        assert_eq!(prepared.cover_insertion, insertion);
        prepared_count += usize::from(prepared.prepared);
        println!(
            "TASK1071_PREPARED_{}=true trigger={} cover_bytes={} cover_insertion={}",
            index + 1,
            prepared.trigger.name(),
            prepared.cover_bytes,
            prepared.cover_insertion.name(),
        );
    }

    for refused_name in ["Manual", "Instant", "Match typing"] {
        let refusal = prepare_whatsapp_cover_for_trigger(
            refused_name,
            WhatsAppCoverInsertion::InsertOnSend,
            "must not prepare",
        );
        assert_eq!(
            refusal,
            Err(WhatsAppCoverPreparationError::RefusedTriggerName(
                refused_name.to_owned()
            )),
            "{refused_name} must be refused by name"
        );
        println!("TASK1071_REFUSED_NAME={refused_name} refused=true");
    }

    println!("TASK1071_PREPARED_COVERS={prepared_count}");
    println!(
        "TASK1071_COVER_INSERTION_OPTIONS={},{}",
        WhatsAppCoverInsertion::InsertOnSend.name(),
        WhatsAppCoverInsertion::TypeNaturally.name(),
    );
    assert_eq!(prepared_count, 3);
}
