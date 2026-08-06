#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RightClickTargetKind {
    ScreenArea,
    Control,
    ProtectedText,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MenuActionSafety {
    Safe,
    Unsafe,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafeEffect {
    FocusArea(&'static str),
    OpenPanel(&'static str),
    OpenRoute(&'static str),
    CopyLiteral(&'static str),
    CopyRedactedProtectedSummary,
    ToggleBoolean(&'static str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RightClickMenuAction {
    pub id: &'static str,
    pub label: &'static str,
    pub safety: MenuActionSafety,
    pub safe_result: &'static str,
    pub effect: SafeEffect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RightClickMenu {
    pub id: &'static str,
    pub title: &'static str,
    pub actions: &'static [RightClickMenuAction],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RightClickTarget {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: RightClickTargetKind,
    pub menu: Option<RightClickMenu>,
}

#[derive(Debug, Default, Eq, PartialEq)]
pub struct RightClickAuditState {
    pub focused_area: Option<String>,
    pub open_panel: Option<String>,
    pub route: Option<String>,
    pub clipboard: Option<String>,
    pub quiet_mode: bool,
    pub unsafe_actions_run: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RightClickActionRun {
    pub target_id: &'static str,
    pub menu_id: &'static str,
    pub action_id: &'static str,
    pub result_name: &'static str,
    pub did_the_thing: bool,
    pub exposed_private_marks: usize,
    pub unsafe_run: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RightClickMenuRecord {
    pub target_id: &'static str,
    pub target_name: &'static str,
    pub menu_id: Option<&'static str>,
    pub action_results: Vec<RightClickActionRun>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RightClickAuditReport {
    pub records: Vec<RightClickMenuRecord>,
    pub areas_showing_menu: usize,
    pub safe_actions_run: usize,
    pub named_safe_actions_run: usize,
    pub private_marks_exposed: usize,
    pub unsafe_actions_run: usize,
    pub unnamed_or_unsafe_menu_actions: usize,
    pub failed_safe_results: usize,
}

const PRIVATE_MARKS: &[&str] = &[
    "PRIVATE-MARK:alpha",
    "OSL1.PEER.private-capsule",
    "protected plaintext: meet at 19:00",
];

const WORKSPACE_ACTIONS: &[RightClickMenuAction] = &[RightClickMenuAction {
    id: "focus_workspace",
    label: "Focus workspace",
    safety: MenuActionSafety::Safe,
    safe_result: "focused workspace",
    effect: SafeEffect::FocusArea("workspace"),
}];

const NAV_ACTIONS: &[RightClickMenuAction] = &[
    RightClickMenuAction {
        id: "open_activity",
        label: "Open activity",
        safety: MenuActionSafety::Safe,
        safe_result: "opened activity route",
        effect: SafeEffect::OpenRoute("activity"),
    },
    RightClickMenuAction {
        id: "open_settings",
        label: "Open settings",
        safety: MenuActionSafety::Safe,
        safe_result: "opened settings route",
        effect: SafeEffect::OpenRoute("settings"),
    },
];

const CONVERSATION_ACTIONS: &[RightClickMenuAction] = &[
    RightClickMenuAction {
        id: "open_thread",
        label: "Open thread",
        safety: MenuActionSafety::Safe,
        safe_result: "focused conversation thread",
        effect: SafeEffect::FocusArea("conversation-thread"),
    },
    RightClickMenuAction {
        id: "view_safety",
        label: "View safety",
        safety: MenuActionSafety::Safe,
        safe_result: "opened safety panel",
        effect: SafeEffect::OpenPanel("safety"),
    },
];

const PROTECTED_TEXT_ACTIONS: &[RightClickMenuAction] = &[
    RightClickMenuAction {
        id: "copy_redacted_summary",
        label: "Copy redacted summary",
        safety: MenuActionSafety::Safe,
        safe_result: "copied redacted protected summary",
        effect: SafeEffect::CopyRedactedProtectedSummary,
    },
    RightClickMenuAction {
        id: "inspect_protection",
        label: "Inspect protection",
        safety: MenuActionSafety::Safe,
        safe_result: "opened protection panel",
        effect: SafeEffect::OpenPanel("protection"),
    },
];

const COMPOSER_ACTIONS: &[RightClickMenuAction] = &[
    RightClickMenuAction {
        id: "paste_safe_text",
        label: "Paste safe text",
        safety: MenuActionSafety::Safe,
        safe_result: "copied safe paste placeholder",
        effect: SafeEffect::CopyLiteral("safe paste placeholder"),
    },
    RightClickMenuAction {
        id: "toggle_quiet_mode",
        label: "Toggle quiet mode",
        safety: MenuActionSafety::Safe,
        safe_result: "toggled quiet mode",
        effect: SafeEffect::ToggleBoolean("quiet_mode"),
    },
];

const ATTACHMENT_ACTIONS: &[RightClickMenuAction] = &[RightClickMenuAction {
    id: "open_attachment_picker",
    label: "Open attachment picker",
    safety: MenuActionSafety::Safe,
    safe_result: "opened attachment picker panel",
    effect: SafeEffect::OpenPanel("attachment-picker"),
}];

const PRIVACY_SCAN_ACTIONS: &[RightClickMenuAction] = &[RightClickMenuAction {
    id: "view_finding_details",
    label: "View finding details",
    safety: MenuActionSafety::Safe,
    safe_result: "opened finding details panel",
    effect: SafeEffect::OpenPanel("finding-details"),
}];

const IDENTITY_ACTIONS: &[RightClickMenuAction] = &[RightClickMenuAction {
    id: "copy_public_invite",
    label: "Copy public invite",
    safety: MenuActionSafety::Safe,
    safe_result: "copied public invite",
    effect: SafeEffect::CopyLiteral("public invite copied"),
}];

pub const FIXED_TEST_SCREEN_TARGETS: &[RightClickTarget] = &[
    RightClickTarget {
        id: "workspace_background",
        name: "Workspace background",
        kind: RightClickTargetKind::ScreenArea,
        menu: Some(RightClickMenu {
            id: "workspace_context",
            title: "Workspace",
            actions: WORKSPACE_ACTIONS,
        }),
    },
    RightClickTarget {
        id: "left_navigation",
        name: "Left navigation",
        kind: RightClickTargetKind::ScreenArea,
        menu: Some(RightClickMenu {
            id: "navigation_context",
            title: "Navigation",
            actions: NAV_ACTIONS,
        }),
    },
    RightClickTarget {
        id: "conversation_row",
        name: "Conversation row",
        kind: RightClickTargetKind::Control,
        menu: Some(RightClickMenu {
            id: "conversation_context",
            title: "Conversation",
            actions: CONVERSATION_ACTIONS,
        }),
    },
    RightClickTarget {
        id: "protected_text_preview",
        name: "Protected text preview",
        kind: RightClickTargetKind::ProtectedText,
        menu: Some(RightClickMenu {
            id: "protected_text_context",
            title: "Protected text",
            actions: PROTECTED_TEXT_ACTIONS,
        }),
    },
    RightClickTarget {
        id: "message_composer",
        name: "Message composer",
        kind: RightClickTargetKind::Control,
        menu: Some(RightClickMenu {
            id: "composer_context",
            title: "Composer",
            actions: COMPOSER_ACTIONS,
        }),
    },
    RightClickTarget {
        id: "send_button",
        name: "Send button",
        kind: RightClickTargetKind::Control,
        menu: None,
    },
    RightClickTarget {
        id: "attachment_button",
        name: "Attachment button",
        kind: RightClickTargetKind::Control,
        menu: Some(RightClickMenu {
            id: "attachment_context",
            title: "Attachment",
            actions: ATTACHMENT_ACTIONS,
        }),
    },
    RightClickTarget {
        id: "privacy_scan_row",
        name: "Privacy scan row",
        kind: RightClickTargetKind::Control,
        menu: Some(RightClickMenu {
            id: "privacy_scan_context",
            title: "Privacy scan",
            actions: PRIVACY_SCAN_ACTIONS,
        }),
    },
    RightClickTarget {
        id: "identity_badge",
        name: "Identity badge",
        kind: RightClickTargetKind::Control,
        menu: Some(RightClickMenu {
            id: "identity_context",
            title: "Identity",
            actions: IDENTITY_ACTIONS,
        }),
    },
];

pub fn fixed_test_screen_targets() -> &'static [RightClickTarget] {
    FIXED_TEST_SCREEN_TARGETS
}

pub fn audit_fixed_test_screen_right_clicks() -> RightClickAuditReport {
    audit_right_click_targets(FIXED_TEST_SCREEN_TARGETS)
}

pub fn audit_right_click_targets(targets: &[RightClickTarget]) -> RightClickAuditReport {
    let mut state = RightClickAuditState::default();
    let mut records = Vec::with_capacity(targets.len());
    let mut areas_showing_menu = 0;
    let mut safe_actions_run = 0;
    let mut named_safe_actions_run = 0;
    let mut private_marks_exposed = 0;
    let mut unnamed_or_unsafe_menu_actions = 0;
    let mut failed_safe_results = 0;

    for target in targets {
        let Some(menu) = target.menu else {
            records.push(RightClickMenuRecord {
                target_id: target.id,
                target_name: target.name,
                menu_id: None,
                action_results: Vec::new(),
            });
            continue;
        };

        areas_showing_menu += 1;
        private_marks_exposed += private_mark_hits(target.id)
            + private_mark_hits(target.name)
            + private_mark_hits(menu.id)
            + private_mark_hits(menu.title);

        let mut action_results = Vec::with_capacity(menu.actions.len());
        for action in menu.actions {
            private_marks_exposed += private_mark_hits(action.id)
                + private_mark_hits(action.label)
                + private_mark_hits(action.safe_result);

            if action.safety != MenuActionSafety::Safe || action.safe_result.trim().is_empty() {
                unnamed_or_unsafe_menu_actions += 1;
                if action.safety == MenuActionSafety::Unsafe {
                    state.unsafe_actions_run += 1;
                }
                action_results.push(RightClickActionRun {
                    target_id: target.id,
                    menu_id: menu.id,
                    action_id: action.id,
                    result_name: action.safe_result,
                    did_the_thing: false,
                    exposed_private_marks: private_mark_hits(action.label)
                        + private_mark_hits(action.safe_result),
                    unsafe_run: action.safety == MenuActionSafety::Unsafe,
                });
                continue;
            }

            let before = state_snapshot(&state);
            apply_safe_effect(&mut state, action.effect);
            let did_the_thing = effect_changed_state(&before, &state, action.effect);
            safe_actions_run += 1;
            named_safe_actions_run += 1;
            if !did_the_thing {
                failed_safe_results += 1;
            }

            action_results.push(RightClickActionRun {
                target_id: target.id,
                menu_id: menu.id,
                action_id: action.id,
                result_name: action.safe_result,
                did_the_thing,
                exposed_private_marks: private_mark_hits(action.label)
                    + private_mark_hits(action.safe_result)
                    + state
                        .clipboard
                        .as_deref()
                        .map(private_mark_hits)
                        .unwrap_or(0),
                unsafe_run: false,
            });
        }

        records.push(RightClickMenuRecord {
            target_id: target.id,
            target_name: target.name,
            menu_id: Some(menu.id),
            action_results,
        });
    }

    RightClickAuditReport {
        records,
        areas_showing_menu,
        safe_actions_run,
        named_safe_actions_run,
        private_marks_exposed,
        unsafe_actions_run: state.unsafe_actions_run,
        unnamed_or_unsafe_menu_actions,
        failed_safe_results,
    }
}

fn apply_safe_effect(state: &mut RightClickAuditState, effect: SafeEffect) {
    match effect {
        SafeEffect::FocusArea(area) => state.focused_area = Some(area.to_owned()),
        SafeEffect::OpenPanel(panel) => state.open_panel = Some(panel.to_owned()),
        SafeEffect::OpenRoute(route) => state.route = Some(route.to_owned()),
        SafeEffect::CopyLiteral(value) => state.clipboard = Some(value.to_owned()),
        SafeEffect::CopyRedactedProtectedSummary => {
            state.clipboard = Some("redacted protected text summary".to_owned());
        }
        SafeEffect::ToggleBoolean("quiet_mode") => state.quiet_mode = !state.quiet_mode,
        SafeEffect::ToggleBoolean(_) => {}
    }
}

fn effect_changed_state(
    before: &(
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        bool,
    ),
    state: &RightClickAuditState,
    effect: SafeEffect,
) -> bool {
    match effect {
        SafeEffect::FocusArea(area) => {
            state.focused_area.as_deref() == Some(area) && before.0.as_deref() != Some(area)
        }
        SafeEffect::OpenPanel(panel) => {
            state.open_panel.as_deref() == Some(panel) && before.1.as_deref() != Some(panel)
        }
        SafeEffect::OpenRoute(route) => {
            state.route.as_deref() == Some(route) && before.2.as_deref() != Some(route)
        }
        SafeEffect::CopyLiteral(value) => {
            state.clipboard.as_deref() == Some(value) && before.3.as_deref() != Some(value)
        }
        SafeEffect::CopyRedactedProtectedSummary => {
            state.clipboard.as_deref() == Some("redacted protected text summary")
                && before.3.as_deref() != Some("redacted protected text summary")
        }
        SafeEffect::ToggleBoolean("quiet_mode") => state.quiet_mode != before.4,
        SafeEffect::ToggleBoolean(_) => false,
    }
}

fn state_snapshot(
    state: &RightClickAuditState,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
) {
    (
        state.focused_area.clone(),
        state.open_panel.clone(),
        state.route.clone(),
        state.clipboard.clone(),
        state.quiet_mode,
    )
}

fn private_mark_hits(value: &str) -> usize {
    PRIVATE_MARKS
        .iter()
        .filter(|private_mark| value.contains(**private_mark))
        .count()
}
