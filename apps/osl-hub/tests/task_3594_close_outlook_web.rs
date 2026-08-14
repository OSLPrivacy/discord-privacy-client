const PLACE_TEXT: &str = include_str!("../examples/task_3406_place_text.rs");
const HARNESS: &str = include_str!("../../../scripts/qa/task-3594-close-outlook-web.ps1");

#[test]
fn task_3594_closes_outlook_web_at_each_named_cover_placement_step() {
    assert!(HARNESS.contains("[ValidateSet('Outlook web')][string]$App = 'Outlook web'"));
    assert!(HARNESS.contains("$steps = @('empty-readback','marked-paste','exact-readback','clear')"));
    assert!(HARNESS.contains("TASK3585_PAUSED provider=Outlook web"));
    assert!(HARNESS.contains("Stop-Process -Id ([int]$Matches[1]) -Force"));
    assert!(PLACE_TEXT.contains("TASK3585_PAUSED provider={} pid={} step={step}"));
    assert!(PLACE_TEXT.contains("GetExitCodeProcess"));
}

#[test]
fn task_3594_requires_one_control_then_preserves_all_count_and_draft_invariants() {
    assert!(HARNESS.contains("fresh control requires receiver_marked=0 and outlook_web_sent=0"));
    assert!(HARNESS.contains("live Outlook web control must produce receiver_marked=1 and outlook_web_sent=1"));
    assert!(HARNESS.contains("$after.ReceiverMarked -ne 1 -or $after.OutlookWebSent -ne 1"));
    assert!(HARNESS.contains("$after.PrivateDraft -cne $control.PrivateDraft -or $after.Recipient -cne $control.Recipient -or $after.Subject -cne $control.Subject"));
    assert!(HARNESS.contains("Retry placement in Outlook web"));
    assert!(HARNESS.contains("control_receiver_marked=0->1 control_outlook_web_sent=0->1 close_attempts=4"));
}
