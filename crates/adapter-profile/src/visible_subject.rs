#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubjectProvider {
    Gmail,
    Outlook,
    Proton,
    Yahoo,
    Aol,
    Gmx,
    Maildotcom,
    Icloud,
    Tuta,
    OslMail,
}

impl SubjectProvider {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Gmail => "gmail",
            Self::Outlook => "outlook",
            Self::Proton => "proton",
            Self::Yahoo => "yahoo",
            Self::Aol => "aol",
            Self::Gmx => "gmx",
            Self::Maildotcom => "maildotcom",
            Self::Icloud => "icloud",
            Self::Tuta => "tuta",
            Self::OslMail => "osl-mail",
        }
    }
}

pub const EMAIL_SUBJECT_PROVIDERS: [SubjectProvider; 9] = [
    SubjectProvider::Gmail,
    SubjectProvider::Outlook,
    SubjectProvider::Proton,
    SubjectProvider::Yahoo,
    SubjectProvider::Aol,
    SubjectProvider::Gmx,
    SubjectProvider::Maildotcom,
    SubjectProvider::Icloud,
    SubjectProvider::Tuta,
];

pub const PRIVATE_SUBJECT_PROVIDERS: [SubjectProvider; 10] = [
    SubjectProvider::Gmail,
    SubjectProvider::Outlook,
    SubjectProvider::Proton,
    SubjectProvider::Yahoo,
    SubjectProvider::Aol,
    SubjectProvider::Gmx,
    SubjectProvider::Maildotcom,
    SubjectProvider::Icloud,
    SubjectProvider::Tuta,
    SubjectProvider::OslMail,
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SentSubject {
    pub provider: SubjectProvider,
    pub subject: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VisibleSubjectError {
    PrivateSubjectRefused { provider: SubjectProvider },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VisibleSubjectSendLedger {
    sent: Vec<SentSubject>,
}

impl VisibleSubjectSendLedger {
    pub fn sent_count(&self, provider: SubjectProvider) -> usize {
        self.sent
            .iter()
            .filter(|message| message.provider == provider)
            .count()
    }

    pub fn sent_subjects(&self, provider: SubjectProvider) -> Vec<&str> {
        self.sent
            .iter()
            .filter(|message| message.provider == provider)
            .map(|message| message.subject.as_str())
            .collect()
    }

    pub fn send_subject(
        &mut self,
        provider: SubjectProvider,
        subject: &str,
    ) -> Result<&str, VisibleSubjectError> {
        if visible_subject_is_private_fact(subject) {
            return Err(VisibleSubjectError::PrivateSubjectRefused { provider });
        }
        self.sent.push(SentSubject {
            provider,
            subject: subject.to_owned(),
        });
        Ok(self
            .sent
            .last()
            .expect("sent subject was just appended")
            .subject
            .as_str())
    }
}

pub fn visible_subject_is_private_fact(subject: &str) -> bool {
    let normalized = subject
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    normalized.contains("bank code")
        && normalized
            .chars()
            .filter(|character| character.is_ascii_digit())
            .count()
            >= 4
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO_SUBJECT: &str = "Hello";
    const PRIVATE_SUBJECT: &str = "MAPLE BANK CODE 4172";

    #[test]
    fn task_1299_private_facts_never_reach_visible_mail_subjects() {
        let mut ledger = VisibleSubjectSendLedger::default();
        let hello_provider_ids = EMAIL_SUBJECT_PROVIDERS
            .iter()
            .map(|provider| provider.id())
            .collect::<Vec<_>>();
        let private_provider_ids = PRIVATE_SUBJECT_PROVIDERS
            .iter()
            .map(|provider| provider.id())
            .collect::<Vec<_>>();

        println!(
            "TASK1299 hello_provider_count={} providers={}",
            EMAIL_SUBJECT_PROVIDERS.len(),
            hello_provider_ids.join(",")
        );
        println!(
            "TASK1299 private_refusal_provider_count={} providers={}",
            PRIVATE_SUBJECT_PROVIDERS.len(),
            private_provider_ids.join(",")
        );

        for provider in EMAIL_SUBJECT_PROVIDERS {
            let before = ledger.sent_count(provider);
            let sent_subject = ledger
                .send_subject(provider, HELLO_SUBJECT)
                .expect("safe Hello subject must be sent")
                .to_owned();
            let after = ledger.sent_count(provider);
            println!(
                "TASK1299_HELLO provider={} before={} after={} returned_subject=\"{}\" stored_subjects={:?}",
                provider.id(),
                before,
                after,
                sent_subject,
                ledger.sent_subjects(provider)
            );
            assert_eq!(before, 0);
            assert_eq!(after, 1);
            assert_eq!(sent_subject, HELLO_SUBJECT);
            assert_eq!(ledger.sent_subjects(provider), vec![HELLO_SUBJECT]);
        }

        for provider in PRIVATE_SUBJECT_PROVIDERS {
            let before = ledger.sent_count(provider);
            let refusal = ledger
                .send_subject(provider, PRIVATE_SUBJECT)
                .expect_err("private subject must be refused before send");
            let after = ledger.sent_count(provider);
            println!(
                "TASK1299_PRIVATE_REFUSAL provider={} subject=\"{}\" refused={:?} before={} after={} stored_subjects={:?}",
                provider.id(),
                PRIVATE_SUBJECT,
                refusal,
                before,
                after,
                ledger.sent_subjects(provider)
            );
            assert_eq!(
                refusal,
                VisibleSubjectError::PrivateSubjectRefused { provider }
            );
            assert_eq!(after, before);
            if EMAIL_SUBJECT_PROVIDERS.contains(&provider) {
                assert_eq!(before, 1);
                assert_eq!(ledger.sent_subjects(provider), vec![HELLO_SUBJECT]);
            } else {
                assert_eq!(before, 0);
                assert!(ledger.sent_subjects(provider).is_empty());
            }
        }

        let unchanged = EMAIL_SUBJECT_PROVIDERS
            .iter()
            .all(|provider| ledger.sent_subjects(*provider) == vec![HELLO_SUBJECT]);
        let counts_stayed_one = EMAIL_SUBJECT_PROVIDERS
            .iter()
            .all(|provider| ledger.sent_count(*provider) == 1);
        println!("TASK1299_HELLO_MESSAGES_STAY_EXACT={unchanged}");
        println!("TASK1299_HELLO_COUNTS_STAY_ONE={counts_stayed_one}");
        assert!(unchanged);
        assert!(counts_stayed_one);
    }
}
