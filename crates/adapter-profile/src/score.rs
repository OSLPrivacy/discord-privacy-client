//! Weighted selector-candidate scoring for adapter profiles.
//!
//! The scorer chooses the best composer and transcript candidates from measured
//! evidence. It is intentionally deterministic and content-free: evidence
//! describes selector shape and trust properties, not row text, account names,
//! handles, credentials, or local paths.

use crate::schema::SelectorKind;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CandidateSurface {
    Composer,
    Transcript,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvidenceSignal {
    RequiredSelector,
    RoleMatches,
    StableBounds,
    CanaryMatched,
    OptionalSelector,
    StaleObservation,
    WrongSurface,
}

impl EvidenceSignal {
    pub const fn weight(self) -> i16 {
        match self {
            Self::RequiredSelector => 40,
            Self::RoleMatches => 24,
            Self::StableBounds => 18,
            Self::CanaryMatched => 14,
            Self::OptionalSelector => 6,
            Self::StaleObservation => -35,
            Self::WrongSurface => -90,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CandidateEvidence {
    pub candidate_id: &'static str,
    pub surface: CandidateSurface,
    pub selector_kind: SelectorKind,
    pub signal: EvidenceSignal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScoredCandidate {
    pub candidate_id: &'static str,
    pub surface: CandidateSurface,
    pub score: i16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CandidateSelection {
    pub composer: Option<ScoredCandidate>,
    pub transcript: Option<ScoredCandidate>,
}

pub fn score(evidence: &[CandidateEvidence]) -> CandidateSelection {
    CandidateSelection {
        composer: best_candidate(evidence, CandidateSurface::Composer),
        transcript: best_candidate(evidence, CandidateSurface::Transcript),
    }
}

fn best_candidate(
    evidence: &[CandidateEvidence],
    surface: CandidateSurface,
) -> Option<ScoredCandidate> {
    let mut best: Option<ScoredCandidate> = None;
    for fact in evidence.iter().filter(|fact| fact.surface == surface) {
        let score = score_one(evidence, fact.candidate_id, surface);
        let candidate = ScoredCandidate {
            candidate_id: fact.candidate_id,
            surface,
            score,
        };
        if best.is_none_or(|current| {
            candidate.score > current.score
                || (candidate.score == current.score
                    && candidate.candidate_id < current.candidate_id)
        }) {
            best = Some(candidate);
        }
    }
    best
}

fn score_one(
    evidence: &[CandidateEvidence],
    candidate_id: &'static str,
    surface: CandidateSurface,
) -> i16 {
    evidence
        .iter()
        .filter(|fact| fact.candidate_id == candidate_id && fact.surface == surface)
        .map(|fact| {
            let expected_kind = match surface {
                CandidateSurface::Composer => SelectorKind::ComposerInput,
                CandidateSurface::Transcript => SelectorKind::MessageList,
            };
            let kind_weight = if fact.selector_kind == expected_kind {
                20
            } else {
                EvidenceSignal::WrongSurface.weight()
            };
            kind_weight + fact.signal.weight()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence(
        candidate_id: &'static str,
        surface: CandidateSurface,
        selector_kind: SelectorKind,
        signal: EvidenceSignal,
    ) -> CandidateEvidence {
        CandidateEvidence {
            candidate_id,
            surface,
            selector_kind,
            signal,
        }
    }

    #[test]
    fn score() {
        let selection = super::score(&[
            evidence(
                "composer-low",
                CandidateSurface::Composer,
                SelectorKind::ComposerInput,
                EvidenceSignal::RequiredSelector,
            ),
            evidence(
                "composer-high",
                CandidateSurface::Composer,
                SelectorKind::ComposerInput,
                EvidenceSignal::RequiredSelector,
            ),
            evidence(
                "composer-high",
                CandidateSurface::Composer,
                SelectorKind::ComposerInput,
                EvidenceSignal::RoleMatches,
            ),
            evidence(
                "composer-high",
                CandidateSurface::Composer,
                SelectorKind::ComposerInput,
                EvidenceSignal::StableBounds,
            ),
            evidence(
                "composer-stale",
                CandidateSurface::Composer,
                SelectorKind::ComposerInput,
                EvidenceSignal::RequiredSelector,
            ),
            evidence(
                "composer-stale",
                CandidateSurface::Composer,
                SelectorKind::ComposerInput,
                EvidenceSignal::StaleObservation,
            ),
            evidence(
                "transcript-high",
                CandidateSurface::Transcript,
                SelectorKind::MessageList,
                EvidenceSignal::RequiredSelector,
            ),
            evidence(
                "transcript-high",
                CandidateSurface::Transcript,
                SelectorKind::MessageList,
                EvidenceSignal::CanaryMatched,
            ),
            evidence(
                "transcript-wrong-kind",
                CandidateSurface::Transcript,
                SelectorKind::ComposerInput,
                EvidenceSignal::RequiredSelector,
            ),
            evidence(
                "transcript-low",
                CandidateSurface::Transcript,
                SelectorKind::MessageList,
                EvidenceSignal::OptionalSelector,
            ),
        ]);

        let composer = selection.composer.expect("composer candidate selected");
        assert_eq!(composer.candidate_id, "composer-high");
        assert_eq!(composer.score, 142);

        let transcript = selection.transcript.expect("transcript candidate selected");
        assert_eq!(transcript.candidate_id, "transcript-high");
        assert_eq!(transcript.score, 94);
        assert!(
            composer.score > 60,
            "weak single-signal candidate must not win"
        );
        assert!(
            transcript.score > 0,
            "wrong-kind evidence must not win by presence alone"
        );
    }
}
