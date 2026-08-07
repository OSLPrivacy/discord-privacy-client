use ipc::mutual_discovery::{
    fixed_discovery_deck, mutual_discovery_label, one_sided_discovery_label,
    publish_discovery_card, scan_mutual_discovery_cards, MUTUAL_DISCOVERY_FIXED_CARD_COUNT,
};

const ALICE: &str = "alice@osl.test";
const BOB: &str = "bob@osl.test";

#[test]
fn task_4754_mutual_discovery_label_requires_both_published_sides() {
    let label_ab = mutual_discovery_label(ALICE, BOB).unwrap();
    let label_ba = mutual_discovery_label(BOB, ALICE).unwrap();
    let label_other_order = mutual_discovery_label(BOB, ALICE).unwrap();
    let one_sided = one_sided_discovery_label(ALICE).unwrap();

    let both_published = fixed_discovery_deck(vec![
        publish_discovery_card(ALICE, BOB).unwrap(),
        publish_discovery_card(BOB, ALICE).unwrap(),
    ])
    .unwrap();
    let alice_both = scan_mutual_discovery_cards(ALICE, BOB, &both_published).unwrap();
    let bob_both = scan_mutual_discovery_cards(BOB, ALICE, &both_published).unwrap();

    let only_alice_published =
        fixed_discovery_deck(vec![publish_discovery_card(ALICE, BOB).unwrap()]).unwrap();
    let alice_one_sided = scan_mutual_discovery_cards(ALICE, BOB, &only_alice_published).unwrap();
    let bob_one_sided = scan_mutual_discovery_cards(BOB, ALICE, &only_alice_published).unwrap();

    println!("TASK4754 label_a_writes_about_b={label_ab}");
    println!("TASK4754 label_b_writes_about_a={label_ba}");
    println!("TASK4754 label_other_order={label_other_order}");
    println!(
        "TASK4754 both_published.machine_a.received_cards={}",
        alice_both.received_card_count
    );
    println!(
        "TASK4754 both_published.machine_b.received_cards={}",
        bob_both.received_card_count
    );
    println!(
        "TASK4754 both_published.machine_a.status={} matching_cards={}",
        alice_both.status(),
        alice_both.matching_cards.len()
    );
    println!(
        "TASK4754 both_published.machine_b.status={} matching_cards={}",
        bob_both.status(),
        bob_both.matching_cards.len()
    );
    println!(
        "TASK4754 only_a_published.machine_a.received_cards={}",
        alice_one_sided.received_card_count
    );
    println!(
        "TASK4754 only_a_published.machine_b.received_cards={}",
        bob_one_sided.received_card_count
    );
    println!(
        "TASK4754 only_a_published.machine_a.status={} matching_cards={}",
        alice_one_sided.status(),
        alice_one_sided.matching_cards.len()
    );
    println!(
        "TASK4754 only_a_published.machine_b.status={} matching_cards={}",
        bob_one_sided.status(),
        bob_one_sided.matching_cards.len()
    );
    println!("TASK4754 one_handle_label={one_sided}");

    assert_eq!(
        label_ab,
        "osl-mutual-discovery-v1:alice@osl.test|bob@osl.test"
    );
    assert_eq!(label_ab, label_ba);
    assert_eq!(label_ab, label_other_order);
    assert!(label_ab.contains(ALICE));
    assert!(label_ab.contains(BOB));
    assert_ne!(one_sided, label_ab);
    assert_eq!(
        alice_both.received_card_count,
        MUTUAL_DISCOVERY_FIXED_CARD_COUNT
    );
    assert_eq!(
        bob_both.received_card_count,
        MUTUAL_DISCOVERY_FIXED_CARD_COUNT
    );
    assert_eq!(alice_both.status(), "matched");
    assert_eq!(bob_both.status(), "matched");
    assert_eq!(alice_both.matching_cards.len(), 1);
    assert_eq!(bob_both.matching_cards.len(), 1);
    assert_eq!(
        alice_one_sided.received_card_count,
        MUTUAL_DISCOVERY_FIXED_CARD_COUNT
    );
    assert_eq!(
        bob_one_sided.received_card_count,
        MUTUAL_DISCOVERY_FIXED_CARD_COUNT
    );
    assert_eq!(alice_one_sided.status(), "no match");
    assert_eq!(bob_one_sided.status(), "no match");
    assert_eq!(alice_one_sided.matching_cards.len(), 0);
    assert_eq!(bob_one_sided.matching_cards.len(), 0);
}
