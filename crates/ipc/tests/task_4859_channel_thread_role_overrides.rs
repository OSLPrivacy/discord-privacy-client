use ipc::commands::{
    cmd_osl_create_thread, cmd_osl_read_channel_role_permission_overrides,
    cmd_osl_read_thread_role_permission_overrides, cmd_osl_set_channel_role_permission_overrides,
    cmd_osl_set_open_channel_members, cmd_osl_set_thread_role_permission_overrides,
    cmd_osl_write_server_member_list,
};
use ipc::server_membership::{
    RolePermissionOverrideCell, RolePermissionOverridePermission, RolePermissionOverrideRole,
    RolePermissionOverrideState, RolePermissionOverrideTable,
};
use ipc::state::AppState;

const SERVER_ID: &str = "server-4859";
const CHANNEL_ID: &str = "channel-4859";
const THREAD_ID: &str = "thread-4859";
const OWNER: &str = "Ari Owner";

fn channel_table_cells() -> Vec<RolePermissionOverrideCell> {
    matrix_cells(|index, _role, _permission| match index % 3 {
        0 => RolePermissionOverrideState::Allow,
        1 => RolePermissionOverrideState::Deny,
        _ => RolePermissionOverrideState::Inherit,
    })
}

fn valid_thread_table_cells(
    channel: &RolePermissionOverrideTable,
) -> Vec<RolePermissionOverrideCell> {
    channel
        .cells
        .iter()
        .map(|cell| RolePermissionOverrideCell {
            role: cell.role,
            permission: cell.permission,
            state: match cell.state {
                RolePermissionOverrideState::Allow => RolePermissionOverrideState::Deny,
                RolePermissionOverrideState::Deny => RolePermissionOverrideState::Deny,
                RolePermissionOverrideState::Inherit => RolePermissionOverrideState::Inherit,
            },
        })
        .collect()
}

fn invalid_thread_table_cells(
    channel: &RolePermissionOverrideTable,
) -> Vec<RolePermissionOverrideCell> {
    let mut cells = valid_thread_table_cells(channel);
    let denied = channel
        .cells
        .iter()
        .position(|cell| cell.state == RolePermissionOverrideState::Deny)
        .expect("fixture contains a denied parent cell");
    cells[denied].state = RolePermissionOverrideState::Allow;
    cells
}

fn matrix_cells(
    state_for: impl Fn(
        usize,
        RolePermissionOverrideRole,
        RolePermissionOverridePermission,
    ) -> RolePermissionOverrideState,
) -> Vec<RolePermissionOverrideCell> {
    RolePermissionOverrideRole::ALL
        .into_iter()
        .flat_map(|role| {
            RolePermissionOverridePermission::ALL
                .into_iter()
                .map(move |permission| (role, permission))
        })
        .enumerate()
        .map(|(index, (role, permission))| RolePermissionOverrideCell {
            role,
            permission,
            state: state_for(index, role, permission),
        })
        .collect()
}

fn state_count(table: &RolePermissionOverrideTable, state: RolePermissionOverrideState) -> usize {
    table
        .cells
        .iter()
        .filter(|cell| cell.state == state)
        .count()
}

#[test]
fn task4859_channel_and_thread_role_permission_overrides_are_tri_state() {
    let state = AppState::new();
    cmd_osl_write_server_member_list(
        &state,
        SERVER_ID.to_owned(),
        OWNER.to_owned(),
        "2026-08-06T09:00:00Z".to_owned(),
    )
    .expect("server owner row can be written");
    cmd_osl_set_open_channel_members(&state, SERVER_ID.to_owned(), CHANNEL_ID.to_owned())
        .expect("channel can be created open");
    cmd_osl_create_thread(
        &state,
        SERVER_ID.to_owned(),
        CHANNEL_ID.to_owned(),
        THREAD_ID.to_owned(),
    )
    .expect("thread can be created");

    let saved_channel = cmd_osl_set_channel_role_permission_overrides(
        &state,
        SERVER_ID.to_owned(),
        CHANNEL_ID.to_owned(),
        channel_table_cells(),
    )
    .expect("channel override table saves");
    let read_channel = cmd_osl_read_channel_role_permission_overrides(
        &state,
        SERVER_ID.to_owned(),
        CHANNEL_ID.to_owned(),
    )
    .expect("channel override table reads back");

    let saved_thread = cmd_osl_set_thread_role_permission_overrides(
        &state,
        SERVER_ID.to_owned(),
        CHANNEL_ID.to_owned(),
        THREAD_ID.to_owned(),
        valid_thread_table_cells(&read_channel),
    )
    .expect("thread override table saves when it is not more open than channel");
    let read_thread = cmd_osl_read_thread_role_permission_overrides(
        &state,
        SERVER_ID.to_owned(),
        CHANNEL_ID.to_owned(),
        THREAD_ID.to_owned(),
    )
    .expect("thread override table reads back");

    let refused = cmd_osl_set_thread_role_permission_overrides(
        &state,
        SERVER_ID.to_owned(),
        CHANNEL_ID.to_owned(),
        THREAD_ID.to_owned(),
        invalid_thread_table_cells(&read_channel),
    )
    .expect_err("thread cannot allow a channel-denied permission");

    let channel_allow = state_count(&read_channel, RolePermissionOverrideState::Allow);
    let channel_deny = state_count(&read_channel, RolePermissionOverrideState::Deny);
    let channel_inherit = state_count(&read_channel, RolePermissionOverrideState::Inherit);
    let thread_allow = state_count(&read_thread, RolePermissionOverrideState::Allow);
    let thread_deny = state_count(&read_thread, RolePermissionOverrideState::Deny);
    let thread_inherit = state_count(&read_thread, RolePermissionOverrideState::Inherit);

    println!(
        "TASK4859 roles={} permissions={} expected_cells={}",
        RolePermissionOverrideRole::ALL.len(),
        RolePermissionOverridePermission::ALL.len(),
        RolePermissionOverrideRole::ALL.len() * RolePermissionOverridePermission::ALL.len()
    );
    println!(
        "TASK4859 channel saved_cell_count={} read_cell_count={} allow={} deny={} inherit={} explicit_grant_row_count={}",
        saved_channel.saved_cell_count,
        read_channel.cells.len(),
        channel_allow,
        channel_deny,
        channel_inherit,
        read_channel.explicit_grant_row_count
    );
    println!(
        "TASK4859 thread saved_cell_count={} read_cell_count={} allow={} deny={} inherit={} explicit_grant_row_count={}",
        saved_thread.saved_cell_count,
        read_thread.cells.len(),
        thread_allow,
        thread_deny,
        thread_inherit,
        read_thread.explicit_grant_row_count
    );
    println!("TASK4859 refusal={refused}");

    assert_eq!(RolePermissionOverrideRole::ALL.len(), 3);
    assert_eq!(RolePermissionOverridePermission::ALL.len(), 40);
    assert_eq!(saved_channel.saved_cell_count, 120);
    assert_eq!(read_channel.cells.len(), 120);
    assert_eq!((channel_allow, channel_deny, channel_inherit), (40, 40, 40));
    assert_eq!(read_channel.explicit_grant_row_count, 80);
    assert_eq!(read_channel.explicit_grant_row_count + channel_inherit, 120);
    assert_eq!(saved_thread.saved_cell_count, 120);
    assert_eq!(read_thread.cells.len(), 120);
    assert_eq!((thread_allow, thread_deny, thread_inherit), (0, 80, 40));
    assert_eq!(read_thread.explicit_grant_row_count, 80);
    assert_eq!(refused, "thread cannot be more open than channel");
}
