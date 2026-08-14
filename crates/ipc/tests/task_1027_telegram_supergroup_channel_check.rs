use ipc::commands::{
    cmd_osl_read_telegram_auto_whitelist_rule, cmd_osl_save_telegram_auto_whitelist_rule,
    TelegramAutoWhitelistRuleDto,
};
use ipc::main_password::set_file_storage_key;
use ipc::state::AppState;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

static KEY_LOCK: Mutex<()> = Mutex::new(());

const ACCOUNT: &str = "telegram-account-1027";
const GATE: &str = "1021";
const NOOP_READER_ENV: &str = "TASK1027_STUB_TELEGRAM_READER_NOOP";

#[derive(Clone, Copy)]
struct TelegramPlace<'a> {
    kind: &'a str,
    place_id: &'a str,
}

trait TelegramPlaceReader {
    fn inspect_places(&self) -> Vec<TelegramPlace<'_>>;
}

struct AllowedTelegramPlaces {
    inspections: AtomicUsize,
}

impl TelegramPlaceReader for AllowedTelegramPlaces {
    fn inspect_places(&self) -> Vec<TelegramPlace<'_>> {
        self.inspections.fetch_add(1, Ordering::SeqCst);
        vec![
            TelegramPlace {
                kind: "supergroup",
                place_id: "tg-supergroup-1027",
            },
            TelegramPlace {
                kind: "channel",
                place_id: "tg-channel-1027",
            },
        ]
    }
}

struct NoopTelegramPlaceReader;

impl TelegramPlaceReader for NoopTelegramPlaceReader {
    fn inspect_places(&self) -> Vec<TelegramPlace<'_>> {
        Vec::new()
    }
}

fn inspect_allowed_telegram_places(
    reader: &impl TelegramPlaceReader,
    state: &AppState,
) -> Result<Vec<TelegramAutoWhitelistRuleDto>, String> {
    let places = reader.inspect_places();
    ["supergroup", "channel"]
        .into_iter()
        .map(|expected_kind| {
            let place = places
                .iter()
                .find(|place| place.kind == expected_kind)
                .ok_or_else(|| format!("TASK1027 did not inspect telegram:{expected_kind}"))?;
            cmd_osl_read_telegram_auto_whitelist_rule(
                state,
                ACCOUNT.to_owned(),
                place.kind.to_owned(),
                place.place_id.to_owned(),
            )
        })
        .collect()
}

#[test]
fn task_1027_check_telegram_supergroup_and_channel() {
    let _guard = KEY_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    set_file_storage_key(Some([0x27; 32]));

    let result = (|| -> Result<(), String> {
        let config_dir = tempfile::tempdir().map_err(|error| error.to_string())?;
        let state = AppState::new();
        for (kind, place_id) in [
            ("supergroup", "tg-supergroup-1027"),
            ("channel", "tg-channel-1027"),
        ] {
            cmd_osl_save_telegram_auto_whitelist_rule(
                &state,
                ACCOUNT.to_owned(),
                kind.to_owned(),
                place_id.to_owned(),
                "always".to_owned(),
                Some(config_dir.path().to_owned()),
            )?;
        }

        let reader = AllowedTelegramPlaces {
            inspections: AtomicUsize::new(0),
        };
        let inspected = if std::env::var_os(NOOP_READER_ENV).is_some() {
            println!("TASK1027_READER_MODE=noop");
            inspect_allowed_telegram_places(&NoopTelegramPlaceReader, &state)?
        } else {
            println!("TASK1027_READER_MODE=allowed_places");
            inspect_allowed_telegram_places(&reader, &state)?
        };

        assert_eq!(reader.inspections.load(Ordering::SeqCst), 1);
        assert_eq!(
            inspected
                .iter()
                .map(|rule| (rule.allowed_place.kind.as_str(), rule.choice.as_str()))
                .collect::<Vec<_>>(),
            vec![("supergroup", "always"), ("channel", "always")]
        );
        assert!(inspected.iter().all(|rule| {
            rule.allowed_place.app == "telegram"
                && rule.allowed_place.account == ACCOUNT
                && rule.rule_lookup == format!("telegram:{}", rule.allowed_place.kind)
        }));

        let noop_error = inspect_allowed_telegram_places(&NoopTelegramPlaceReader, &state)
            .expect_err("a Telegram reader that does nothing must fail the check");
        assert_eq!(noop_error, "TASK1027 did not inspect telegram:supergroup");

        println!("TASK1027_GATE={GATE}");
        println!(
            "TASK1027_TELEGRAM_SUPERGROUP_KIND={}",
            inspected[0].allowed_place.kind
        );
        println!("TASK1027_TELEGRAM_SUPERGROUP_ALLOW={}", inspected[0].choice);
        println!(
            "TASK1027_TELEGRAM_CHANNEL_KIND={}",
            inspected[1].allowed_place.kind
        );
        println!("TASK1027_TELEGRAM_CHANNEL_ALLOW={}", inspected[1].choice);
        println!("TASK1027_NOOP_READER=failed_as_expected");
        Ok(())
    })();

    set_file_storage_key(None);
    result.expect("TASK1027 Telegram supergroup and channel check");
}
