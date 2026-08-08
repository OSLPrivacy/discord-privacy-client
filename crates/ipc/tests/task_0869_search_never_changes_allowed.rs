//! TASK 0869 - check search never changes what is allowed.
//!
//! Records the allowed state of all 40 allowed places, runs ten different
//! searches (one of them empty), then records it again. The two recordings
//! must be identical for all 40 places, with nothing added and nothing
//! removed.
//!
//! The recording is not a bare row dump: for every place it also asks the
//! store's own `allowed_place_is_allowed` whether that place is still
//! allowed, so a search that silently un-allowed a place would show up even
//! if the row survived.

use ipc::allowed_places::{
    add_allowed_place_record, allowed_place_is_allowed, allowed_places_db_path, AllowedPlaceQuery,
    AllowedPlaceRecord,
};
use ipc::commands::cmd_osl_search_allowed_places;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::path::Path;

/// The same 40-place fixture TASK 0867 pinned its search counts against.
const PLACES: [(&str, &str); 40] = [
    ("Elm Studio 01", "Mara Vale"),
    ("North Elm Annex", "Theo Grant"),
    ("Elmstead Project Room", "Priya Nolen"),
    ("Signal Pier 08", "Selma Price"),
    ("Helix Bench 30", "Helmi Torres"),
    ("Archive Hall 24", "Anselm Ward"),
    ("Forge Atlas Room 17", "Avery Stone"),
    ("Copper Quay", "Nico Reed"),
    ("Lumen Booth", "Iris Lane"),
    ("Harbor Desk", "Owen Pike"),
    ("Summit Pod", "Jules Hart"),
    ("Cinder Lab", "Rhea Moss"),
    ("Orbit Table", "Noah Fenn"),
    ("Cobalt Room", "Leah Quinn"),
    ("Prairie Nook", "Milo Kent"),
    ("River Bay", "Tara Holt"),
    ("Anchor Loft", "Ezra Vale"),
    ("Garden Seat", "Mina Ford"),
    ("Canvas Room", "Otis Blair"),
    ("Pioneer Bay", "Lina Cross"),
    ("Quartz Booth", "Hugo Ames"),
    ("Beacon Room", "Rosa Finch"),
    ("Willow Desk", "Cole Vance"),
    ("Summit East", "Dana Frost"),
    ("Canyon West", "Remy Shore"),
    ("Lattice Bar", "Sage Rowe"),
    ("Atrium Six", "Pax Nolan"),
    ("Foundry One", "Nell Ash"),
    ("Vector Nine", "Tess Brook"),
    ("Station Four", "Glen Ray"),
    ("Nova Room", "Beth Crowe"),
    ("Slate Table", "Ivan Locke"),
    ("Lagoon Seat", "June Miles"),
    ("Beacon North", "Kira Wells"),
    ("Crescent East", "Poe Harris"),
    ("Monarch Deck", "Ruth Clay"),
    ("Vista Room", "Alan Grove"),
    ("Prairie West", "Cleo Drake"),
    ("Orchid Bay", "Finn Marsh"),
    ("Keystone Room", "Bea Lyons"),
];

/// Ten different searches. `SEARCHES[0]` is the empty one the task asks for.
const SEARCHES: [&str; 10] = [
    "",
    "   ",
    "elm",
    "Forge Atlas Room 17",
    "zzqxv",
    "room",
    "BEACON",
    "Mara Vale",
    "'; DELETE FROM allowed_places; --",
    "%",
];

/// One place's allowed state, as recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
struct AllowedPlaceState {
    stable_id: String,
    app: String,
    account: String,
    kind: String,
    place_name: String,
    person_name: String,
    allowed: bool,
}

impl AllowedPlaceState {
    /// One stable line per place; the whole recording hashes these.
    fn line(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|allowed={}",
            self.stable_id,
            self.app,
            self.account,
            self.kind,
            self.place_name,
            self.person_name,
            self.allowed
        )
    }
}

/// Read every allowed place out of the store and ask, per place, whether it
/// is still allowed. Ordered by stable_id so two recordings line up.
fn record_allowed_state(dir: &Path) -> Vec<AllowedPlaceState> {
    let conn = Connection::open(allowed_places_db_path(dir)).expect("open allowed places db");
    let mut statement = conn
        .prepare(
            "SELECT app, account, kind, stable_id, place_name, person_name
             FROM allowed_places
             ORDER BY stable_id ASC",
        )
        .expect("prepare recording query");
    let rows: Vec<(String, String, String, String, String, String)> = statement
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        })
        .expect("run recording query")
        .collect::<std::result::Result<Vec<_>, _>>()
        .expect("read recording rows");

    rows.into_iter()
        .map(
            |(app, account, kind, stable_id, place_name, person_name)| {
                let allowed = allowed_place_is_allowed(
                    dir,
                    &AllowedPlaceQuery {
                        app: app.clone(),
                        account: account.clone(),
                        kind: kind.clone(),
                        stable_id: stable_id.clone(),
                    },
                )
                .expect("ask store whether place is allowed");
                AllowedPlaceState {
                    stable_id,
                    app,
                    account,
                    kind,
                    place_name,
                    person_name,
                    allowed,
                }
            },
        )
        .collect()
}

