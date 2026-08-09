//! TASK 3637: provider UI changes must either yield one proven chat composer or
//! stop before a private character can reach the provider.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Provider {
    Discord,
    Telegram,
    Signal,
    WhatsApp,
    X,
    Instagram,
    Messenger,
}

impl Provider {
    const ALL: [Self; 7] = [
        Self::Discord,
        Self::Telegram,
        Self::Signal,
        Self::WhatsApp,
        Self::X,
        Self::Instagram,
        Self::Messenger,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Discord => "Discord",
            Self::Telegram => "Telegram",
            Self::Signal => "Signal",
            Self::WhatsApp => "WhatsApp",
            Self::X => "X",
            Self::Instagram => "Instagram",
            Self::Messenger => "Messenger",
        }
    }

    fn composer_labels(self) -> &'static [&'static str] {
        match self {
            Self::Discord => &["message #osl-lab"],
            Self::Telegram => &["write a message..."],
            Self::Signal => &["send a message"],
            Self::WhatsApp => &["type a message"],
            Self::X => &["start a message"],
            Self::Instagram => &["message..."],
            Self::Messenger => &["message"],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Layout {
    AlternateTheme,
    Compact,
    Beta,
}

impl Layout {
    const ALL: [Self; 3] = [Self::AlternateTheme, Self::Compact, Self::Beta];

    fn name(self) -> &'static str {
        match self {
            Self::AlternateTheme => "alternate-theme",
            Self::Compact => "compact",
            Self::Beta => "beta",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Role {
    Edit,
    Document,
    Text,
}

#[derive(Clone, Debug)]
struct Node {
    id: String,
    role: Role,
    accessible_name: String,
    enabled: bool,
    keyboard_focusable: bool,
    writable: bool,
    in_active_chat: bool,
}

#[derive(Debug, Eq, PartialEq)]
enum LocateError {
    NoSafeComposer,
    AmbiguousComposer { candidates: usize },
}

#[derive(Default, Debug)]
struct ProviderEffectLog {
    typed_characters: usize,
    sent_messages: usize,
}

fn normalise(value: &str) -> String {
    value
        .trim()
        .trim_end_matches('.')
        .replace('\u{2026}', "")
        .to_lowercase()
}

fn is_composer(provider: Provider, node: &Node) -> bool {
    matches!(node.role, Role::Edit | Role::Document | Role::Text)
        && node.enabled
        && node.keyboard_focusable
        && node.writable
        && node.in_active_chat
        && provider
            .composer_labels()
            .iter()
            .any(|label| normalise(&node.accessible_name) == normalise(label))
}

fn locate_composer(provider: Provider, nodes: &[Node]) -> Result<&Node, LocateError> {
    let candidates: Vec<&Node> = nodes
        .iter()
        .filter(|node| is_composer(provider, node))
        .collect();
    match candidates.as_slice() {
        [composer] => Ok(composer),
        [] => Err(LocateError::NoSafeComposer),
        _ => Err(LocateError::AmbiguousComposer {
            candidates: candidates.len(),
        }),
    }
}

/// This is deliberately the only operation that can count a typed character.
/// It never sends: the task is about locating/refusing before a placement.
fn stage_private_text(
    provider: Provider,
    nodes: &[Node],
    private_text: &str,
    effects: &mut ProviderEffectLog,
) -> Result<String, LocateError> {
    let composer = locate_composer(provider, nodes)?;
    effects.typed_characters += private_text.chars().count();
    Ok(composer.id.clone())
}

fn layout_fixture(provider: Provider, layout: Layout) -> (Vec<Node>, String) {
    let prefix = format!("{}.{}", provider.name().to_lowercase(), layout.name());
    let composer_id = format!("{prefix}.composer");
    let composer_role = match layout {
        Layout::AlternateTheme => Role::Document,
        Layout::Compact => Role::Edit,
        Layout::Beta => Role::Text,
    };
    let label = provider.composer_labels()[0];
    (
        vec![
            Node {
                id: format!("{prefix}.search"),
                role: Role::Edit,
                accessible_name: "Search conversations".into(),
                enabled: true,
                keyboard_focusable: true,
                writable: true,
                in_active_chat: false,
            },
            Node {
                id: format!("{prefix}.transcript"),
                role: Role::Document,
                accessible_name: "Message transcript".into(),
                enabled: true,
                keyboard_focusable: false,
                writable: false,
                in_active_chat: true,
            },
            Node {
                id: composer_id.clone(),
                role: composer_role,
                accessible_name: label.into(),
                enabled: true,
                keyboard_focusable: true,
                writable: true,
                in_active_chat: true,
            },
        ],
        composer_id,
    )
}

#[test]
fn task_3637_all_provider_alternate_layouts_select_once_or_refuse_before_typing() {
    const PRIVATE_MARK: &str = "private-3637";
    let mut cases = 0usize;
    let mut selected = 0usize;
    let mut refusals = 0usize;

    for provider in Provider::ALL {
        for layout in Layout::ALL {
            cases += 1;
            let (nodes, expected_id) = layout_fixture(provider, layout);
            let mut effects = ProviderEffectLog::default();
            let selected_id = stage_private_text(provider, &nodes, PRIVATE_MARK, &mut effects)
                .expect("each recorded provider-layout fixture has one correct composer");
            assert_eq!(selected_id, expected_id);
            assert_eq!(effects.typed_characters, PRIVATE_MARK.chars().count());
            assert_eq!(effects.sent_messages, 0);
            selected += 1;
            println!(
                "TASK3637_CASE provider={} layout={} outcome=selected candidates=1 typed={} sent={}",
                provider.name(),
                layout.name(),
                effects.typed_characters,
                effects.sent_messages
            );

            // A provider remount that presents a second matching box is unsafe.
            // Check the same decision boundary refuses it before text staging.
            let mut ambiguous = nodes.clone();
            let mut duplicate = ambiguous[2].clone();
            duplicate.id.push_str(".duplicate");
            ambiguous.push(duplicate);
            let mut refusal_effects = ProviderEffectLog::default();
            let before_typed = refusal_effects.typed_characters;
            let before_sent = refusal_effects.sent_messages;
            let refusal = stage_private_text(
                provider,
                &ambiguous,
                PRIVATE_MARK,
                &mut refusal_effects,
            );
            assert_eq!(
                refusal,
                Err(LocateError::AmbiguousComposer { candidates: 2 })
            );
            assert_eq!(refusal_effects.typed_characters, before_typed);
            assert_eq!(refusal_effects.sent_messages, before_sent);
            refusals += 1;
            println!(
                "TASK3637_REFUSAL provider={} layout={} reason=ambiguous_composer typed_before={} typed_after={} sent_before={} sent_after={}",
                provider.name(),
                layout.name(),
                before_typed,
                refusal_effects.typed_characters,
                before_sent,
                refusal_effects.sent_messages
            );
        }
    }

    assert_eq!(cases, 21);
    assert_eq!(selected, 21);
    assert_eq!(refusals, 21);
    println!(
        "TASK3637_SUMMARY cases={cases} selected_exactly_one={selected} refusals={refusals} refusal_typed_delta=0 refusal_sent_delta=0"
    );
}
