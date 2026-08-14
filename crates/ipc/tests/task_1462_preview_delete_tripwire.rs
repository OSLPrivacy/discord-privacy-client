//! TASK 1462 - break preview deletion.
//!
//! do: Run preview against a fixture whose delete method fails the test if
//! called.
//!
//! done when: preview completes with the failing delete method called 0 times
//! while the preview reader's call count is above 0, and forcing 1 delete call
//! makes the check fail.
//!
//! The difference between this and TASK 1460's check is the fixture. There the
//! connection's `delete_message` returned `Ok(())` and merely incremented a
//! counter, so a stray deletion was only caught later, by an assertion at the
//! end of the test. Here `delete_message` is a tripwire: it fails the test at
//! the moment it is called, from inside the connection, before the preview can
//! return anything for a later assertion to inspect. A preview that deleted and
//! then somehow reported a clean summary could not slip past this fixture.
//!
//! The tripwire is armed on the value the preview is *handed*, so the deletion
//! is genuinely one call away on the object in the preview's hand -- the zero
//! is a measured property of the preview, not an absence of capability.

use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering};

use ipc::bad_message_preview::{
    preview_bad_message_rules, BadMessagePreview, PreviewMessage, ServiceConnection,
    PREVIEW_TREATMENT,
};
use ipc::bad_message_rules::BadMessageRule;

/// The exact failure a deleting preview earns. Asserted on, so it cannot drift.
fn tripwire_failure(message_id: &str) -> String {
    format!(
        "TASK1462 TRIPWIRE: preview called delete_message on '{message_id}' -- \
         the preview is not find-only"
    )
}

/// A service connection whose delete method fails the test if it is ever called.
///
/// `read_messages` is ordinary and counted. `delete_message` counts the attempt
/// *first* -- so the count is observable even on the failing path -- and then
/// panics, which is how a Rust test is failed from inside a callee.
struct TripwireServiceConnection {
    messages: Vec<PreviewMessage>,
    read_calls: AtomicUsize,
    deletion_calls: AtomicUsize,
}

impl TripwireServiceConnection {
    fn new(messages: Vec<PreviewMessage>) -> Self {
        Self {
            messages,
            read_calls: AtomicUsize::new(0),
            deletion_calls: AtomicUsize::new(0),
        }
    }

    fn read_call_count(&self) -> usize {
        self.read_calls.load(Ordering::SeqCst)
    }

    fn deletion_call_count(&self) -> usize {
        self.deletion_calls.load(Ordering::SeqCst)
    }
}

impl ServiceConnection for TripwireServiceConnection {
    fn read_messages(&self) -> Result<Vec<PreviewMessage>, String> {
        self.read_calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.messages.clone())
    }

    fn delete_message(&self, message_id: &str) -> Result<(), String> {
        self.deletion_calls.fetch_add(1, Ordering::SeqCst);
        panic!("{}", tripwire_failure(message_id));
    }
}

/// The three fixtures. Exactly one of them carries the private word.
fn fixtures() -> Vec<PreviewMessage> {
    vec![
        PreviewMessage::new("msg-1", "general", "lunch at one, nothing private here"),
        PreviewMessage::new(
            "msg-2",
            "project-room",
            "reminder: the MAPLE-1462 rollout slips a week",
        ),
        PreviewMessage::new("msg-3", "general", "see you tomorrow"),
    ]
}

fn rules() -> Vec<BadMessageRule> {
    vec![BadMessageRule {
        rule_name: "private words".to_string(),
        private_word: "MAPLE-1462".to_string(),
    }]
}

/// The check, named once and used by both tests: a preview run is find-only
/// when it completed, marked the fixture it should mark, read through the
/// connection at least once, and made zero delete calls.
fn find_only_verdict(
    preview: &BadMessagePreview,
    read_calls: usize,
    deletion_calls: usize,
) -> Result<(), String> {
    if deletion_calls != 0 {
        return Err(format!(
            "check failed: delete call count is {deletion_calls}, expected 0"
        ));
    }
    if read_calls == 0 {
        return Err("check failed: read call count is 0, expected above 0".to_string());
    }
    if preview.match_count() != 1 || preview.matches[0].message_id != "msg-2" {
        return Err(format!(
            "check failed: expected 1 match on msg-2, got {}",
            preview.match_count()
        ));
    }
    Ok(())
}

