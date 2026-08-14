use osl_english_catalogue::EnglishCatalogue;
use osl_privacy_hub::email_timer_contract::{
    arm_protected_email_timer, installed_x_timer_inventory, EMAIL_TIMER_CONTROL,
    EMAIL_TIMER_DISCLOSURE, EMAIL_TIMER_DISCLOSURE_KEY,
};

#[test]
fn task_3301_protected_email_pointer_dies_but_cover_remains_ordinary_is_refused_and_x_is_unavailable() {
    let catalogue = EnglishCatalogue::packaged("task-3301")
        .expect("the shipping English catalogue must load");
    let disclosure = catalogue
        .resolve(EMAIL_TIMER_DISCLOSURE_KEY, [])
        .expect("the email disclosure must resolve from its one catalogue key");
    assert_eq!(disclosure.value, EMAIL_TIMER_DISCLOSURE);
    assert!(!disclosure.value.contains("delete"));
    assert!(!disclosure.value.contains("expire"));
    assert!(!disclosure.value.contains("disappear"));

    let mut protected = arm_protected_email_timer(
        "outlook-cover-task3301-real-target",
        "osl-stored-object-task3301",
        1_700_000_100,
    )
    .expect("a protected email target can arm a pointer-only timer");
    assert!(protected.readable(1_700_000_099));
    assert!(protected.protected_object_exists());
    let cover_id = protected.cover_id.clone();
    assert!(protected.destroy_due_object(1_700_000_100));
    assert!(!protected.readable(1_700_000_100));
    assert!(!protected.protected_object_exists());
    assert_eq!(protected.cover_id, cover_id, "the cover must remain");

    let ordinary = arm_protected_email_timer("ordinary-email-cover", "", 1_700_000_100)
        .expect_err("ordinary email must never create a timer record");
    assert!(ordinary.contains("ordinary email is refused"));
    assert!(ordinary.contains("records=0"));

    let x = installed_x_timer_inventory();
    assert_eq!(x.timer_controls, 0);
    assert_eq!(x.records, 0);
    assert_eq!(x.live_effects, 0);
    assert_eq!(x.shipping_claims, 0);
    assert!(!x.available);

    println!("TASK3301_EMAIL_CONTROL={EMAIL_TIMER_CONTROL}");
    println!("TASK3301_EMAIL_DISCLOSURE_KEY={EMAIL_TIMER_DISCLOSURE_KEY}");
    println!("TASK3301_EMAIL_DISCLOSURE={}", disclosure.value);
    println!("TASK3301_PROTECTED before_due_readable=true after_due_readable=false stored_object_destroyed=true cover_remains=true");
    println!("TASK3301_ORDINARY refused=true records=0");
    println!("TASK3301_X available=false timer_controls={} records={} live_effects={} shipping_claims={}", x.timer_controls, x.records, x.live_effects, x.shipping_claims);
}
