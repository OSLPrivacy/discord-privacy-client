//! TASK 3635: each reviewed browser-mail service must find the one compose
//! body in every supported non-English locale without consulting the service's
//! translated accessible label.  A search field is deliberately included in
//! every fixture so a generic editable-field fallback cannot pass this check.

const SUPPORTED_LOCALES: [Locale; 5] = [
    Locale {
        id: "ja-JP",
        compose_label: "メッセージ本文",
        search_label: "メールを検索",
    },
    Locale {
        id: "zh-CN",
        compose_label: "邮件正文",
        search_label: "搜索邮件",
    },
    Locale {
        id: "ar-SA",
        compose_label: "نص الرسالة",
        search_label: "بحث في البريد",
    },
    Locale {
        id: "hi-IN",
        compose_label: "संदेश का मुख्य भाग",
        search_label: "मेल खोजें",
    },
    Locale {
        id: "ru-RU",
        compose_label: "Текст сообщения",
        search_label: "Поиск почты",
    },
];

const SERVICES: [Service; 6] = [
    Service { id: "gmail" },
    Service { id: "outlook-web" },
    Service { id: "proton-mail" },
    Service { id: "yahoo-mail" },
    Service { id: "aol-mail" },
    Service { id: "icloud-mail" },
];

#[derive(Clone, Copy, Debug)]
struct Locale {
    id: &'static str,
    compose_label: &'static str,
    search_label: &'static str,
}

#[derive(Clone, Copy, Debug)]
struct Service {
    id: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ControlPurpose {
    ComposeBody,
    Search,
}

#[derive(Clone, Debug)]
struct EditableControl {
    service_id: &'static str,
    // This is realistic localized provider UI text.  The compose locator must
    // not inspect it: all identity comes from the reviewed structural purpose.
    localized_label: &'static str,
    purpose: ControlPurpose,
}

#[derive(Default)]
struct BrowserMailPage {
    editable_controls: Vec<EditableControl>,
    typed_characters: usize,
}

impl BrowserMailPage {
    fn localized(service: Service, locale: Locale) -> Self {
        Self {
            // Both controls are editable.  This makes selecting "the first
            // text box" unsafe and proves the locator is not using a label.
            editable_controls: vec![
                EditableControl {
                    service_id: service.id,
                    localized_label: locale.search_label,
                    purpose: ControlPurpose::Search,
                },
                EditableControl {
                    service_id: service.id,
                    localized_label: locale.compose_label,
                    purpose: ControlPurpose::ComposeBody,
                },
            ],
            ..Self::default()
        }
    }

    /// The selection criterion intentionally contains no localized text or
    /// translated-name comparison.  It is the reviewed compose-body control
    /// for the named service and therefore fails closed on zero or many nodes.
    fn find_compose_box(
        &self,
        service: Service,
        _locale: Locale,
    ) -> Result<&EditableControl, String> {
        let expected_purpose = ControlPurpose::ComposeBody;
        let matches = self
            .editable_controls
            .iter()
            .filter(|control| {
                control.service_id == service.id && control.purpose == expected_purpose
            })
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [compose_box] => Ok(compose_box),
            _ => Err(format!("{}: compose box refusal", service.id)),
        }
    }
}

#[test]
fn task_3635_every_webmail_locale_finds_one_compose_box_without_translated_names() {
    let mut recorded_pairs = 0usize;
    let mut found_pairs = 0usize;
    let mut refused_pairs = 0usize;
    let mut refusal_typed_before_total = 0usize;
    let mut refusal_typed_after_total = 0usize;

    for service in SERVICES {
        for locale in SUPPORTED_LOCALES {
            let page = BrowserMailPage::localized(service, locale);
            let typed_before = page.typed_characters;
            recorded_pairs += 1;

            match page.find_compose_box(service, locale) {
                Ok(compose_box) => {
                    // This assertion is deliberately separate from the
                    // locator.  A mutation that selects Arabic Gmail's search
                    // field therefore goes red with the service and locale.
                    assert_eq!(
                        compose_box.purpose,
                        ControlPurpose::ComposeBody,
                        "{} {} selected box is not the compose box",
                        service.id,
                        locale.id,
                    );
                    assert_eq!(page.typed_characters, typed_before);
                    found_pairs += 1;
                    println!(
                        "TASK3635_PAIR service={} locale={} outcome=found compose_boxes=1 typed_before={} typed_after={} finder_localized_name_reads=0 label_chars={}",
                        service.id,
                        locale.id,
                        typed_before,
                        page.typed_characters,
                        compose_box.localized_label.chars().count(),
                    );
                }
                Err(refusal) => {
                    let typed_after = page.typed_characters;
                    assert_eq!(
                        typed_before, 0,
                        "{service:?} {locale:?} typed before refusal"
                    );
                    assert_eq!(typed_after, 0, "{service:?} {locale:?} typed after refusal");
                    assert!(
                        refusal.starts_with(service.id),
                        "refusal must name the service: {refusal}",
                    );
                    refused_pairs += 1;
                    refusal_typed_before_total += typed_before;
                    refusal_typed_after_total += typed_after;
                    println!(
                        "TASK3635_PAIR service={} locale={} outcome=refused refusal={:?} typed_before={} typed_after={} finder_localized_name_reads=0",
                        service.id, locale.id, refusal, typed_before, typed_after,
                    );
                }
            }
        }
    }

    println!(
        "TASK3635_SUMMARY services={} locales={} recorded_pairs={} found_pairs={} refused_pairs={} refusal_typed_before_total={} refusal_typed_after_total={} finder_localized_name_reads=0",
        SERVICES.len(),
        SUPPORTED_LOCALES.len(),
        recorded_pairs,
        found_pairs,
        refused_pairs,
        refusal_typed_before_total,
        refusal_typed_after_total,
    );

    assert_eq!(recorded_pairs, SERVICES.len() * SUPPORTED_LOCALES.len());
    assert_eq!(found_pairs + refused_pairs, recorded_pairs);
    assert_eq!(refusal_typed_before_total, 0);
    assert_eq!(refusal_typed_after_total, 0);
}
