//! Hosted-provider recipe admission.
//!
//! A hosted provider recipe is not usable until the provider proves the hosted
//! account is bound to the OSL owner. Search capability and ownership evidence
//! are separate inputs so a broad search recipe cannot stand in for proof.

use std::fmt;

#[derive(Debug, Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum HostedProviderKind {
    Gmail,
    Discord,
    Telegram,
}

impl HostedProviderKind {
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Gmail => "gmail",
            Self::Discord => "discord",
            Self::Telegram => "telegram",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct HostedProviderDiscriminator {
    pub provider: HostedProviderKind,
    pub discriminator_id: &'static str,
    pub calibrated_probability_bps: u16,
    pub label: &'static str,
}

impl HostedProviderDiscriminator {
    pub const fn is_probabilistic(&self) -> bool {
        self.calibrated_probability_bps > 0 && self.calibrated_probability_bps < 10_000
    }
}

pub const HOSTED_PROVIDER_DISCRIMINATORS: &[HostedProviderDiscriminator] = &[
    HostedProviderDiscriminator {
        provider: HostedProviderKind::Gmail,
        discriminator_id: "gmail-visible-message-row",
        calibrated_probability_bps: 9_300,
        label: "probabilistic",
    },
    HostedProviderDiscriminator {
        provider: HostedProviderKind::Discord,
        discriminator_id: "discord-visible-self-row",
        calibrated_probability_bps: 8_800,
        label: "probabilistic",
    },
    HostedProviderDiscriminator {
        provider: HostedProviderKind::Telegram,
        discriminator_id: "telegram-visible-chat-bubble",
        calibrated_probability_bps: 8_500,
        label: "probabilistic",
    },
];

pub const fn hosted_provider_discriminators() -> &'static [HostedProviderDiscriminator] {
    HOSTED_PROVIDER_DISCRIMINATORS
}

#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct HostedOwnershipEvidence {
    commitment: [u8; 32],
}

impl HostedOwnershipEvidence {
    pub fn new(commitment: [u8; 32]) -> Option<Self> {
        if commitment.iter().all(|byte| *byte == 0) {
            None
        } else {
            Some(Self { commitment })
        }
    }

    pub const fn commitment(&self) -> [u8; 32] {
        self.commitment
    }
}

impl fmt::Debug for HostedOwnershipEvidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("HostedOwnershipEvidence")
            .field(&"<redacted>")
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct HostedSearchRecipe {
    recipe_id: &'static str,
    read_only: bool,
}

impl HostedSearchRecipe {
    pub const fn read_only(recipe_id: &'static str) -> Self {
        Self {
            recipe_id,
            read_only: true,
        }
    }

    pub const fn recipe_id(&self) -> &'static str {
        self.recipe_id
    }

    pub const fn is_read_only(&self) -> bool {
        self.read_only
    }
}

impl fmt::Debug for HostedSearchRecipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostedSearchRecipe")
            .field("recipe_id", &self.recipe_id)
            .field("read_only", &self.read_only)
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct HostedProviderWebRecipe {
    provider: HostedProviderKind,
    search: HostedSearchRecipe,
    deletion_supported: bool,
}

impl HostedProviderWebRecipe {
    pub const fn new(provider: HostedProviderKind, recipe_id: &'static str) -> Self {
        Self {
            provider,
            search: HostedSearchRecipe::read_only(recipe_id),
            deletion_supported: false,
        }
    }

    pub const fn provider(&self) -> HostedProviderKind {
        self.provider
    }

    pub const fn search(&self) -> &HostedSearchRecipe {
        &self.search
    }

    pub const fn deletion_supported(&self) -> bool {
        self.deletion_supported
    }
}

impl fmt::Debug for HostedProviderWebRecipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostedProviderWebRecipe")
            .field("provider", &self.provider)
            .field("search", &self.search)
            .field("deletion_supported", &self.deletion_supported)
            .finish()
    }
}

pub const GMAIL_WEB_RECIPE: HostedProviderWebRecipe =
    HostedProviderWebRecipe::new(HostedProviderKind::Gmail, "gmail-web-visible-row-scan-v1");
pub const DISCORD_WEB_RECIPE: HostedProviderWebRecipe = HostedProviderWebRecipe::new(
    HostedProviderKind::Discord,
    "discord-web-visible-row-scan-v1",
);
pub const TELEGRAM_WEB_RECIPE: HostedProviderWebRecipe = HostedProviderWebRecipe::new(
    HostedProviderKind::Telegram,
    "telegram-web-visible-row-scan-v1",
);

