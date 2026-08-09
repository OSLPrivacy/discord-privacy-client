use osl_privacy_hub::signal_place_reader::{
    inspect_signal_place, SignalDirectMessageAllowance, SignalPlace, SignalPlaceKind,
};
use osl_privacy_hub::signal_surface_finder::{
    find_signal_open_direct_message, task_1031_open_direct_message_fixture,
};

#[test]
fn task_1054_note_to_self_stays_separate_from_direct_message_allowances() {
    let (windows, nodes) = task_1031_open_direct_message_fixture();
    let surface = find_signal_open_direct_message(&windows, &nodes)
        .expect("TASK 1031 must find the Signal conversation surface");
    let no_op_reader = std::env::var("TASK1054_SIGNAL_PLACE_READER").as_deref() == Ok("noop");
    let mut reader_calls = 0usize;
    let mut reader = |_: &_| {
        reader_calls += 1;
        (!no_op_reader).then(|| SignalPlace {
            kind: SignalPlaceKind::NoteToSelf,
            // Deliberately collide with an allowed direct-message identifier.
            // Kind separation, not a convenient identifier mismatch, must keep
            // the direct-message allowance from leaking into note to self.
            stable_conversation_id: "signal-conversation-1054".to_owned(),
        })
    };
    let direct_message_allowances = vec![SignalDirectMessageAllowance {
        stable_conversation_id: "signal-conversation-1054".to_owned(),
    }];
    let allowance_count_before = direct_message_allowances.len();
    let mut direct_message_reader = |_: &_| {
        Some(SignalPlace {
            kind: SignalPlaceKind::DirectMessage,
            stable_conversation_id: "signal-conversation-1054".to_owned(),
        })
    };
    let direct_message = inspect_signal_place(
        &surface,
        &mut direct_message_reader,
        &direct_message_allowances,
    )
    .expect("positive control must inspect the allowed direct message");
    assert!(direct_message.direct_message_allowed);

    let inspected = inspect_signal_place(&surface, &mut reader, &direct_message_allowances)
        .expect("the Signal place reader must directly return note to self");

    assert_eq!(reader_calls, 1);
    assert_eq!(inspected.kind, SignalPlaceKind::NoteToSelf);
    assert!(!inspected.direct_message_allowed);
    assert_eq!(direct_message_allowances.len(), allowance_count_before);
    println!(
        "TASK1054 signal_place_reader_calls={reader_calls} kind={} direct_message_allowance_matched={} direct_message_allowance_inherited={} allowlist_entries_before={} allowlist_entries_after={}",
        inspected.kind.as_str(),
        direct_message.direct_message_allowed,
        inspected.direct_message_allowed,
        allowance_count_before,
        direct_message_allowances.len(),
    );
}
