use std::process::Command;

const MESSENGER_PLACE_KIND: &str = env!("CARGO_BIN_EXE_messenger-place-kind");

#[test]
fn task_0164_messenger_fixture_places_resolve_and_public_post_exits_1() {
    let direct_message = Command::new(MESSENGER_PLACE_KIND)
        .arg("direct_message")
        .output()
        .expect("run Messenger direct-message fixture place");
    let direct_stdout = String::from_utf8(direct_message.stdout).expect("stdout is UTF-8");
    let direct_stderr = String::from_utf8(direct_message.stderr).expect("stderr is UTF-8");
    assert!(
        direct_message.status.success(),
        "direct_message should resolve, stdout={direct_stdout}, stderr={direct_stderr}"
    );
    assert!(
        direct_stdout.contains("kind=direct_message status=allowed"),
        "{direct_stdout}"
    );

    let group_chat = Command::new(MESSENGER_PLACE_KIND)
        .arg("group_chat")
        .output()
        .expect("run Messenger group-chat fixture place");
    let group_stdout = String::from_utf8(group_chat.stdout).expect("stdout is UTF-8");
    let group_stderr = String::from_utf8(group_chat.stderr).expect("stderr is UTF-8");
    assert!(
        group_chat.status.success(),
        "group_chat should resolve, stdout={group_stdout}, stderr={group_stderr}"
    );
    assert!(
        group_stdout.contains("kind=group_chat status=allowed"),
        "{group_stdout}"
    );

    let public_post = Command::new(MESSENGER_PLACE_KIND)
        .arg("public_post")
        .output()
        .expect("run Messenger public-post fixture place");
    let public_stderr = String::from_utf8(public_post.stderr).expect("stderr is UTF-8");
    assert_eq!(public_post.status.code(), Some(1), "{public_stderr}");
    assert!(
        public_stderr.contains("OSL: unknown Messenger whitelist kind 'public_post'"),
        "{public_stderr}"
    );

    println!(
        "TASK0164 direct_message_exit={} direct_message_status=allowed group_chat_exit={} group_chat_status=allowed public_post_exit=1 public_post_error=\"{}\"",
        direct_message.status.code().unwrap_or(0),
        group_chat.status.code().unwrap_or(0),
        public_stderr.trim()
    );
}
