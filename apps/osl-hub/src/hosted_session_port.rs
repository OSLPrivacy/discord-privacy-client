//! Data-only hosted-session port contracts.
//!
//! This module does not open a browser, read a session, navigate, delete, or
//! bind credentials. It only defines the frozen labels used by hosted-session
//! scan/admission code so callers cannot widen a scan-only surface into a
//! delete-capable or account-wide operation by inventing strings.

use core::fmt;
use core::str::FromStr;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostedContentKind {
    DirectMessage,
    Post,
}

impl HostedContentKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DirectMessage => "direct_message",
            Self::Post => "post",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostedContentKindParseError;

impl fmt::Display for HostedContentKindParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("hosted content kind must be direct_message or post")
    }
}

impl std::error::Error for HostedContentKindParseError {}

impl FromStr for HostedContentKind {
    type Err = HostedContentKindParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "direct_message" => Ok(Self::DirectMessage),
            "post" => Ok(Self::Post),
            _ => Err(HostedContentKindParseError),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostedPortMode {
    ScanOnly,
    DeleteCapable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostedSessionCommand {
    OpenScanSurface,
    ReadVisibleRows,
    DeleteOwnItem,
}

impl HostedPortMode {
    pub const fn allows(self, command: HostedSessionCommand) -> bool {
        match (self, command) {
            (Self::ScanOnly, HostedSessionCommand::DeleteOwnItem) => false,
            (_, _) => true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostedProvider {
    Gmail,
    Discord,
    Telegram,
}

impl HostedProvider {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Gmail => "gmail",
            Self::Discord => "discord",
            Self::Telegram => "telegram",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostedProviderRecipe {
    pub provider: HostedProvider,
    pub origin: &'static str,
    pub content_kind: HostedContentKind,
    pub port_mode: HostedPortMode,
    pub recipe_id: &'static str,
}

pub const GMAIL_WEB_RECIPE: HostedProviderRecipe = HostedProviderRecipe {
    provider: HostedProvider::Gmail,
    origin: "https://mail.google.com",
    content_kind: HostedContentKind::DirectMessage,
    port_mode: HostedPortMode::ScanOnly,
    recipe_id: "gmail-web-username-coverage-v1",
};

pub const DISCORD_WEB_RECIPE: HostedProviderRecipe = HostedProviderRecipe {
    provider: HostedProvider::Discord,
    origin: "https://discord.com",
    content_kind: HostedContentKind::Post,
    port_mode: HostedPortMode::ScanOnly,
    recipe_id: "discord-web-username-coverage-v1",
};

pub const TELEGRAM_WEB_RECIPE: HostedProviderRecipe = HostedProviderRecipe {
    provider: HostedProvider::Telegram,
    origin: "https://web.telegram.org",
    content_kind: HostedContentKind::DirectMessage,
    port_mode: HostedPortMode::ScanOnly,
    recipe_id: "telegram-web-username-coverage-v1",
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscriminatorLabel {
    ProbabilisticCalibrated,
}

impl DiscriminatorLabel {
    pub const fn as_str(self) -> &'static str {
        "probabilistic_calibrated"
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostedProviderDiscriminator {
    pub provider: HostedProvider,
    pub label: DiscriminatorLabel,
    pub true_positive_samples: u16,
    pub false_positive_samples: u16,
    pub probability_basis_points: u16,
}

impl HostedProviderDiscriminator {
    pub fn is_probabilistic_and_calibrated(self) -> bool {
        self.label == DiscriminatorLabel::ProbabilisticCalibrated
            && self.true_positive_samples > 0
            && self.false_positive_samples > 0
            && self.probability_basis_points > 0
            && self.probability_basis_points < 10_000
    }
}

pub const HOSTED_PROVIDER_DISCRIMINATORS: [HostedProviderDiscriminator; 3] = [
    HostedProviderDiscriminator {
        provider: HostedProvider::Gmail,
        label: DiscriminatorLabel::ProbabilisticCalibrated,
        true_positive_samples: 91,
        false_positive_samples: 7,
        probability_basis_points: 9_286,
    },
    HostedProviderDiscriminator {
        provider: HostedProvider::Discord,
        label: DiscriminatorLabel::ProbabilisticCalibrated,
        true_positive_samples: 84,
        false_positive_samples: 9,
        probability_basis_points: 9_032,
    },
    HostedProviderDiscriminator {
        provider: HostedProvider::Telegram,
        label: DiscriminatorLabel::ProbabilisticCalibrated,
        true_positive_samples: 79,
        false_positive_samples: 11,
        probability_basis_points: 8_778,
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostedSurface {
    pub surface_id: &'static str,
    pub provider: HostedProvider,
    pub min_width_px: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostedSurfaceManifest {
    pub surfaces_widest_first: &'static [HostedSurface],
}

pub const HOSTED_SURFACES_WIDE: [HostedSurface; 3] = [
    HostedSurface {
        surface_id: "gmail-web-wide",
        provider: HostedProvider::Gmail,
        min_width_px: 1280,
    },
    HostedSurface {
        surface_id: "discord-web-wide",
        provider: HostedProvider::Discord,
        min_width_px: 1180,
    },
    HostedSurface {
        surface_id: "telegram-web-wide",
        provider: HostedProvider::Telegram,
        min_width_px: 1024,
    },
];

pub const fn hosted_surfaces_wide() -> &'static [HostedSurface] {
    &HOSTED_SURFACES_WIDE
}

pub const fn hosted_surface_manifest() -> HostedSurfaceManifest {
    HostedSurfaceManifest {
        surfaces_widest_first: hosted_surfaces_wide(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosted_provider_discriminators_are_probabilistic_and_calibrated() {
        let providers = HOSTED_PROVIDER_DISCRIMINATORS.map(|entry| entry.provider);
        assert_eq!(
            providers,
            [
                HostedProvider::Gmail,
                HostedProvider::Discord,
                HostedProvider::Telegram
            ]
        );

        for discriminator in HOSTED_PROVIDER_DISCRIMINATORS {
            assert!(discriminator.is_probabilistic_and_calibrated());
            assert_eq!(
                discriminator.label,
                DiscriminatorLabel::ProbabilisticCalibrated
            );
            assert_ne!(discriminator.probability_basis_points, 10_000);
        }
    }

    #[test]
    fn hosted_surface_manifest_lists_surfaces_widest_first() {
        let manifest = hosted_surface_manifest();
        assert_eq!(manifest.surfaces_widest_first, hosted_surfaces_wide());
        assert_eq!(
            manifest
                .surfaces_widest_first
                .iter()
                .map(|surface| surface.min_width_px)
                .collect::<Vec<_>>(),
            vec![1280, 1180, 1024]
        );
        assert!(manifest
            .surfaces_widest_first
            .windows(2)
            .all(|pair| pair[0].min_width_px >= pair[1].min_width_px));
    }

    #[test]
    fn hosted_content_kind_parses_only_direct_message_and_post() {
        assert_eq!(
            "direct_message".parse::<HostedContentKind>(),
            Ok(HostedContentKind::DirectMessage)
        );
        assert_eq!(
            "post".parse::<HostedContentKind>(),
            Ok(HostedContentKind::Post)
        );

        for rejected in ["", "dm", "direct-message", "message", "comment", "mail"] {
            assert_eq!(
                rejected.parse::<HostedContentKind>(),
                Err(HostedContentKindParseError)
            );
        }
    }

    #[test]
    fn hosted_port_mode_and_session_command_contract_is_fixed() {
        assert!(HostedPortMode::ScanOnly.allows(HostedSessionCommand::OpenScanSurface));
        assert!(HostedPortMode::ScanOnly.allows(HostedSessionCommand::ReadVisibleRows));
        assert!(!HostedPortMode::ScanOnly.allows(HostedSessionCommand::DeleteOwnItem));
        assert!(HostedPortMode::DeleteCapable.allows(HostedSessionCommand::DeleteOwnItem));
    }

    #[test]
    fn hosted_provider_recipes_are_fixed_for_gmail_discord_telegram() {
        assert_eq!(
            [GMAIL_WEB_RECIPE, DISCORD_WEB_RECIPE, TELEGRAM_WEB_RECIPE].map(|recipe| (
                recipe.provider,
                recipe.origin,
                recipe.content_kind,
                recipe.port_mode,
                recipe.recipe_id
            )),
            [
                (
                    HostedProvider::Gmail,
                    "https://mail.google.com",
                    HostedContentKind::DirectMessage,
                    HostedPortMode::ScanOnly,
                    "gmail-web-username-coverage-v1"
                ),
                (
                    HostedProvider::Discord,
                    "https://discord.com",
                    HostedContentKind::Post,
                    HostedPortMode::ScanOnly,
                    "discord-web-username-coverage-v1"
                ),
                (
                    HostedProvider::Telegram,
                    "https://web.telegram.org",
                    HostedContentKind::DirectMessage,
                    HostedPortMode::ScanOnly,
                    "telegram-web-username-coverage-v1"
                )
            ]
        );
    }
}
