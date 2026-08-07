#![cfg(feature = "core")]

use osl_privacy_hub::scrub_hosted::place_scope::{
    place_hosted_item, prepare_hosted_place_draft, read_hosted_place, HostedAllowedPlaceRecord,
    HostedPlaceActionError, HostedPlaceActionLog, HostedPlaceActionRequest, HostedPlaceList,
};
use std::fmt;

type AppActionRunner = fn(
    &HostedPlaceList,
    &mut HostedPlaceActionLog,
    &HostedAllowedPlaceRecord,
) -> Result<AllowedActionNames, NamedAllowedAppError>;

struct AppFixture {
    app: &'static str,
    place_kind: &'static str,
    allowed_suffix: &'static str,
    place_name: &'static str,
    runner: AppActionRunner,
}

struct AllowedActionNames {
    read_item: String,
    draft_item: String,
    placed_item: String,
}

#[derive(Debug)]
struct NamedAllowedAppError {
    app: String,
    source: HostedPlaceActionError,
}

impl fmt::Display for NamedAllowedAppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "allowed-place app {} refused: {}", self.app, self.source)
    }
}

const APPS: [AppFixture; 8] = [
    AppFixture {
        app: "discord",
        place_kind: "direct_message",
        allowed_suffix: "discord-peer-0168",
        place_name: "Discord task 0168 DM",
        runner: run_hosted_place_actions,
    },
    AppFixture {
        app: "telegram",
        place_kind: "direct_message",
        allowed_suffix: "telegram-peer-0168",
        place_name: "Telegram task 0168 DM",
        runner: run_hosted_place_actions,
    },
    AppFixture {
        app: "signal",
        place_kind: "direct_message",
        allowed_suffix: "signal-peer-0168",
        place_name: "Signal task 0168 DM",
        runner: run_hosted_place_actions,
    },
    AppFixture {
        app: "whatsapp",
        place_kind: "direct_message",
        allowed_suffix: "whatsapp-peer-0168",
        place_name: "WhatsApp task 0168 DM",
        runner: run_hosted_place_actions,
    },
    AppFixture {
        app: "x",
        place_kind: "direct_message",
        allowed_suffix: "x-peer-0168",
        place_name: "X task 0168 DM",
        runner: run_hosted_place_actions,
    },
    AppFixture {
        app: "instagram",
        place_kind: "direct_message",
        allowed_suffix: "instagram-peer-0168",
        place_name: "Instagram task 0168 DM",
        runner: run_hosted_place_actions,
    },
    AppFixture {
        app: "messenger",
        place_kind: "direct_message",
        allowed_suffix: "messenger-peer-0168",
        place_name: "Messenger task 0168 DM",
        runner: run_hosted_place_actions,
    },
    AppFixture {
        app: "email",
        place_kind: "email_address",
        allowed_suffix: "friend0168@example.test",
        place_name: "Email task 0168 address",
        runner: run_hosted_place_actions,
    },
];

fn stable_place_id(app: &str, place_kind: &str, suffix: &str) -> String {
    format!("{app}:account-0168:{place_kind}:{suffix}")
}

fn request(app: &str, stable_place_id: &str, action: &str) -> HostedPlaceActionRequest {
    HostedPlaceActionRequest::new(app, stable_place_id, format!("{app}-{action}-item-0168"))
}

fn allowed_app_error(
    record: &HostedAllowedPlaceRecord,
    source: HostedPlaceActionError,
) -> NamedAllowedAppError {
    NamedAllowedAppError {
        app: record.app.clone(),
        source,
    }
}

fn run_hosted_place_actions(
    places: &HostedPlaceList,
    log: &mut HostedPlaceActionLog,
    record: &HostedAllowedPlaceRecord,
) -> Result<AllowedActionNames, NamedAllowedAppError> {
    let read_request = request(&record.app, &record.stable_place_id, "read");
    let draft_request = request(&record.app, &record.stable_place_id, "draft");
    let place_request = request(&record.app, &record.stable_place_id, "placed");

    let read = read_hosted_place(places, log, &read_request)
        .map_err(|error| allowed_app_error(record, error))?;
    let draft = prepare_hosted_place_draft(places, log, &draft_request)
        .map_err(|error| allowed_app_error(record, error))?;
    let placed = place_hosted_item(places, log, &place_request)
        .map_err(|error| allowed_app_error(record, error))?;

    Ok(AllowedActionNames {
        read_item: read.item_id,
        draft_item: draft.item_id,
        placed_item: placed.item_id,
    })
}

#[allow(dead_code)]
fn always_refuse_app_actions(
    _places: &HostedPlaceList,
    _log: &mut HostedPlaceActionLog,
    record: &HostedAllowedPlaceRecord,
) -> Result<AllowedActionNames, NamedAllowedAppError> {
    Err(allowed_app_error(
        record,
        HostedPlaceActionError::PlaceNotAllowed,
    ))
}

