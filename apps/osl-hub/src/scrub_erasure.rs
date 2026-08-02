//! Local composition of statutory erasure requests.
//!
//! This module deliberately has no transport dependency. It prepares plain text
//! for the user to send from their own mailbox; it neither identifies the user
//! to OSL nor transmits request data anywhere.

/// A category of personal data the user asks a provider to erase.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ErasureDataCategory {
    AccountProfile,
    PostsAndMessages,
    CommentsAndReactions,
    DirectMessages,
    PhotosAndMedia,
    UsageAndDeviceData,
}

impl ErasureDataCategory {
    const fn label(self) -> &'static str {
        match self {
            Self::AccountProfile => "account profile data",
            Self::PostsAndMessages => "posts and messages",
            Self::CommentsAndReactions => "comments and reactions",
            Self::DirectMessages => "direct messages",
            Self::PhotosAndMedia => "photos and other media",
            Self::UsageAndDeviceData => "usage and device data",
        }
    }
}

/// The provider-specific details needed to write a request.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ErasureRequestInput {
    /// The service that holds the data, such as "Example Social".
    pub provider_name: String,
    /// An identifier the provider can use to locate the user's account.
    pub provider_account_identifier: String,
    pub data_categories: Vec<ErasureDataCategory>,
}

/// Plain-text request for the user to review and send themselves.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ComposedErasureRequest {
    pub subject: String,
    pub body: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ErasureCompositionError {
    MissingProviderName,
    MissingProviderAccountIdentifier,
    MissingDataCategories,
}

/// Compose a complete GDPR Art. 17 / CCPA erasure request entirely in memory.
///
/// The returned text is not a deletion receipt: the user must send it and a
/// later independent re-scan must verify any claimed removal.
pub fn compose_erasure_request(
    input: &ErasureRequestInput,
) -> Result<ComposedErasureRequest, ErasureCompositionError> {
    if input.provider_name.trim().is_empty() {
        return Err(ErasureCompositionError::MissingProviderName);
    }
    if input.provider_account_identifier.trim().is_empty() {
        return Err(ErasureCompositionError::MissingProviderAccountIdentifier);
    }
    if input.data_categories.is_empty() {
        return Err(ErasureCompositionError::MissingDataCategories);
    }

    let categories = input
        .data_categories
        .iter()
        .map(|category| format!("- {}", category.label()))
        .collect::<Vec<_>>()
        .join("\n");
    let provider_name = input.provider_name.trim();
    let provider_account_identifier = input.provider_account_identifier.trim();

    Ok(ComposedErasureRequest {
        subject: format!("Request to erase my personal data — {provider_name}"),
        body: format!(
            "Hello {provider_name} privacy team,\n\n\
I request that you erase my personal data associated with the account identified below. \
To the extent applicable, this is a request under GDPR Article 17 and the CCPA/CPRA right to delete.\n\n\
Account identifier: {provider_account_identifier}\n\n\
Please erase the following data categories:\n{categories}\n\n\
Please confirm what you have erased, or explain any data you must retain and the legal basis for doing so.\n\n\
Thank you."
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scr_g1_composes_a_complete_local_request_with_named_categories() {
        let request = compose_erasure_request(&ErasureRequestInput {
            provider_name: "Example Social".to_owned(),
            provider_account_identifier: "@river".to_owned(),
            data_categories: vec![
                ErasureDataCategory::PostsAndMessages,
                ErasureDataCategory::DirectMessages,
                ErasureDataCategory::PhotosAndMedia,
            ],
        })
        .expect("complete input composes locally");

        assert_eq!(
            request.subject,
            "Request to erase my personal data — Example Social"
        );
        assert!(request.body.contains("GDPR Article 17"));
        assert!(request.body.contains("CCPA/CPRA right to delete"));
        assert!(request.body.contains("Account identifier: @river"));
        assert!(request.body.contains("- posts and messages"));
        assert!(request.body.contains("- direct messages"));
        assert!(request.body.contains("- photos and other media"));
        assert!(!request.body.contains("OSL"));
        assert!(!request.body.contains("telemetry"));
    }

    #[test]
    fn scr_g1_refuses_a_request_without_data_categories() {
        let result = compose_erasure_request(&ErasureRequestInput {
            provider_name: "Example Social".to_owned(),
            provider_account_identifier: "@river".to_owned(),
            data_categories: vec![],
        });

        assert_eq!(result, Err(ErasureCompositionError::MissingDataCategories));
    }
}
