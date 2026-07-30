//! Hosted-provider recipe admission.
//!
//! A hosted provider recipe is not usable until the provider proves the hosted
//! account is bound to the OSL owner. Search capability and ownership evidence
//! are separate inputs so a broad search recipe cannot stand in for proof.

use std::fmt;

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
}
