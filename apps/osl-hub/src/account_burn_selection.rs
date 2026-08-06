use rusqlite::{params, Connection};

pub const SELECT_ACCOUNT_BURN_SENDER_MESSAGES_SQL: &str = r#"
SELECT owner_osl_user_id, service_id, account_id, service_message_id
FROM provider_sender_messages
WHERE owner_osl_user_id = ?1
  AND account_id = ?2
  AND authored_by_self = 1
ORDER BY service_id ASC, service_message_id ASC
"#;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AccountBurnSenderMessageRecord {
    pub owner_osl_user_id: String,
    pub service_id: String,
    pub account_id: String,
    pub service_message_id: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AccountBurnSelection {
    pub current_account_id: String,
    pub selected_message_ids: Vec<String>,
}

impl AccountBurnSelection {
    pub fn empty(current_account_id: impl Into<String>) -> Self {
        Self {
            current_account_id: current_account_id.into(),
            selected_message_ids: Vec::new(),
        }
    }

    pub fn selected_total(&self) -> usize {
        self.selected_message_ids.len()
    }
}

pub fn select_account_burn_sender_messages(
    conn: &Connection,
    owner_osl_user_id: &str,
    account_id: &str,
) -> rusqlite::Result<Vec<AccountBurnSenderMessageRecord>> {
    let mut statement = conn.prepare(SELECT_ACCOUNT_BURN_SENDER_MESSAGES_SQL)?;
    let rows = statement.query_map(params![owner_osl_user_id, account_id], |row| {
        Ok(AccountBurnSenderMessageRecord {
            owner_osl_user_id: row.get(0)?,
            service_id: row.get(1)?,
            account_id: row.get(2)?,
            service_message_id: row.get(3)?,
        })
    })?;
    rows.collect()
}

pub fn select_account_burn_selection(
    conn: &Connection,
    owner_osl_user_id: &str,
    account_id: &str,
) -> rusqlite::Result<AccountBurnSelection> {
    let records = select_account_burn_sender_messages(conn, owner_osl_user_id, account_id)?;
    Ok(AccountBurnSelection {
        current_account_id: account_id.to_owned(),
        selected_message_ids: records
            .into_iter()
            .map(|record| record.service_message_id)
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(
        conn: &Connection,
        owner: &str,
        service: &str,
        account: &str,
        message_id: &str,
        authored_by_self: bool,
    ) {
        conn.execute(
            "INSERT INTO provider_sender_messages (
                owner_osl_user_id,
                service_id,
                account_id,
                service_message_id,
                authored_by_self
            ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![owner, service, account, message_id, authored_by_self],
        )
        .expect("seed provider sender message");
    }

    #[test]
    fn direct_query_selects_current_account_sender_messages_across_services() {
        let conn = Connection::open_in_memory().expect("open in-memory direct query db");
        conn.execute_batch(
            "CREATE TABLE provider_sender_messages (
                owner_osl_user_id TEXT NOT NULL,
                service_id TEXT NOT NULL,
                account_id TEXT NOT NULL,
                service_message_id TEXT NOT NULL,
                authored_by_self INTEGER NOT NULL
            );",
        )
        .expect("create provider sender message table");

        let owner = "identity-current";
        for (service, message_id) in [
            ("discord", "current-discord-1"),
            ("discord", "current-discord-2"),
            ("telegram", "current-telegram-1"),
            ("telegram", "current-telegram-2"),
            ("signal", "current-signal-1"),
            ("whatsapp", "current-whatsapp-1"),
        ] {
            seed(&conn, owner, service, "current-account", message_id, true);
        }
        for (service, message_id) in [
            ("discord", "other-discord-1"),
            ("telegram", "other-telegram-1"),
            ("signal", "other-signal-1"),
        ] {
            seed(&conn, owner, service, "other-account", message_id, true);
        }
        seed(
            &conn,
            owner,
            "discord",
            "current-account",
            "current-received-1",
            false,
        );
        seed(
            &conn,
            "identity-other",
            "discord",
            "current-account",
            "other-owner-current-1",
            true,
        );

        let selected =
            select_account_burn_sender_messages(&conn, owner, "current-account").unwrap();
        let selected_ids = selected
            .iter()
            .map(|record| record.service_message_id.clone())
            .collect::<Vec<_>>();
        let current_account_records = selected
            .iter()
            .filter(|record| record.account_id == "current-account")
            .count();
        let other_account_records = selected
            .iter()
            .filter(|record| record.account_id == "other-account")
            .count();

        assert_eq!(selected.len(), 6);
        assert_eq!(current_account_records, 6);
        assert_eq!(other_account_records, 0);
        assert!(selected
            .iter()
            .all(|record| record.owner_osl_user_id == owner));
        assert!(selected
            .iter()
            .all(|record| record.account_id == "current-account"));
        assert!(!selected_ids.iter().any(|id| id.contains("received")));
        assert!(!selected_ids.iter().any(|id| id.contains("other")));
        assert_eq!(
            selected_ids,
            vec![
                "current-discord-1",
                "current-discord-2",
                "current-signal-1",
                "current-telegram-1",
                "current-telegram-2",
                "current-whatsapp-1",
            ]
        );

        println!(
            "TASK0537 direct_query=SELECT_ACCOUNT_BURN_SENDER_MESSAGES_SQL account=current-account current_account_records={} other_account_records={} ids={:?}",
            current_account_records, other_account_records, selected_ids
        );
    }

    #[test]
    fn account_burn_action_result_reports_current_account_id_and_selected_total() {
        let conn = Connection::open_in_memory().expect("open in-memory account burn db");
        conn.execute_batch(
            "CREATE TABLE provider_sender_messages (
                owner_osl_user_id TEXT NOT NULL,
                service_id TEXT NOT NULL,
                account_id TEXT NOT NULL,
                service_message_id TEXT NOT NULL,
                authored_by_self INTEGER NOT NULL
            );",
        )
        .expect("create provider sender message table");

        let owner = "identity-current";
        for (service, message_id) in [
            ("discord", "current-discord-1"),
            ("discord", "current-discord-2"),
            ("telegram", "current-telegram-1"),
            ("telegram", "current-telegram-2"),
            ("signal", "current-signal-1"),
            ("whatsapp", "current-whatsapp-1"),
        ] {
            seed(&conn, owner, service, "current-account", message_id, true);
        }
        for (service, message_id) in [
            ("discord", "other-discord-1"),
            ("telegram", "other-telegram-1"),
            ("signal", "other-signal-1"),
        ] {
            seed(&conn, owner, service, "other-account", message_id, true);
        }
        seed(
            &conn,
            owner,
            "discord",
            "current-account",
            "current-received-1",
            false,
        );

        let selection = select_account_burn_selection(&conn, owner, "current-account").unwrap();

        assert_eq!(selection.current_account_id, "current-account");
        assert_eq!(selection.selected_total(), 6);
        assert_eq!(
            selection.selected_message_ids,
            vec![
                "current-discord-1",
                "current-discord-2",
                "current-signal-1",
                "current-telegram-1",
                "current-telegram-2",
                "current-whatsapp-1",
            ]
        );
        assert!(!selection
            .selected_message_ids
            .iter()
            .any(|id| id.contains("other") || id.contains("received")));

        println!(
            "TASK0538 result current_account_id={} selected_total={} selected_message_ids={:?}",
            selection.current_account_id,
            selection.selected_total(),
            selection.selected_message_ids
        );
    }
}
