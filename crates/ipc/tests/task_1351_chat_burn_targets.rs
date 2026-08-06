use ipc::commands::{
    cmd_osl_select_chat_burn_targets, ChatBurnTargetScopeInput, ChatBurnTargetScopeKind,
};
use std::collections::HashSet;

#[test]
fn direct_command_returns_separate_target_list_for_each_of_seven_scopes() {
    let result = cmd_osl_select_chat_burn_targets(vec![
        ChatBurnTargetScopeInput {
            scope_kind: ChatBurnTargetScopeKind::DirectMessage,
            scope_id: "dm-task-1351".to_owned(),
            service_id: None,
            account_id: None,
            server_id: None,
            channel_id: None,
            thread_id: None,
            parent_message_id: None,
        },
        ChatBurnTargetScopeInput {
            scope_kind: ChatBurnTargetScopeKind::Group,
            scope_id: "group-task-1351".to_owned(),
            service_id: None,
            account_id: None,
            server_id: None,
            channel_id: None,
            thread_id: None,
            parent_message_id: None,
        },
        ChatBurnTargetScopeInput {
            scope_kind: ChatBurnTargetScopeKind::Channel,
            scope_id: "channel-task-1351".to_owned(),
            service_id: None,
            account_id: None,
            server_id: Some("server-task-1351".to_owned()),
            channel_id: Some("channel-task-1351".to_owned()),
            thread_id: None,
            parent_message_id: None,
        },
        ChatBurnTargetScopeInput {
            scope_kind: ChatBurnTargetScopeKind::Server,
            scope_id: "server-task-1351".to_owned(),
            service_id: None,
            account_id: None,
            server_id: Some("server-task-1351".to_owned()),
            channel_id: None,
            thread_id: None,
            parent_message_id: None,
        },
        ChatBurnTargetScopeInput {
            scope_kind: ChatBurnTargetScopeKind::Thread,
            scope_id: "thread-task-1351".to_owned(),
            service_id: None,
            account_id: None,
            server_id: Some("server-task-1351".to_owned()),
            channel_id: Some("channel-task-1351".to_owned()),
            thread_id: Some("thread-task-1351".to_owned()),
            parent_message_id: Some("parent-message-task-1351".to_owned()),
        },
        ChatBurnTargetScopeInput {
            scope_kind: ChatBurnTargetScopeKind::Service,
            scope_id: "service-task-1351".to_owned(),
            service_id: Some("osl-chat".to_owned()),
            account_id: None,
            server_id: None,
            channel_id: None,
            thread_id: None,
            parent_message_id: None,
        },
        ChatBurnTargetScopeInput {
            scope_kind: ChatBurnTargetScopeKind::Account,
            scope_id: "account-task-1351".to_owned(),
            service_id: Some("osl-chat".to_owned()),
            account_id: Some("account-task-1351".to_owned()),
            server_id: None,
            channel_id: None,
            thread_id: None,
            parent_message_id: None,
        },
    ])
    .expect("direct command returns chat burn targets");

    let scope_names = result
        .iter()
        .map(|list| list.scope_kind.as_str())
        .collect::<Vec<_>>();
    let unique_scope_names = scope_names.iter().copied().collect::<HashSet<_>>();

    println!("TASK1351_DIRECT_COMMAND=cmd_osl_select_chat_burn_targets");
    println!("TASK1351_SCOPE_LIST_COUNT={}", result.len());
    println!("TASK1351_SCOPE_NAMES={}", scope_names.join(","));
    for list in &result {
        let targets = list
            .targets
            .iter()
            .map(|target| format!("{}:{}", target.target_kind, target.target_id))
            .collect::<Vec<_>>();
        println!(
            "TASK1351_TARGET_LIST_{}={}",
            list.scope_kind,
            targets.join("|")
        );
        println!(
            "TASK1351_TARGET_COUNT_{}={}",
            list.scope_kind,
            list.targets.len()
        );
    }

    assert_eq!(result.len(), 7);
    assert_eq!(
        scope_names,
        vec![
            "direct_message",
            "group",
            "channel",
            "server",
            "thread",
            "service",
            "account"
        ]
    );
    assert_eq!(unique_scope_names.len(), 7);
    assert!(result.iter().all(|list| !list.targets.is_empty()));
    assert_eq!(
        result
            .iter()
            .find(|list| list.scope_kind == "channel")
            .expect("channel target list")
            .targets[0]
            .target_id,
        "server-task-1351:channel-task-1351"
    );
    assert_eq!(
        result
            .iter()
            .find(|list| list.scope_kind == "thread")
            .expect("thread target list")
            .targets[2]
            .target_id,
        "parent-message-task-1351"
    );
}