pub const HOSTED_PROVIDER_WEB_RECIPES: &[HostedProviderWebRecipe] =
    &[GMAIL_WEB_RECIPE, DISCORD_WEB_RECIPE, TELEGRAM_WEB_RECIPE];

pub const fn hosted_provider_web_recipes() -> &'static [HostedProviderWebRecipe] {
    HOSTED_PROVIDER_WEB_RECIPES
}

#[derive(Clone, Eq, PartialEq)]
pub struct HostedProviderRecipe {
    search: HostedSearchRecipe,
    ownership_evidence: HostedOwnershipEvidence,
}

impl HostedProviderRecipe {
    pub const fn search(&self) -> &HostedSearchRecipe {
        &self.search
    }

    pub const fn ownership_evidence(&self) -> HostedOwnershipEvidence {
        self.ownership_evidence
    }
}

impl fmt::Debug for HostedProviderRecipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostedProviderRecipe")
            .field("search", &self.search)
            .field("ownership_evidence", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum HostedProviderRecipeError {
    OwnershipEvidenceRequired,
    SearchRecipeMustBeReadOnly,
}

pub fn admit_hosted_provider_recipe(
    search: HostedSearchRecipe,
    ownership_evidence: Option<HostedOwnershipEvidence>,
) -> Result<HostedProviderRecipe, HostedProviderRecipeError> {
    if !search.is_read_only() {
        return Err(HostedProviderRecipeError::SearchRecipeMustBeReadOnly);
    }
    let ownership_evidence =
        ownership_evidence.ok_or(HostedProviderRecipeError::OwnershipEvidenceRequired)?;
    Ok(HostedProviderRecipe {
        search,
        ownership_evidence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosted_provider_recipe_requires_ownership_evidence() {
        let search = HostedSearchRecipe::read_only("discord-visible-row-scan-v1");

        assert_eq!(
            admit_hosted_provider_recipe(search.clone(), None),
            Err(HostedProviderRecipeError::OwnershipEvidenceRequired)
        );

        let evidence = HostedOwnershipEvidence::new([9; 32]).expect("nonzero evidence");
        let recipe = admit_hosted_provider_recipe(search.clone(), Some(evidence))
            .expect("ownership evidence admits provider recipe");

        assert_eq!(recipe.search().recipe_id(), search.recipe_id());
        assert_eq!(recipe.ownership_evidence(), evidence);
        assert!(HostedOwnershipEvidence::new([0; 32]).is_none());
    }

    #[test]
    fn hosted_provider_recipes_are_fixed_for_gmail_discord_telegram() {
        let recipes = hosted_provider_web_recipes();

        assert_eq!(
            recipes,
            &[GMAIL_WEB_RECIPE, DISCORD_WEB_RECIPE, TELEGRAM_WEB_RECIPE]
        );
        assert_eq!(
            recipes
                .iter()
                .map(|recipe| recipe.provider().wire_name())
                .collect::<Vec<_>>(),
            vec!["gmail", "discord", "telegram"]
        );
        assert_eq!(
            recipes
                .iter()
                .map(|recipe| recipe.search().recipe_id())
                .collect::<Vec<_>>(),
            vec![
                "gmail-web-visible-row-scan-v1",
                "discord-web-visible-row-scan-v1",
                "telegram-web-visible-row-scan-v1"
            ]
        );
        assert!(recipes.iter().all(|recipe| recipe.search().is_read_only()));
        assert!(recipes.iter().all(|recipe| !recipe.deletion_supported()));
    }

    #[test]
    fn hosted_provider_discriminators_are_probabilistic_and_calibrated() {
        let discriminators = hosted_provider_discriminators();

        assert_eq!(discriminators.len(), 3);
        assert_eq!(
            discriminators
                .iter()
                .map(|discriminator| discriminator.provider.wire_name())
                .collect::<Vec<_>>(),
            vec!["gmail", "discord", "telegram"]
        );
        assert_eq!(
            discriminators
                .iter()
                .map(|discriminator| discriminator.discriminator_id)
                .collect::<Vec<_>>(),
            vec![
                "gmail-visible-message-row",
                "discord-visible-self-row",
                "telegram-visible-chat-bubble"
            ]
        );
        assert_eq!(
            discriminators
                .iter()
                .map(|discriminator| discriminator.calibrated_probability_bps)
                .collect::<Vec<_>>(),
            vec![9_300, 8_800, 8_500]
        );
        assert!(discriminators.iter().all(|discriminator| {
            discriminator.is_probabilistic() && discriminator.label == "probabilistic"
        }));
    }
}