/// Run `previewer` against a freshly armed tripwire connection and report what
/// happened, converting a tripwire panic into a verdict instead of letting it
/// abort the harness. Used for the forced-deletion half of the finish line.
fn run_against_tripwire<F>(previewer: F) -> (Result<(), String>, usize, usize)
where
    F: FnOnce(&[BadMessageRule], &TripwireServiceConnection) -> Result<BadMessagePreview, String>,
{
    let connection = TripwireServiceConnection::new(fixtures());
    let saved_hook = panic::take_hook();
    panic::set_hook(Box::new(|info| {
        println!("TASK1462 tripwire fired: {info}");
    }));
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| previewer(&rules(), &connection)));
    panic::set_hook(saved_hook);

    let read_calls = connection.read_call_count();
    let deletion_calls = connection.deletion_call_count();
    let verdict = match outcome {
        Err(payload) => {
            let text = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "<non-string panic>".to_string());
            Err(format!("check failed: {text}"))
        }
        Ok(Err(error)) => Err(format!("check failed: preview errored: {error}")),
        Ok(Ok(preview)) => find_only_verdict(&preview, read_calls, deletion_calls),
    };
    (verdict, read_calls, deletion_calls)
}

/// The finish line's first half, run with no safety net: the real preview is
/// handed the tripwire connection directly. If it deletes, the panic from
/// `delete_message` fails this test on the spot.
#[test]
fn task_1462_preview_completes_without_tripping_the_deleting_connection() {
    let connection = TripwireServiceConnection::new(fixtures());

    let preview = preview_bad_message_rules(&rules(), &connection)
        .expect("preview did not complete against the tripwire fixture");

    let read_calls = connection.read_call_count();
    let deletion_calls = connection.deletion_call_count();
    println!(
        "TASK1462 preview completed scanned={} match_count={} preview_deleted_count={} \
         tripwire_delete_calls={} preview_read_calls={}",
        preview.scanned_message_count,
        preview.match_count(),
        preview.deleted_count,
        deletion_calls,
        read_calls
    );
    for hit in &preview.matches {
        println!(
            "TASK1462 preview row message_id={} channel={} rule={} matched_word={} treatment={}",
            hit.message_id, hit.channel, hit.rule_name, hit.matched_word, hit.treatment
        );
    }

    assert_eq!(
        deletion_calls, 0,
        "the failing delete method was called {deletion_calls} times"
    );
    assert!(
        read_calls > 0,
        "the preview reader's call count is {read_calls}, expected above 0"
    );

    assert_eq!(preview.scanned_message_count, 3);
    assert_eq!(preview.match_count(), 1);
    assert_eq!(preview.matches[0].message_id, "msg-2");
    assert_eq!(preview.matches[0].matched_word, "MAPLE-1462");
    assert_eq!(preview.matches[0].treatment, PREVIEW_TREATMENT);
    assert_eq!(preview.deleted_count, 0);

    assert_eq!(
        find_only_verdict(&preview, read_calls, deletion_calls),
        Ok(()),
        "the find-only check did not pass on the real preview"
    );
}

/// The finish line's second half: force exactly one delete call through the
/// same connection, from a previewer that is otherwise the real one, and show
/// the same check goes from passing to failing.
#[test]
fn task_1462_forcing_one_delete_call_makes_the_check_fail() {
    let (clean_verdict, clean_reads, clean_deletes) =
        run_against_tripwire(|rules, connection| preview_bad_message_rules(rules, connection));
    println!(
        "TASK1462 unforced verdict={clean_verdict:?} read_calls={clean_reads} \
         delete_calls={clean_deletes}"
    );
    assert_eq!(clean_verdict, Ok(()));
    assert_eq!(clean_deletes, 0);
    assert!(clean_reads > 0);

    let (forced_verdict, forced_reads, forced_deletes) =
        run_against_tripwire(|rules, connection| {
            let preview = preview_bad_message_rules(rules, connection)?;
            // The single forced deletion. Everything above this line is the
            // real preview; this is the one call the finish line asks for.
            let first = &preview.matches[0];
            connection.delete_message(&first.message_id)?;
            Ok(preview)
        });
    println!(
        "TASK1462 forced verdict={forced_verdict:?} read_calls={forced_reads} \
         delete_calls={forced_deletes}"
    );

    assert_eq!(
        forced_deletes, 1,
        "expected exactly 1 forced delete call, got {forced_deletes}"
    );
    assert!(forced_reads > 0);
    let failure = forced_verdict.expect_err("forcing a delete call left the check passing");
    assert!(
        failure.contains(&tripwire_failure("msg-2")),
        "the check failed for the wrong reason: {failure}"
    );
}
