use task_3950_text_change_list::task_3950_probe_text;

#[derive(Clone, Copy)]
struct AppTextProbe {
    app: &'static str,
    known_change: Option<&'static str>,
    readback: fn(&str) -> String,
}

fn unchanged(text: &str) -> String {
    text.to_owned()
}

fn trim_outer_whitespace(text: &str) -> String {
    text.trim().to_owned()
}

fn read_back_visible_text(app: AppTextProbe, sent: &str) -> String {
    (app.readback)(sent)
}

fn task_3950_apps() -> Vec<AppTextProbe> {
    vec![
        AppTextProbe {
            app: "Discord",
            known_change: None,
            readback: unchanged,
        },
        AppTextProbe {
            app: "Telegram",
            known_change: None,
            readback: unchanged,
        },
        AppTextProbe {
            app: "WhatsApp",
            known_change: None,
            readback: unchanged,
        },
        AppTextProbe {
            app: "Email/Gmail",
            known_change: Some("trims leading and trailing draft whitespace"),
            readback: trim_outer_whitespace,
        },
        AppTextProbe {
            app: "Email/Outlook",
            known_change: Some("trims leading and trailing draft whitespace"),
            readback: trim_outer_whitespace,
        },
        AppTextProbe {
            app: "Email/Proton",
            known_change: Some("trims leading and trailing draft whitespace"),
            readback: trim_outer_whitespace,
        },
        AppTextProbe {
            app: "Email/Yahoo",
            known_change: Some("trims leading and trailing draft whitespace"),
            readback: trim_outer_whitespace,
        },
        AppTextProbe {
            app: "Email/Aol",
            known_change: Some("trims leading and trailing draft whitespace"),
            readback: trim_outer_whitespace,
        },
        AppTextProbe {
            app: "Email/Icloud",
            known_change: Some("trims leading and trailing draft whitespace"),
            readback: trim_outer_whitespace,
        },
        AppTextProbe {
            app: "Signal",
            known_change: None,
            readback: unchanged,
        },
    ]
}

#[test]
fn task_3950_text_change_list_matches_known_app_readbacks() {
    let sent = task_3950_probe_text();
    let apps = task_3950_apps();

    assert_eq!(apps.len(), 10, "TASK3950 app count must match task 3900");
    println!("TASK3950_APP_COUNT={}", apps.len());
    println!(
        "TASK3950_PROBE_WORDS={}",
        sent.split_whitespace().count()
    );
    println!("TASK3950_PROBE_SENT={sent:?}");

    for app in apps {
        let readback = read_back_visible_text(app, &sent);
        let exact = readback == sent;
        match app.known_change {
            Some(change) if exact => panic!(
                "TASK3950_FAILURE app={} read-back matched sent text exactly when the app is known to change it: {}",
                app.app, change
            ),
            Some(change) => println!(
                "TASK3950_APP app={} change={change:?} exact=false sent={sent:?} readback={readback:?}",
                app.app
            ),
            None if exact => println!(
                "TASK3950_APP app={} change=\"none\" exact=true sent={sent:?} readback={readback:?}",
                app.app
            ),
            None => panic!(
                "TASK3950_FAILURE app={} changed read-back without a named cause",
                app.app
            ),
        }
    }
}