fn digest(states: &[AllowedPlaceState]) -> String {
    let mut hasher = Sha256::new();
    for state in states {
        hasher.update(state.line().as_bytes());
        hasher.update(b"\n");
    }
    format!("{:x}", hasher.finalize())
}

/// What comparing two recordings found.
#[derive(Debug, Default)]
struct RecordingDiff {
    added: Vec<String>,
    removed: Vec<String>,
    changed: Vec<String>,
}

impl RecordingDiff {
    fn identical(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }

    fn summary(&self) -> String {
        format!(
            "added={} removed={} changed={} added_ids='{}' removed_ids='{}' changed_ids='{}'",
            self.added.len(),
            self.removed.len(),
            self.changed.len(),
            self.added.join(","),
            self.removed.join(","),
            self.changed.join(",")
        )
    }
}

/// The check itself. Given two recordings, say exactly what moved.
fn compare_recordings(before: &[AllowedPlaceState], after: &[AllowedPlaceState]) -> RecordingDiff {
    let mut diff = RecordingDiff::default();
    for old in before {
        match after.iter().find(|new| new.stable_id == old.stable_id) {
            None => diff.removed.push(old.stable_id.clone()),
            Some(new) if new != old => diff.changed.push(old.stable_id.clone()),
            Some(_) => {}
        }
    }
    for new in after {
        if !before.iter().any(|old| old.stable_id == new.stable_id) {
            diff.added.push(new.stable_id.clone());
        }
    }
    diff
}

fn seed_forty_places(dir: &Path) {
    for (index, (place_name, person_name)) in PLACES.iter().enumerate() {
        add_allowed_place_record(
            dir,
            &AllowedPlaceRecord {
                app: "discord".to_string(),
                account: "900000000000086700".to_string(),
                kind: "direct_message".to_string(),
                stable_id: format!(
                    "discord:900000000000086700:direct_message:{:02}",
                    index + 1
                ),
                place_name: place_name.to_string(),
                person_name: person_name.to_string(),
            },
        )
        .expect("seed allowed place");
    }
}

#[test]
fn ten_searches_leave_all_forty_allowed_places_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    seed_forty_places(dir);

    let before = record_allowed_state(dir);
    let before_digest = digest(&before);
    println!(
        "TASK_0869_RECORD_BEFORE place_count={} allowed_count={} digest={}",
        before.len(),
        before.iter().filter(|state| state.allowed).count(),
        before_digest
    );

    let mut search_report: Vec<String> = Vec::new();
    for (index, query) in SEARCHES.iter().enumerate() {
        let results = cmd_osl_search_allowed_places(dir.to_path_buf(), (*query).to_string())
            .expect("run allowed-place search");
        search_report.push(format!("{}:'{}'={}", index + 1, query, results.len()));
    }
    println!(
        "TASK_0869_SEARCHES count={} distinct={} empty_query_included={} results={}",
        SEARCHES.len(),
        {
            let mut seen: Vec<&str> = SEARCHES.to_vec();
            seen.sort_unstable();
            seen.dedup();
            seen.len()
        },
        SEARCHES.iter().any(|query| query.is_empty()),
        search_report.join(" ")
    );

    let after = record_allowed_state(dir);
    let after_digest = digest(&after);
    println!(
        "TASK_0869_RECORD_AFTER place_count={} allowed_count={} digest={}",
        after.len(),
        after.iter().filter(|state| state.allowed).count(),
        after_digest
    );

    let diff = compare_recordings(&before, &after);
    println!(
        "TASK_0869_COMPARE identical={} identical_places={}/{} {}",
        diff.identical(),
        before
            .iter()
            .zip(after.iter())
            .filter(|(old, new)| old == new)
            .count(),
        before.len(),
        diff.summary()
    );

    // Ten searches, all different, one of them empty.
    assert_eq!(SEARCHES.len(), 10, "ten searches");
    let mut distinct: Vec<&str> = SEARCHES.to_vec();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), 10, "ten *different* searches");
    assert!(
        SEARCHES.iter().any(|query| query.is_empty()),
        "one of the ten searches is empty"
    );

    // The recordings.
    assert_eq!(before.len(), 40, "40 places recorded before");
    assert_eq!(after.len(), 40, "40 places recorded after");
    assert!(
        before.iter().all(|state| state.allowed),
        "all 40 places allowed before"
    );
    assert!(
        after.iter().all(|state| state.allowed),
        "all 40 places allowed after"
    );
    assert_eq!(before, after, "the two recordings are identical");
    assert_eq!(before_digest, after_digest, "recording digests match");
    assert!(
        diff.identical(),
        "search changed the allowed places: {}",
        diff.summary()
    );
    assert!(diff.added.is_empty(), "no allowed place was added");
    assert!(diff.removed.is_empty(), "no allowed place was removed");

    // Pin the counts TASK 0867 established, so this is a real search run and
    // not ten no-ops. The empty query returns nothing by design.
    let count_for = |query: &str| {
        cmd_osl_search_allowed_places(dir.to_path_buf(), query.to_string())
            .expect("search")
            .len()
    };
    assert_eq!(count_for(""), 0, "empty search returns 0");
    assert_eq!(count_for("elm"), 6, "'elm' returns 6");
    assert_eq!(count_for("Forge Atlas Room 17"), 1, "exact name returns 1");
    assert_eq!(count_for("zzqxv"), 0, "nonsense returns 0");
    assert_eq!(
        count_for("'; DELETE FROM allowed_places; --"),
        0,
        "an injection-shaped query is just a query that matches nothing"
    );
}

