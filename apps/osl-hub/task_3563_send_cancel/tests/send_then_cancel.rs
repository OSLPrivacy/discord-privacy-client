use std::{env, fs, path::PathBuf};

use task_3563_send_cancel::press_send_then_cancel_immediately;

const HUB_MAIN: &str = include_str!("../../../osl-hub-ui/src/main.ts");
const CHAT_VIEW: &str = include_str!("../../../osl-hub-ui/src/osl-chats-view.ts");
const MAIL_VIEW: &str = include_str!("../../../osl-hub-ui/src/osl-mail-view.ts");
const DISCORD_OVERLAY: &str = include_str!("../../../osl-hub-ui/src/overlay.ts");
const DISCORD_OVERLAY_HTML: &str = include_str!("../../../osl-hub-ui/overlay.html");
const WHATSAPP_OVERLAY_HTML: &str = include_str!("../../../osl-hub-ui/whatsapp-overlay.html");

const SURFACES: [(&str, &str); 3] = [
    ("osl-chat", "TASK3563-OSL-CHAT-SEND-CANCEL"),
    ("osl-mail", "TASK3563-OSL-MAIL-SEND-CANCEL"),
    ("discord-protected-overlay", "TASK3563-DISCORD-SEND-CANCEL"),
];

fn report_path() -> PathBuf {
    env::var_os("TASK3563_REPORT_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| env::temp_dir().join("task-3563-send-then-cancel.jsonl"))
}

#[test]
fn task_3563_send_then_immediate_cancel_keeps_delivery_and_local_completion_in_lockstep() {
    // These anchors make the enumerated surfaces concrete.  A new visible
    // Send control or a renamed dispatch path must extend this audit rather
    // than silently falling outside it.
    assert!(CHAT_VIEW.contains("class=\"osl-chat-send\" type=\"submit\""));
    assert!(HUB_MAIN.contains("async function sendOslChat(event: SubmitEvent): Promise<void>"));
    assert!(HUB_MAIN.contains("state: \"sent\" as const"));

    assert!(MAIL_VIEW.contains("Send protected"));
    assert!(
        HUB_MAIN.contains("async function sendOslMailForm(form: HTMLFormElement): Promise<void>")
    );
    assert!(HUB_MAIN.contains("sendOslMail(recipient, subject, body)"));

    assert!(DISCORD_OVERLAY_HTML.contains("id=\"prepare-protected\" class=\"send-action\""));
    assert!(DISCORD_OVERLAY.contains("async function sendDraft(): Promise<void>"));
    assert!(DISCORD_OVERLAY.contains("sendNativeDiscordOverlayCarrier("));

    // WhatsApp intentionally offers Copy, and chat attachments intentionally
    // offer Choose file.  Neither is a surface that can dispatch Send.
    assert!(WHATSAPP_OVERLAY_HTML.contains(">Copy</button>"));
    assert!(!WHATSAPP_OVERLAY_HTML.contains(">Send</button>"));
    assert!(HUB_MAIN.contains("id=\"osl-chat-attach\""));
    assert!(!HUB_MAIN.contains("id=\"osl-chat-attach\" type=\"submit\""));

    let runs: Vec<_> = SURFACES
        .into_iter()
        .map(|(surface, mark)| press_send_then_cancel_immediately(surface, mark))
        .collect();

    println!(
        "TASK3563 send_surfaces=osl-chat,osl-mail,discord-protected-overlay surface_count={}",
        runs.len()
    );

    for run in &runs {
        println!(
            "TASK3563 run surface={} mark={} send_presses={} cancel_presses={} documented_results={} result={} delivery_count={} local_completed_send_count={} counts_match={}",
            run.surface,
            run.mark,
            run.send_presses,
            run.cancel_presses,
            run.documented_results,
            run.result.as_str(),
            run.delivery_count,
            run.local_completed_send_count,
            run.counts_match(),
        );
        assert_eq!(
            run.send_presses, 1,
            "{} did not receive exactly one Send press",
            run.surface
        );
        assert_eq!(
            run.cancel_presses, 1,
            "{} did not receive exactly one immediate Cancel press",
            run.surface
        );
        assert_eq!(
            run.documented_results, 1,
            "{} did not end in exactly one documented result",
            run.surface
        );
        assert!(
            matches!(run.delivery_count, 0 | 1),
            "{} recorded an invalid delivery count",
            run.surface
        );
        assert!(
            matches!(run.local_completed_send_count, 0 | 1),
            "{} recorded an invalid local completed-send count",
            run.surface
        );
        assert!(
            run.counts_match(),
            "{} split delivery/local completion counts",
            run.surface
        );
    }

    let cancelled = runs
        .iter()
        .filter(|run| run.result.as_str() == "cancelled-before-dispatch")
        .count();
    let deliveries: u8 = runs.iter().map(|run| run.delivery_count).sum();
    let local_completed: u8 = runs.iter().map(|run| run.local_completed_send_count).sum();
    println!(
        "TASK3563 summary runs={} documented_results={} immediate_cancels={} matching_delivery_local_pairs={}/{} total_delivery_count={} total_local_completed_send_count={}",
        runs.len(),
        runs.iter().map(|run| run.documented_results as usize).sum::<usize>(),
        cancelled,
        runs.iter().filter(|run| run.counts_match()).count(),
        runs.len(),
        deliveries,
        local_completed,
    );

    let mut report = String::from("{\"task\":3563,\"send_surfaces\":[\"osl-chat\",\"osl-mail\",\"discord-protected-overlay\"],\"surface_count\":3}\n");
    for run in &runs {
        report.push_str(&format!(
            "{{\"surface\":\"{}\",\"mark\":\"{}\",\"send_presses\":{},\"cancel_presses\":{},\"documented_results\":{},\"result\":\"{}\",\"delivery_count\":{},\"local_completed_send_count\":{},\"counts_match\":{}}}\n",
            run.surface,
            run.mark,
            run.send_presses,
            run.cancel_presses,
            run.documented_results,
            run.result.as_str(),
            run.delivery_count,
            run.local_completed_send_count,
            run.counts_match(),
        ));
    }
    fs::write(report_path(), report).expect("TASK3563 could not write its measured run report");
}
