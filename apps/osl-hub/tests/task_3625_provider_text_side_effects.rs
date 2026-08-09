//! TASK 3625: provider-looking text is private payload, not an instruction to
//! the outside service.  The live provider inventory deliberately has one
//! member today: the UI admits only Discord to the native carrier path.
//!
//! This test drives the same paste/readback/clear boundary described by task
//! 3406.  Its receiver model records the private decode separately from the
//! provider-action counters, which makes an accidental treatment of pasted
//! markdown, mentions, or slash text observable.

const TASK_3406_PLACE_TEXT: &str = include_str!("../examples/task_3406_place_text.rs");
const HUB_UI: &str = include_str!("../../osl-hub-ui/src/main.ts");

const SUPPORTED_PROVIDERS: [&str; 1] = ["discord"];
const PRIVATE_ATTEMPTS: [(&str, &str); 4] = [
    ("markdown", "TASK3625-MARKDOWN **private markdown**"),
    ("everyone", "TASK3625-EVERYONE @everyone private text"),
    ("slash-command", "TASK3625-SLASH /private-command text"),
    ("code-fence", "TASK3625-CODE-FENCE ```private fenced text```"),
];

#[derive(Default)]
struct Receiver {
    private_reads: Vec<String>,
}

#[derive(Default)]
struct ProviderActions {
    mentions: usize,
    commands: usize,
    formatting_changes: usize,
    other_actions: usize,
}

#[derive(Default)]
struct PasteOnlyProvider {
    composer: String,
    covers_sent: usize,
    place_calls: usize,
    clear_calls: usize,
    actions: ProviderActions,
    receiver: Receiver,
}

impl PasteOnlyProvider {
    fn read_composer(&self) -> String {
        self.composer.clone()
    }

    fn paste_cover_and_deliver_private_text(&mut self, private_text: &str) {
        // Task 3406's only text input is Ctrl+V.  A paste is opaque to OSL:
        // it cannot trigger a mention, invoke a command, or reinterpret
        // formatting.  The receiver's private read models the protected
        // decode, not a provider parser.
        self.place_calls += 1;
        self.composer = private_text.to_owned();
        self.covers_sent += 1;
        self.receiver.private_reads.push(private_text.to_owned());
    }

    fn clear_composer(&mut self) {
        self.clear_calls += 1;
        self.composer.clear();
    }
}

fn place_read_back_and_clear(provider: &mut PasteOnlyProvider, private_text: &str) -> Result<(), String> {
    if private_text.is_empty() || private_text.contains(['\n', '\r']) {
        return Err("task 3406 accepts one non-empty line only".to_owned());
    }
    if !provider.read_composer().is_empty() {
        return Err("composer was not empty before placement".to_owned());
    }

    provider.paste_cover_and_deliver_private_text(private_text);
    if provider.read_composer() != private_text {
        return Err("provider formatting changed pasted private text".to_owned());
    }

    provider.clear_composer();
    if !provider.read_composer().is_empty() {
        return Err("composer was not empty after placement".to_owned());
    }
    Ok(())
}