#[test]
fn task_0168_allowed_list_is_silent_for_all_apps_outside_marked_places() {
    let allowed_records: Vec<_> = APPS
        .iter()
        .map(|fixture| {
            HostedAllowedPlaceRecord::new(
                fixture.app,
                stable_place_id(fixture.app, fixture.place_kind, fixture.allowed_suffix),
                fixture.place_name,
                168,
            )
        })
        .collect();
    let places = HostedPlaceList::new(allowed_records.clone());
    let mut log = HostedPlaceActionLog::default();
    let before_count = log.action_count();
    let fingerprints_before = places.record_fingerprints();

    let readable_count = allowed_records
        .iter()
        .filter(|record| places.is_readable(&record.app, &record.stable_place_id))
        .count();
    println!("TASK0168_MARKED_ALLOWED_READABLE_COUNT={readable_count}");
    println!("TASK0168_ACTION_COUNT_BEFORE={before_count}");
    println!(
        "TASK0168_ALLOWED_RECORD_FINGERPRINT_COUNT_BEFORE={}",
        fingerprints_before.len()
    );

    assert_eq!(places.readable_count(), 8);
    assert_eq!(readable_count, 8);
    assert_eq!(before_count, 0);
    assert_eq!(fingerprints_before.len(), 8);

    let mut allowed_action_names = Vec::new();
    for (fixture, record) in APPS.iter().zip(allowed_records.iter()) {
        let actions =
            (fixture.runner)(&places, &mut log, record).unwrap_or_else(|error| panic!("{error}"));
    for record in &allowed_records {
        let read_request = request(&record.app, &record.stable_place_id, "read");
        let draft_request = request(&record.app, &record.stable_place_id, "draft");
        let place_request = request(&record.app, &record.stable_place_id, "placed");

        let read = read_hosted_place(&places, &mut log, &read_request).expect("read allowed");
        let draft =
            prepare_hosted_place_draft(&places, &mut log, &draft_request).expect("draft allowed");
        let placed = place_hosted_item(&places, &mut log, &place_request).expect("place allowed");

        println!(
            "TASK0168_ALLOWED_APP app={} read_item={} draft_item={} placed_item={} cumulative_action_count={}",
            record.app,
            actions.read_item,
            actions.draft_item,
            actions.placed_item,
            read.item_id,
            draft.item_id,
            placed.item_id,
            log.action_count()
        );
        allowed_action_names.push(format!(
            "{}:{},{},{}",
            record.app, actions.read_item, actions.draft_item, actions.placed_item
            record.app, read.item_id, draft.item_id, placed.item_id
        ));
    }

    let after_allowed_count = log.action_count();
    println!(
        "TASK0168_ALLOWED_APP_ACTION_NAME_COUNT={}",
        allowed_action_names.len()
    );
    println!("TASK0168_ACTION_COUNT_AFTER_ALLOWED={after_allowed_count}");
    assert_eq!(allowed_action_names.len(), 8);
    assert_eq!(after_allowed_count, 24);

    let fingerprints_after_allowed = places.record_fingerprints();
    let mut unlisted_refusals = 0usize;
    for record in &allowed_records {
        let unlisted_place_id = format!("{}-UNLISTED", record.stable_place_id);
        for (call, allowed_request) in [
            (
                "read",
                request(&record.app, &record.stable_place_id, "read"),
            ),
            (
                "draft",
                request(&record.app, &record.stable_place_id, "draft"),
            ),
            (
                "place",
                request(&record.app, &record.stable_place_id, "placed"),
            ),
        ] {
            let unlisted_request = allowed_request.with_stable_place_id(&unlisted_place_id);
            assert_eq!(allowed_request.app, unlisted_request.app);
            assert_eq!(allowed_request.item_id, unlisted_request.item_id);
            assert_ne!(
                allowed_request.stable_place_id,
                unlisted_request.stable_place_id
            );

            let result = match call {
                "read" => read_hosted_place(&places, &mut log, &unlisted_request).map(|_| ()),
                "draft" => {
                    prepare_hosted_place_draft(&places, &mut log, &unlisted_request).map(|_| ())
                }
                "place" => place_hosted_item(&places, &mut log, &unlisted_request).map(|_| ()),
                _ => unreachable!(),
            };
            assert_eq!(result, Err(HostedPlaceActionError::PlaceNotAllowed));
            println!(
                "TASK0168_UNLISTED_CALL app={} call={call} stable_place_id={} refused={}",
                record.app,
                unlisted_request.stable_place_id,
                result.as_ref().unwrap_err()
            );
            unlisted_refusals += 1;
        }
    }

    let after_unlisted_count = log.action_count();
    let fingerprints_after_unlisted = places.record_fingerprints();
    println!("TASK0168_UNLISTED_REFUSAL_COUNT={unlisted_refusals}");
    println!("TASK0168_ACTION_COUNT_AFTER_UNLISTED={after_unlisted_count}");
    println!(
        "TASK0168_ALLOWED_RECORD_FINGERPRINTS_UNCHANGED={}",
        fingerprints_before == fingerprints_after_allowed
            && fingerprints_before == fingerprints_after_unlisted
    );
    println!(
        "TASK0168_ALLOWED_RECORD_FINGERPRINTS={}",
        fingerprints_after_unlisted.join(",")
    );

    assert_eq!(unlisted_refusals, 24);
    assert_eq!(after_unlisted_count, 24);
    assert_eq!(fingerprints_before, fingerprints_after_allowed);
    assert_eq!(fingerprints_before, fingerprints_after_unlisted);
    assert_eq!(log.records().len(), 24);
}
