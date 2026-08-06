use rusqlite::{params, Connection};

pub const SERVICE_BURN_SELECTION_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS service_burn_sender_messages (
    owner_osl_user_id TEXT NOT NULL,
    service_id TEXT NOT NULL,
    account_id TEXT NOT NULL,
    service_message_id TEXT NOT NULL,
    authored_by_self INTEGER NOT NULL CHECK (authored_by_self IN (0, 1)),
    created_at_unix_ms INTEGER,
    PRIMARY KEY (owner_osl_user_id, service_id, account_id, service_message_id)
);

CREATE INDEX IF NOT EXISTS idx_service_burn_sender_messages_service
    ON service_burn_sender_messages(owner_osl_user_id, service_id, authored_by_self);
"#;

pub const SELECT_SERVICE_BURN_SENDER_MESSAGES_SQL: &str = r#"
SELECT service_id, service_message_id
  FROM service_burn_sender_messages
 WHERE owner_osl_user_id = ?1
   AND service_id = ?2
   AND authored_by_self = 1
 ORDER BY created_at_unix_ms ASC, service_message_id ASC
"#;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceBurnSenderMessage {
    pub service_id: String,
    pub service_message_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceBurnSenderMessageRecord {
    pub owner_osl_user_id: String,
    pub service_id: String,
    pub account_id: String,
    pub service_message_id: String,
    pub authored_by_self: bool,
    pub created_at_unix_ms: Option<i64>,
}

pub fn install_service_burn_selection_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(SERVICE_BURN_SELECTION_SCHEMA)
}

pub fn record_service_burn_sender_message(
    conn: &Connection,
    record: &ServiceBurnSenderMessageRecord,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO service_burn_sender_messages \
            (owner_osl_user_id, service_id, account_id, service_message_id, \
             authored_by_self, created_at_unix_ms) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT(owner_osl_user_id, service_id, account_id, service_message_id) \
         DO UPDATE SET \
            authored_by_self = excluded.authored_by_self, \
            created_at_unix_ms = excluded.created_at_unix_ms",
        params![
            record.owner_osl_user_id,
            record.service_id,
            record.account_id,
            record.service_message_id,
            if record.authored_by_self { 1i64 } else { 0i64 },
            record.created_at_unix_ms,
        ],
    )?;
    Ok(())
}

pub fn select_service_burn_sender_messages(
    conn: &Connection,
    owner_osl_user_id: &str,
    service_id: &str,
) -> rusqlite::Result<Vec<ServiceBurnSenderMessage>> {
    let mut stmt = conn.prepare(SELECT_SERVICE_BURN_SENDER_MESSAGES_SQL)?;
    let rows = stmt.query_map(params![owner_osl_user_id, service_id], |row| {
        Ok(ServiceBurnSenderMessage {
            service_id: row.get(0)?,
            service_message_id: row.get(1)?,
        })
    })?;

    let mut selected = Vec::new();
    for row in rows {
        selected.push(row?);
    }
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER: &str = "owner-0533";
    const DISCORD_ACCOUNT: &str = "discord-acct-0533";
    const TELEGRAM_ACCOUNT: &str = "telegram-acct-0533";

    fn row(
        service_id: &str,
        account_id: &str,
        service_message_id: &str,
        authored_by_self: bool,
        created_at_unix_ms: i64,
    ) -> ServiceBurnSenderMessageRecord {
        ServiceBurnSenderMessageRecord {
            owner_osl_user_id: OWNER.to_owned(),
            service_id: service_id.to_owned(),
            account_id: account_id.to_owned(),
            service_message_id: service_message_id.to_owned(),
            authored_by_self,
            created_at_unix_ms: Some(created_at_unix_ms),
        }
    }

    #[test]
    fn direct_query_selects_only_sender_messages_for_named_service() {
        let conn = Connection::open_in_memory().unwrap();
        install_service_burn_selection_schema(&conn).unwrap();
        for record in [
            row("discord", DISCORD_ACCOUNT, "1180000000000000001", true, 1),
            row("discord", DISCORD_ACCOUNT, "1180000000000000002", true, 2),
            row("discord", DISCORD_ACCOUNT, "1180000000000000003", true, 3),
            row("discord", DISCORD_ACCOUNT, "1180000000000000004", true, 4),
            row("discord", DISCORD_ACCOUNT, "1180000000000000099", false, 5),
            row("telegram", TELEGRAM_ACCOUNT, "telegram-0533-1", true, 6),
            row("telegram", TELEGRAM_ACCOUNT, "telegram-0533-2", true, 7),
        ] {
            record_service_burn_sender_message(&conn, &record).unwrap();
        }

        let selected = select_service_burn_sender_messages(&conn, OWNER, "discord").unwrap();
        let discord_ids: Vec<_> = selected
            .iter()
            .filter(|message| message.service_id == "discord")
            .map(|message| message.service_message_id.as_str())
            .collect();
        let telegram_count = selected
            .iter()
            .filter(|message| message.service_id == "telegram")
            .count();

        println!(
            "direct_query=SELECT_SERVICE_BURN_SENDER_MESSAGES_SQL service=discord discord_count={} telegram_count={} discord_ids={discord_ids:?}",
            discord_ids.len(),
            telegram_count
        );

        assert_eq!(
            discord_ids,
            vec![
                "1180000000000000001",
                "1180000000000000002",
                "1180000000000000003",
                "1180000000000000004",
            ]
        );
        assert_eq!(telegram_count, 0);
        assert!(selected
            .iter()
            .all(|message| message.service_id == "discord"));
    }
}
