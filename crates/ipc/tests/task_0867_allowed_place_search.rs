use ipc::allowed_places::{add_allowed_place_record, AllowedPlaceRecord};
use ipc::commands::cmd_osl_search_allowed_places;
use rusqlite::Connection;

fn seed_allowed_place(dir: &std::path::Path, index: usize, place_name: &str, person_name: &str) {
    add_allowed_place_record(
        dir,
        &AllowedPlaceRecord {
            app: "discord".to_string(),
            account: "900000000000086700".to_string(),
            kind: "direct_message".to_string(),
            stable_id: format!("discord:900000000000086700:direct_message:{index:02}"),
            place_name: place_name.to_string(),
            person_name: person_name.to_string(),
        },
    )
    .expect("seed allowed place");
}

#[test]
fn direct_allowed_place_search_matches_place_and_person_names() {
    let tmp = tempfile::tempdir().unwrap();
    let places = [
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

    for (index, (place_name, person_name)) in places.iter().enumerate() {
        seed_allowed_place(tmp.path(), index + 1, place_name, person_name);
    }

    let conn = Connection::open(tmp.path().join("allowed_places.sqlite")).unwrap();
    let allowed_place_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM allowed_places", [], |row| row.get(0))
        .unwrap();

    let exact =
        cmd_osl_search_allowed_places(tmp.path().to_path_buf(), "Forge Atlas Room 17".into())
            .expect("search exact place name");
    let shared = cmd_osl_search_allowed_places(tmp.path().to_path_buf(), "elm".into())
        .expect("search shared three letters");
    let nonsense = cmd_osl_search_allowed_places(tmp.path().to_path_buf(), "zzqxv".into())
        .expect("search nonsense");
    let shared_names: Vec<String> = shared
        .iter()
        .map(|row| format!("{}:{}", row.place_name, row.person_name))
        .collect();
    let expected_shared = vec![
        "Archive Hall 24:Anselm Ward".to_string(),
        "Elm Studio 01:Mara Vale".to_string(),
        "Elmstead Project Room:Priya Nolen".to_string(),
        "Helix Bench 30:Helmi Torres".to_string(),
        "North Elm Annex:Theo Grant".to_string(),
        "Signal Pier 08:Selma Price".to_string(),
    ];

    println!(
        "TASK_0867_ALLOWED_PLACE_SEARCH command=cmd_osl_search_allowed_places allowed_place_count={} exact_query='Forge Atlas Room 17' exact_count={} exact_place='{}' shared_query='elm' shared_count={} shared_results='{}' nonsense_query='zzqxv' nonsense_count={}",
        allowed_place_count,
        exact.len(),
        exact.first().map(|row| row.place_name.as_str()).unwrap_or(""),
        shared.len(),
        shared_names.join(" | "),
        nonsense.len()
    );

    assert_eq!(allowed_place_count, 40);
    assert_eq!(exact.len(), 1);
    assert_eq!(exact[0].place_name, "Forge Atlas Room 17");
    assert_eq!(shared_names, expected_shared);
    assert_eq!(nonsense.len(), 0);
}
