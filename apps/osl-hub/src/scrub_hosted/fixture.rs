//! Repeatable large-message fixtures for hosted Scrub runner checks.

use crate::native_discord_adapter::guided_deletion::{
    DeletionScan, RowShape, ScannedRow, WalkCompleteness,
};

pub const LARGE_SCRUB_MESSAGE_COUNT: usize = 100_000;
pub const LARGE_SCRUB_MARKED_MATCH_COUNT: usize = 10_000;
pub const LARGE_SCRUB_MATCH_INTERVAL: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrubFixtureMessage {
    /// One-based position in the account's ordered message history.
    pub position: usize,
    pub message_id: String,
    pub marked_match: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrubFixtureCreationReport {
    pub messages_before_creation: usize,
    pub messages_after_creation: usize,
    pub marked_matches_after_creation: usize,
    pub first_marked_position: Option<usize>,
    pub last_marked_position: Option<usize>,
    pub marked_matches_are_every_tenth_position: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrubFixtureError {
    ExistingMessages,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovedScrubAccountFixture {
    account_id: String,
    owner_osl_user_id: String,
    messages: Vec<ScrubFixtureMessage>,
}

impl ApprovedScrubAccountFixture {
    pub fn fresh(account_id: impl Into<String>, owner_osl_user_id: impl Into<String>) -> Self {
        Self {
            account_id: account_id.into(),
            owner_osl_user_id: owner_osl_user_id.into(),
            messages: Vec::new(),
        }
    }

    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    pub fn marked_match_count(&self) -> usize {
        self.messages
            .iter()
            .filter(|message| message.marked_match)
            .count()
    }

    pub fn messages(&self) -> &[ScrubFixtureMessage] {
        &self.messages
    }

    pub fn populate_one_hundred_thousand_ordered_messages(
        &mut self,
    ) -> Result<ScrubFixtureCreationReport, ScrubFixtureError> {
        let messages_before_creation = self.message_count();
        if messages_before_creation != 0 {
            return Err(ScrubFixtureError::ExistingMessages);
        }

        self.messages.reserve_exact(LARGE_SCRUB_MESSAGE_COUNT);
        for position in 1..=LARGE_SCRUB_MESSAGE_COUNT {
            self.messages.push(ScrubFixtureMessage {
                position,
                message_id: format!("scrub-message-{position:06}"),
                marked_match: position % LARGE_SCRUB_MATCH_INTERVAL == 0,
            });
        }

        Ok(ScrubFixtureCreationReport {
            messages_before_creation,
            messages_after_creation: self.message_count(),
            marked_matches_after_creation: self.marked_match_count(),
            first_marked_position: self
                .messages
                .iter()
                .find(|message| message.marked_match)
                .map(|message| message.position),
            last_marked_position: self
                .messages
                .iter()
                .rfind(|message| message.marked_match)
                .map(|message| message.position),
            marked_matches_are_every_tenth_position: self.messages.iter().all(|message| {
                message.marked_match == (message.position % LARGE_SCRUB_MATCH_INTERVAL == 0)
            }),
        })
    }

    pub fn messages_are_strictly_ordered(&self) -> bool {
        self.messages
            .iter()
            .enumerate()
            .all(|(index, message)| message.position == index + 1)
    }

    pub fn marked_match_positions(&self) -> impl Iterator<Item = usize> + '_ {
        self.messages
            .iter()
            .filter(|message| message.marked_match)
            .map(|message| message.position)
    }

    pub fn to_guided_deletion_scan(&self) -> DeletionScan {
        let candidates = self
            .messages
            .iter()
            .filter(|message| message.marked_match)
            .map(|message| ScannedRow {
                scan_ordinal: message.position,
                shape_ordinal: message.position,
                shape: RowShape {
                    height_px: 44,
                    children: 0,
                },
                text_len: message.message_id.len(),
                authored_by_operator: true,
            })
            .collect();

        DeletionScan {
            scope_binding_hash: "3".repeat(64),
            generation: 3687,
            rows_seen: self.messages.len(),
            rows_unreadable: 0,
            walk: WalkCompleteness::Complete,
            candidates,
        }
    }

    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    pub fn owner_osl_user_id(&self) -> &str {
        &self.owner_osl_user_id
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ApprovedScrubAccountFixture, LARGE_SCRUB_MARKED_MATCH_COUNT, LARGE_SCRUB_MESSAGE_COUNT,
    };

    #[test]
    fn large_scrub_fixture_creates_ordered_messages_and_every_tenth_match() {
        let mut fixture = ApprovedScrubAccountFixture::fresh("approved-account-3687", "owner-3687");
        let report = fixture
            .populate_one_hundred_thousand_ordered_messages()
            .expect("fresh approved fixture can be populated");

        assert_eq!(report.messages_before_creation, 0);
        assert_eq!(report.messages_after_creation, LARGE_SCRUB_MESSAGE_COUNT);
        assert_eq!(
            report.marked_matches_after_creation,
            LARGE_SCRUB_MARKED_MATCH_COUNT
        );
        assert_eq!(report.first_marked_position, Some(10));
        assert_eq!(report.last_marked_position, Some(100_000));
        assert!(report.marked_matches_are_every_tenth_position);
        assert!(fixture.messages_are_strictly_ordered());
    }
}
