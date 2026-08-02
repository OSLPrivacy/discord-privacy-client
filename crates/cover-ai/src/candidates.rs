//! Independent capability candidates for cover selection.
//!
//! The selector receives already-rendered covers.  It never derives a second
//! candidate from a first one: each capability is drawn directly from the OS
//! CSPRNG before the canonical renderer sees it.

use rand::{rngs::OsRng, RngCore};

use crate::scorer::RenderedCover;

pub const CAPABILITY_BYTES: usize = 32;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateCapability([u8; CAPABILITY_BYTES]);

impl CandidateCapability {
    pub fn as_bytes(&self) -> &[u8; CAPABILITY_BYTES] {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedCandidate {
    pub capability: CandidateCapability,
    pub cover: RenderedCover,
}

/// Draw and render `count` independently generated capabilities.
///
/// Rendering is injected by the shipping codec owner.  This keeps the sampler
/// unable to choose words or alter canonical carrier bytes.
pub fn sample_candidates<F>(count: usize, mut render: F) -> Vec<RenderedCandidate>
where
    F: FnMut(&CandidateCapability) -> RenderedCover,
{
    (0..count)
        .map(|_| {
            let mut bytes = [0u8; CAPABILITY_BYTES];
            OsRng.fill_bytes(&mut bytes);
            let capability = CandidateCapability(bytes);
            let cover = render(&capability);
            RenderedCandidate { capability, cover }
        })
        .collect()
}
