use sensitive_classify::categories::{warning_category_for, DetectorCategory};

#[test]
fn t13_tg4_political_speech_categories_never_reach_the_send_warning() {
    // Ordinary political argument and historical discussion can trigger the
    // Scrub lexical categories.  Those categories are intentionally absent
    // from the consequence warning rather than softened by a threshold.
    for category in [
        DetectorCategory::Profanity,
        DetectorCategory::PotentiallyUnlawfulConduct,
        DetectorCategory::ControlledSubstances,
    ] {
        assert_eq!(warning_category_for(category), None);
    }
}
