//! Deterministic, owner-controlled attempt ordering for hosted Scrub runs.
//!
//! A provider's enumeration order is not an acceptable deletion order: a run
//! can stop at any item, so sensitive categories must be attempted first and
//! the default order must prefer the oldest remaining content.

/// The minimum finding data needed to determine hosted Scrub attempt order.
pub trait HostedScrubOrderingItem {
    /// The owner-visible category assigned during review.
    fn category(&self) -> &str;

    /// When the item was created, as milliseconds since the Unix epoch.
    fn created_at_unix_ms(&self) -> u64;

    /// A stable provider item identifier used only to break otherwise equal
    /// priorities, rather than preserving provider enumeration order.
    fn stable_item_key(&self) -> &str;
}

/// Owner-declared category priorities for a hosted Scrub run.
///
/// Categories appear in descending importance: every item in the first
/// category is attempted before the second, and all unlisted categories follow
/// in oldest-first order. Within a category, oldest content is attempted first.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HostedScrubOrdering {
    priority_categories: Vec<String>,
}

impl HostedScrubOrdering {
    /// Builds an ordering from the categories the owner flagged as sensitive.
    pub fn sensitive_first(priority_categories: Vec<String>) -> Self {
        Self {
            priority_categories,
        }
    }

    /// Returns findings in the order they must be attempted.
    pub fn order_for_attempt<T>(&self, mut findings: Vec<T>) -> Vec<T>
    where
        T: HostedScrubOrderingItem,
    {
        findings.sort_by(|left, right| {
            self.priority_for(left.category())
                .cmp(&self.priority_for(right.category()))
                .then_with(|| left.created_at_unix_ms().cmp(&right.created_at_unix_ms()))
                .then_with(|| left.stable_item_key().cmp(right.stable_item_key()))
        });
        findings
    }

    fn priority_for(&self, category: &str) -> usize {
        self.priority_categories
            .iter()
            .position(|priority| priority == category)
            .unwrap_or(self.priority_categories.len())
    }
}

#[cfg(test)]
mod tests {
    use super::{HostedScrubOrdering, HostedScrubOrderingItem};

    #[derive(Debug)]
    struct Finding {
        id: &'static str,
        category: &'static str,
        created_at_unix_ms: u64,
    }

    impl HostedScrubOrderingItem for Finding {
        fn category(&self) -> &str {
            self.category
        }

        fn created_at_unix_ms(&self) -> u64 {
            self.created_at_unix_ms
        }

        fn stable_item_key(&self) -> &str {
            self.id
        }
    }

    #[test]
    fn scr_h6_attempts_owner_priority_then_oldest_never_provider_order() {
        let provider_returned = vec![
            Finding {
                id: "recent-post",
                category: "posts",
                created_at_unix_ms: 400,
            },
            Finding {
                id: "old-message",
                category: "messages",
                created_at_unix_ms: 100,
            },
            Finding {
                id: "new-sensitive",
                category: "credentials",
                created_at_unix_ms: 300,
            },
            Finding {
                id: "old-sensitive",
                category: "credentials",
                created_at_unix_ms: 200,
            },
            Finding {
                id: "middle-post",
                category: "posts",
                created_at_unix_ms: 250,
            },
        ];

        let ordered = HostedScrubOrdering::sensitive_first(vec!["credentials".to_owned()])
            .order_for_attempt(provider_returned);

        assert_eq!(
            ordered.iter().map(|finding| finding.id).collect::<Vec<_>>(),
            vec![
                "old-sensitive",
                "new-sensitive",
                "old-message",
                "middle-post",
                "recent-post"
            ]
        );
    }

    #[test]
    fn default_order_is_oldest_first() {
        let provider_returned = vec![
            Finding {
                id: "newest",
                category: "posts",
                created_at_unix_ms: 300,
            },
            Finding {
                id: "oldest",
                category: "messages",
                created_at_unix_ms: 100,
            },
            Finding {
                id: "middle",
                category: "posts",
                created_at_unix_ms: 200,
            },
        ];

        let ordered = HostedScrubOrdering::default().order_for_attempt(provider_returned);

        assert_eq!(
            ordered.iter().map(|finding| finding.id).collect::<Vec<_>>(),
            vec!["oldest", "middle", "newest"]
        );
    }
}