#[test]
fn changing_one_saved_value_in_a_throwaway_copy_makes_the_check_fail() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    seed_forty_places(dir);

    let before = record_allowed_state(dir);
    assert_eq!(before.len(), 40);

    // (a) Throwaway copy of the recording with exactly 1 saved value changed.
    let mut edited_place_name = before.clone();
    let target = 7;
    let original_place_name = edited_place_name[target].place_name.clone();
    edited_place_name[target].place_name = format!("{original_place_name} (edited)");
    let diff_place_name = compare_recordings(&before, &edited_place_name);
    println!(
        "TASK_0869_BREAK_VALUE field=place_name stable_id={} was='{}' now='{}' identical={} digest_before={} digest_after={} {}",
        before[target].stable_id,
        original_place_name,
        edited_place_name[target].place_name,
        diff_place_name.identical(),
        digest(&before),
        digest(&edited_place_name),
        diff_place_name.summary()
    );
    assert!(
        !diff_place_name.identical(),
        "changing 1 saved place_name must make the check fail"
    );
    assert_eq!(diff_place_name.changed.len(), 1);
    assert_ne!(digest(&before), digest(&edited_place_name));

    // (b) Same again on the allowed flag itself.
    let mut edited_allowed = before.clone();
    edited_allowed[target].allowed = false;
    let diff_allowed = compare_recordings(&before, &edited_allowed);
    println!(
        "TASK_0869_BREAK_VALUE field=allowed stable_id={} was=true now=false identical={} {}",
        before[target].stable_id,
        diff_allowed.identical(),
        diff_allowed.summary()
    );
    assert!(
        !diff_allowed.identical(),
        "changing 1 saved allowed value must make the check fail"
    );
    assert_eq!(diff_allowed.changed.len(), 1);

    // (c) The strongest form: a throwaway *copy of the store* with one place
    //     really removed. Proves the recorder sees a real removal, not just
    //     that two structs differ.
    let throwaway = tempfile::tempdir().unwrap();
    std::fs::copy(
        allowed_places_db_path(dir),
        allowed_places_db_path(throwaway.path()),
    )
    .expect("copy allowed places store");
    let removed_id = before[target].stable_id.clone();
    let removed = ipc::allowed_places::remove_allowed_place_record(throwaway.path(), &removed_id)
        .expect("remove one place from the throwaway copy");
    assert!(removed);
    let throwaway_recording = record_allowed_state(throwaway.path());
    let diff_removed = compare_recordings(&before, &throwaway_recording);
    println!(
        "TASK_0869_BREAK_STORE removed_stable_id={} throwaway_place_count={} identical={} {}",
        removed_id,
        throwaway_recording.len(),
        diff_removed.identical(),
        diff_removed.summary()
    );
    assert_eq!(throwaway_recording.len(), 39);
    assert!(
        !diff_removed.identical(),
        "a removed allowed place must make the check fail"
    );
    assert_eq!(diff_removed.removed, vec![removed_id]);

    // The real store is untouched by any of it.
    let after = record_allowed_state(dir);
    println!(
        "TASK_0869_BREAK_ORIGINAL_INTACT place_count={} identical_to_before={}",
        after.len(),
        compare_recordings(&before, &after).identical()
    );
    assert_eq!(after.len(), 40);
    assert!(compare_recordings(&before, &after).identical());
}