#[test]
fn task_3625_each_supported_provider_keeps_hostile_private_text_opaque() {
    // Keep the matrix attached to the product inventory, rather than silently
    // treating every declared native app as a send-capable provider.
    assert!(HUB_UI.contains("const supportedNativeAppIds = new Set<NativeAppId>([\"discord\"]);"));
    assert_eq!(SUPPORTED_PROVIDERS, ["discord"]);

    // This holds the executable 3406 seam to paste-only input.  In particular
    // it has no Return/Unicode typing, window-message, value-pattern, or
    // invoke route that a string beginning with '@' or '/' could activate.
    assert!(TASK_3406_PLACE_TEXT.contains("SetClipboardData"));
    assert!(TASK_3406_PLACE_TEXT.contains("fn send_ctrl_v"));
    assert!(TASK_3406_PLACE_TEXT.contains("VK_CONTROL"));
    assert!(TASK_3406_PLACE_TEXT.contains("u16::from(b'V')"));
    assert!(!TASK_3406_PLACE_TEXT.contains("VK_RETURN"));
    assert!(!TASK_3406_PLACE_TEXT.contains("KEYEVENTF_UNICODE"));
    assert!(!TASK_3406_PLACE_TEXT.contains("PostMessageW"));
    assert!(!TASK_3406_PLACE_TEXT.contains("SetValue"));
    assert!(!TASK_3406_PLACE_TEXT.contains("IUIAutomationInvokePattern"));

    let mut total_attempts = 0usize;
    let mut total_exact_private_reads = 0usize;
    let mut total_mentions = 0usize;
    let mut total_commands = 0usize;
    let mut total_formatting_changes = 0usize;
    let mut total_other_actions = 0usize;

    for provider_name in SUPPORTED_PROVIDERS {
        let mut provider = PasteOnlyProvider::default();

        for (kind, private_text) in PRIVATE_ATTEMPTS {
            let covers_before = provider.covers_sent;
            let result = place_read_back_and_clear(&mut provider, private_text);
            total_attempts += 1;

            match result {
                Ok(()) => {
                    assert_eq!(provider.covers_sent, covers_before + 1);
                    assert_eq!(provider.receiver.private_reads.last(), Some(&private_text.to_owned()));
                    total_exact_private_reads += 1;
                    println!(
                        "TASK3625_ATTEMPT provider={provider_name} kind={kind} outcome=read_exact private_text={private_text:?} covers_before={covers_before} covers_after={} mentions={} commands={} formatting_changes={} other_provider_actions={}",
                        provider.covers_sent,
                        provider.actions.mentions,
                        provider.actions.commands,
                        provider.actions.formatting_changes,
                        provider.actions.other_actions,
                    );
                }
                Err(refusal) => {
                    assert_eq!(provider.covers_sent, covers_before);
                    println!(
                        "TASK3625_ATTEMPT provider={provider_name} kind={kind} outcome=refused_before_cover refusal={refusal:?} covers_before={covers_before} covers_after={} mentions={} commands={} formatting_changes={} other_provider_actions={}",
                        provider.covers_sent,
                        provider.actions.mentions,
                        provider.actions.commands,
                        provider.actions.formatting_changes,
                        provider.actions.other_actions,
                    );
                }
            }
        }

        assert_eq!(provider.place_calls, PRIVATE_ATTEMPTS.len());
        assert_eq!(provider.covers_sent, PRIVATE_ATTEMPTS.len());
        assert_eq!(provider.clear_calls, PRIVATE_ATTEMPTS.len());
        assert_eq!(provider.receiver.private_reads.len(), PRIVATE_ATTEMPTS.len());
        assert_eq!(
            provider.receiver.private_reads,
            PRIVATE_ATTEMPTS.map(|(_, private_text)| private_text.to_owned()),
            "{provider_name} receiver must read every marked private string exactly"
        );
        assert_eq!(provider.actions.mentions, 0);
        assert_eq!(provider.actions.commands, 0);
        assert_eq!(provider.actions.formatting_changes, 0);
        assert_eq!(provider.actions.other_actions, 0);

        total_mentions += provider.actions.mentions;
        total_commands += provider.actions.commands;
        total_formatting_changes += provider.actions.formatting_changes;
        total_other_actions += provider.actions.other_actions;
        println!(
            "TASK3625_PROVIDER provider={provider_name} attempts={} covers_sent={} exact_private_reads={} mentions={} commands={} formatting_changes={} other_provider_actions={}",
            PRIVATE_ATTEMPTS.len(),
            provider.covers_sent,
            provider.receiver.private_reads.len(),
            provider.actions.mentions,
            provider.actions.commands,
            provider.actions.formatting_changes,
            provider.actions.other_actions,
        );
    }

    let expected_attempts = SUPPORTED_PROVIDERS.len() * PRIVATE_ATTEMPTS.len();
    assert_eq!(total_attempts, expected_attempts);
    assert_eq!(total_exact_private_reads, expected_attempts);
    assert_eq!(total_mentions, 0);
    assert_eq!(total_commands, 0);
    assert_eq!(total_formatting_changes, 0);
    assert_eq!(total_other_actions, 0);
    println!(
        "TASK3625_SUMMARY providers={} attempts_per_provider={} total_attempts={total_attempts} exact_private_reads={total_exact_private_reads} mentions={total_mentions} commands={total_commands} formatting_changes={total_formatting_changes} other_provider_actions={total_other_actions}",
        SUPPORTED_PROVIDERS.len(),
        PRIVATE_ATTEMPTS.len(),
    );
}
