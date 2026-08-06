use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BadMessageRuleChoice {
    pub id: &'static str,
    pub label: &'static str,
    pub explanation: &'static str,
}

const BAD_MESSAGE_RULE_CHOICES: [BadMessageRuleChoice; 6] = [
    BadMessageRuleChoice {
        id: "passwords_and_codes",
        label: "Passwords and codes",
        explanation:
            "Login passwords, one-time codes, recovery phrases, PINs, or invite codes that could let someone into an account.",
    },
    BadMessageRuleChoice {
        id: "personal_details",
        label: "Personal details",
        explanation:
            "Names, addresses, phone numbers, locations, IDs, health details, or other facts that identify a person.",
    },
    BadMessageRuleChoice {
        id: "money_details",
        label: "Money details",
        explanation:
            "Card numbers, bank details, invoices, tax details, account balances, or payment information.",
    },
    BadMessageRuleChoice {
        id: "private_words",
        label: "Private words",
        explanation:
            "Words or names you add yourself, like a project name, nickname, or phrase you do not want left in messages.",
    },
    BadMessageRuleChoice {
        id: "private_pictures",
        label: "Private pictures",
        explanation:
            "Photos, screenshots, scans, or attachments that may show people, documents, rooms, screens, or other private things.",
    },
    BadMessageRuleChoice {
        id: "everything_above",
        label: "Everything above",
        explanation: "Use all of these rules together.",
    },
];

pub fn list_bad_message_rules() -> Vec<BadMessageRuleChoice> {
    BAD_MESSAGE_RULE_CHOICES.to_vec()
}

#[cfg(test)]
mod tests {
    use super::list_bad_message_rules;

    #[test]
    fn bad_message_rules_command_lists_all_six_choices_with_plain_explanations() {
        let rules = list_bad_message_rules();
        println!("list_bad_message_rules -> {} choices", rules.len());
        for rule in &rules {
            println!("{}: {}", rule.label, rule.explanation);
        }

        assert_eq!(rules.len(), 6);
        assert_eq!(rules[0].label, "Passwords and codes");
        assert_eq!(
            rules[0].explanation,
            "Login passwords, one-time codes, recovery phrases, PINs, or invite codes that could let someone into an account."
        );
        assert_eq!(rules[1].label, "Personal details");
        assert_eq!(
            rules[1].explanation,
            "Names, addresses, phone numbers, locations, IDs, health details, or other facts that identify a person."
        );
        assert_eq!(rules[2].label, "Money details");
        assert_eq!(
            rules[2].explanation,
            "Card numbers, bank details, invoices, tax details, account balances, or payment information."
        );
        assert_eq!(rules[3].label, "Private words");
        assert_eq!(
            rules[3].explanation,
            "Words or names you add yourself, like a project name, nickname, or phrase you do not want left in messages."
        );
        assert_eq!(rules[4].label, "Private pictures");
        assert_eq!(
            rules[4].explanation,
            "Photos, screenshots, scans, or attachments that may show people, documents, rooms, screens, or other private things."
        );
        assert_eq!(rules[5].label, "Everything above");
        assert_eq!(rules[5].explanation, "Use all of these rules together.");

        assert!(rules.iter().all(|rule| !rule.id.is_empty()));
        assert!(rules.iter().all(|rule| !rule.explanation.is_empty()));
    }
}
