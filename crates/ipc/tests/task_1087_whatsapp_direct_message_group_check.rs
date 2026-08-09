use ipc::commands::{
    cmd_osl_read_whatsapp_auto_whitelist_rule, cmd_osl_save_whatsapp_auto_whitelist_rule,
    WhatsAppAutoWhitelistRuleDto,
};
use ipc::main_password::set_file_storage_key;
use ipc::state::AppState;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

static KEY_LOCK: Mutex<()> = Mutex::new(());

const ACCOUNT: &str = "whatsapp-account-1087";
const GATE: &str = "1083";
const NOOP_READER_ENV: &str = "TASK1087_STUB_WHATSAPP_READER_NOOP";

#[derive(Clone, Copy)]
struct WhatsAppPlace<'a> {
    kind: &'a str,
    place_id: &'a str,
}

/// This is deliberately a seam the test drives directly: the check must read
/// both WhatsApp places, rather than merely construct the expected responses.
trait WhatsAppPlaceReader {
    fn inspect_places(&self) -> Vec<WhatsAppPlace<'_>>;
}

struct AllowedWhatsAppPlaces {
    inspections: AtomicUsize,
}

impl WhatsAppPlaceReader for AllowedWhatsAppPlaces {
    fn inspect_places(&self) -> Vec<WhatsAppPlace<'_>> {
        self.inspections.fetch_add(1, Ordering::SeqCst);
        vec![
            WhatsAppPlace {
                kind: "direct_message",
                place_id: "wa-direct-message-1087",
            },
            WhatsAppPlace {
                kind: "group_chat",
                place_id: "wa-group-chat-1087",
            },
        ]
    }
}

struct NoopWhatsAppPlaceReader;

impl WhatsAppPlaceReader for NoopWhatsAppPlaceReader {
    fn inspect_places(&self) -> Vec<WhatsAppPlace<'_>> {
        Vec::new()
    }
}

fn inspect_allowed_whatsapp_places(
    reader: &impl WhatsAppPlaceReader,
    state: &AppState,
) -> Result<Vec<WhatsAppAutoWhitelistRuleDto>, String> {
    let places = reader.inspect_places();
    ["direct_message", "group_chat"]
        .into_iter()
        .map(|expected_kind| {
            let place = places
                .iter()
                .find(|place| place.kind == expected_kind)
                .ok_or_else(|| format!("TASK1087 did not inspect whatsapp:{expected_kind}"))?;
            cmd_osl_read_whatsapp_auto_whitelist_rule(
                state,
                place.kind.to_owned(),
                ACCOUNT.to_owned(),
                place.place_id.to_owned(),
            )
        })
        .collect()
}

#[test]
fn task_1087_check_whatsapp_direct_message_and_group() {
    let _guard = KEY_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    set_file_storage_key(Some([0x87; 32]));

    let result = (|| -> Result<(), String> {
        let config_dir = tempfile::tempdir().map_err(|error| error.to_string())?;
        let state = AppState::new();
        for (kind, place_id) in [
            ("direct_message", "wa-direct-message-1087"),
            ("group_chat", "wa-group-chat-1087"),
        ] {
            cmd_osl_save_whatsapp_auto_whitelist_rule(
                &state,
                kind.to_owned(),
                ACCOUNT.to_owned(),
                place_id.to_owned(),
                "always".to_owned(),
                Some(config_dir.path().to_owned()),
            )?;
        }

        let reader = AllowedWhatsAppPlaces {
            inspections: AtomicUsize::new(0),
        };
        let inspected = if std::env::var_os(NOOP_READER_ENV).is_some() {
            println!("TASK1087_READER_MODE=noop");
            inspect_allowed_whatsapp_places(&NoopWhatsAppPlaceReader, &state)?
        } else {
            println!("TASK1087_READER_MODE=allowed_places");
            inspect_allowed_whatsapp_places(&reader, &state)?
        };

        assert_eq!(reader.inspections.load(Ordering::SeqCst), 1);
        assert_eq!(
            inspected
                .iter()
                .map(|rule| (rule.whatsapp_kind.as_str(), rule.choice.as_str()))
                .collect::<Vec<_>>(),
            vec![("direct_message", "always"), ("group_chat", "always")]
        );
        assert!(inspected.iter().all(|rule| {
            rule.allowed_place.app == "whatsapp"
                && rule.allowed_place.account == ACCOUNT
                && rule.auto_rule_app_kind == format!("whatsapp:{}", rule.whatsapp_kind)
        }));

        let noop_error = inspect_allowed_whatsapp_places(&NoopWhatsAppPlaceReader, &state)
            .expect_err("a WhatsApp reader that does nothing must fail the check");
        assert_eq!(noop_error, "TASK1087 did not inspect whatsapp:direct_message");

        println!("TASK1087_GATE={GATE}");
        println!(
            "TASK1087_WHATSAPP_DIRECT_MESSAGE_KIND={}",
            inspected[0].whatsapp_kind
        );
        println!(
            "TASK1087_WHATSAPP_DIRECT_MESSAGE_OSL_CONTROL={}",
            inspected[0].choice
        );
        println!("TASK1087_WHATSAPP_GROUP_KIND={}", inspected[1].whatsapp_kind);
        println!(
            "TASK1087_WHATSAPP_GROUP_OSL_CONTROL={}",
            inspected[1].choice
        );
        println!("TASK1087_NOOP_READER=failed_as_expected");
        Ok(())
    })();

    set_file_storage_key(None);
    result.expect("TASK1087 WhatsApp direct message and group check");
}
