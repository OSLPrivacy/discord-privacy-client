//! X deletion adapter with a local archive as its only ID source.
//!
//! The provider UI is used solely for the visible delete action.  Discovery by
//! scrolling is intentionally absent: the owner's downloaded archive is the
//! complete, free and reviewable source of tweet identifiers.

use super::verify_surface::{verify_on_independent_surface, HostedVerification, IndependentHostedSurface};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XArchive {
    tweet_ids: BTreeSet<String>,
}

impl XArchive {
    pub fn from_tweet_ids(ids: impl IntoIterator<Item = String>) -> Result<Self, XArchiveError> {
        let tweet_ids = ids.into_iter().map(|id| id.trim().to_owned()).collect::<BTreeSet<_>>();
        if tweet_ids.is_empty() || tweet_ids.iter().any(|id| !is_tweet_id(id)) {
            return Err(XArchiveError::InvalidArchive);
        }
        Ok(Self { tweet_ids })
    }

    pub fn contains(&self, tweet_id: &str) -> bool { self.tweet_ids.contains(tweet_id) }
    pub fn ids(&self) -> impl Iterator<Item = &str> { self.tweet_ids.iter().map(String::as_str) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XArchiveError { InvalidArchive, TweetNotInArchive, UiDeleteRefused }

/// The narrow visible-UI operation supplied by the consent-gated host.
/// There is deliberately no list/scroll method here.
pub trait XVisibleDelete {
    fn delete_tweet_by_id(&mut self, tweet_id: &str) -> Result<(), XArchiveError>;
}

pub struct XWebAdapter<'a, D, V> {
    archive: &'a XArchive,
    delete_ui: D,
    verifier: V,
}

impl<'a, D: XVisibleDelete, V: IndependentHostedSurface> XWebAdapter<'a, D, V> {
    pub fn new(archive: &'a XArchive, delete_ui: D, verifier: V) -> Self { Self { archive, delete_ui, verifier } }

    /// Delete only an identifier reviewed from the local export, then re-resolve
    /// that exact id on the independent provider surface.
    pub fn delete_and_verify(&mut self, tweet_id: &str) -> Result<HostedVerification, XArchiveError> {
        if !self.archive.contains(tweet_id) { return Err(XArchiveError::TweetNotInArchive); }
        self.delete_ui.delete_tweet_by_id(tweet_id)?;
        Ok(verify_on_independent_surface(&mut self.verifier, tweet_id))
    }
}

fn is_tweet_id(id: &str) -> bool { !id.is_empty() && id.len() <= 64 && id.bytes().all(|byte| byte.is_ascii_digit()) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scrub_hosted::verify_surface::{HostedVerifyError, HostedVerifyResult};

    struct Ui(Vec<String>);
    impl XVisibleDelete for Ui {
        fn delete_tweet_by_id(&mut self, id: &str) -> Result<(), XArchiveError> { self.0.push(id.to_owned()); Ok(()) }
    }
    struct Verify;
    impl IndependentHostedSurface for Verify {
        fn resolve_item_id(&mut self, _: &str) -> Result<HostedVerifyResult, HostedVerifyError> {
            Ok(HostedVerifyResult { covered: true, present: false })
        }
    }

    #[test]
    fn scr_h10_ids_come_from_archive_and_are_reresolved_after_visible_delete() {
        let archive = XArchive::from_tweet_ids(["123456".to_owned()]).unwrap();
        let mut adapter = XWebAdapter::new(&archive, Ui(Vec::new()), Verify);
        assert_eq!(adapter.delete_and_verify("123456"), Ok(HostedVerification::VerifiedGone));
        assert_eq!(adapter.delete_ui.0, ["123456"]);
        assert_eq!(adapter.delete_and_verify("999999"), Err(XArchiveError::TweetNotInArchive));
        // There is no scrolling/enumeration API on XVisibleDelete: archive IDs
        // are the only values this adapter can send to the provider UI.
    }
}
