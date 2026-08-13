//! Executable acceptance check for TASK 6824.
//!
//! `TASK6824_STARVE=<dimension>` deliberately withholds one required witness.
//! The final completeness check names the missing dimension and exits 1.

use std::collections::BTreeSet;
use task_6824_bot_identity::{
    BotAction, BotAuthority, BotCredentials, BotDeclaration, BotError, BotLifecycle, BotScope,
    BotUiClaim, HumanCredentials, PackageDigest, SignedOwnerAction,
};

const DIMENSIONS: [&str; 8] = [
    "bot",
    "challenge",
    "signer",
    "owner-action",
    "restart",
    "revoke-path",
    "hostile-retained-credential",
    "ui-authority",
];

#[derive(Default)]
struct Counts {
    bots_added: usize,
    independent_keys: usize,
    authenticated_after_restart: usize,
    signed_posts_after_restart: usize,
    distinct_signed_messages: usize,
    challenge_refusals: usize,
    signer_refusals: usize,
    owner_action_refusals: usize,
    replay_refusals: usize,
    revoke_path_refusals: usize,
    hostile_retained_refusals: usize,
    preserved_human_identities: usize,
    authenticated_ui_tags: usize,
    refused_ui_tags: usize,
}

struct Check {
    starve: Option<String>,
    rows: BTreeSet<&'static str>,
    counts: Counts,
}

impl Check {
    fn from_environment() -> Result<Self, String> {
        let starve = std::env::var("TASK6824_STARVE")
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        if let Some(value) = starve.as_deref() {
            if !DIMENSIONS.contains(&value) {
                return Err(format!(
                    "TASK6824_STARVE must be one of {} (received {value:?})",
                    DIMENSIONS.join(", ")
                ));
            }
        }
        Ok(Self {
            starve,
            rows: BTreeSet::new(),
            counts: Counts::default(),
        })
    }

    fn starved(&self, dimension: &str) -> bool {
        self.starve.as_deref() == Some(dimension)
    }

    fn record(&mut self, dimension: &'static str) {
        if !self.starved(dimension) {
            self.rows.insert(dimension);
        }
    }

    fn finish(&self) -> Result<(), String> {
        let missing: Vec<_> = DIMENSIONS
            .iter()
            .copied()
            .filter(|dimension| !self.rows.contains(dimension))
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "TASK 6824 check incomplete: missing required dimension(s): {}",
                missing.join(", ")
            ));
        }
        Ok(())
    }
}

fn require(condition: bool, message: impl Into<String>) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn expect_error_unchanged<T>(
    authority: &mut BotAuthority,
    expected: BotError,
    label: &str,
    operation: impl FnOnce(&mut BotAuthority) -> Result<T, BotError>,
) -> Result<(), String> {
    let before = authority.persist();
    match operation(authority) {
        Err(actual) if actual == expected => {}
        Err(actual) => {
            return Err(format!(
                "{label}: expected {expected:?}, received {actual:?}"
            ));
        }
        Ok(_) => return Err(format!("{label}: hostile operation was accepted")),
    }
    require(
        authority.persist() == before,
        format!("{label}: refusal changed durable bot authority"),
    )
}

fn add_bot(
    authority: &mut BotAuthority,
    owner: &HumanCredentials,
    bot: &BotCredentials,
    digest: PackageDigest,
    scopes: Vec<BotScope>,
) -> Result<(), String> {
    let declaration = BotDeclaration::new(
        bot.bot_id(),
        owner.human_id(),
        bot.public_key(),
        digest,
        scopes,
    );
    let add = SignedOwnerAction::add_bot(
        authority.enclave_id(),
        authority.next_membership_epoch(),
        declaration,
        owner,
    );
    let challenge = authority
        .begin_add(add)
        .map_err(|error| format!("authorized add refused: {error:?}"))?;
    authority
        .complete_add(bot.answer_challenge(&challenge, digest))
        .map_err(|error| format!("correct enrollment challenge refused: {error:?}"))?;
    Ok(())
}

fn authenticate(
    authority: &mut BotAuthority,
    bot: &BotCredentials,
    digest: PackageDigest,
) -> Result<task_6824_bot_identity::BotSession, String> {
    let challenge = authority
        .begin_authentication(bot.bot_id(), digest)
        .map_err(|error| format!("authentication challenge refused: {error:?}"))?;
    authority
        .complete_authentication(bot.answer_challenge(&challenge, digest))
        .map_err(|error| format!("correct authentication response refused: {error:?}"))
}

fn run() -> Result<(), String> {
    let mut check = Check::from_environment()?;
    let owner = HumanCredentials::generate("human-owner");
    let member = HumanCredentials::generate("human-member");
    let mut authority = BotAuthority::new(
        "enclave-6824-check",
        owner.public_identity(),
        [member.public_identity()],
    );
    let original_humans = authority.human_identities().to_vec();

    let archive = BotCredentials::generate("bot-archive");
    let deploy = BotCredentials::generate("bot-deploy");
    let wrong_bot = BotCredentials::generate("bot-wrong-key");
    let archive_digest = PackageDigest::sha256(b"archive package 1.0.0");
    let deploy_digest = PackageDigest::sha256(b"deploy package 4.2.1");
    let changed_archive_digest = PackageDigest::sha256(b"archive package changed");

    // An owner action from a human member who is not an owner must never begin
    // enrollment. The byte comparison also catches partial pending state.
    if !check.starved("owner-action") {
        let unauthorized = BotDeclaration::new(
            archive.bot_id(),
            member.human_id(),
            archive.public_key(),
            archive_digest,
            vec![BotScope::post("incident-response")],
        );
        let action = SignedOwnerAction::add_bot(
            authority.enclave_id(),
            authority.next_membership_epoch(),
            unauthorized,
            &member,
        );
        expect_error_unchanged(
            &mut authority,
            BotError::UnauthorizedOwner,
            "unauthorized add",
            |authority| authority.begin_add(action),
        )?;
        check.counts.owner_action_refusals += 1;
        check.record("owner-action");
    }

    // Exercise the enrollment challenge independently before adding the two
    // production fixtures. Wrong key, changed package and replay each leave
    // the pending/accepted state unchanged.
    let probe = BotCredentials::generate("bot-challenge-probe");
    let probe_digest = PackageDigest::sha256(b"challenge probe package");
    let probe_changed = PackageDigest::sha256(b"challenge probe changed");
    let probe_declaration = BotDeclaration::new(
        probe.bot_id(),
        owner.human_id(),
        probe.public_key(),
        probe_digest,
        vec![BotScope::post("probe")],
    );
    let probe_add = SignedOwnerAction::add_bot(
        authority.enclave_id(),
        authority.next_membership_epoch(),
        probe_declaration,
        &owner,
    );
    let probe_challenge = authority
        .begin_add(probe_add)
        .map_err(|error| format!("challenge probe add refused: {error:?}"))?;
    if !check.starved("challenge") {
        expect_error_unchanged(
            &mut authority,
            BotError::WrongBotKey,
            "wrong challenge signer",
            |authority| {
                authority.complete_add(wrong_bot.answer_challenge(&probe_challenge, probe_digest))
            },
        )?;
        expect_error_unchanged(
            &mut authority,
            BotError::PackageDigestMismatch,
            "changed challenge package",
            |authority| {
                authority.complete_add(probe.answer_challenge(&probe_challenge, probe_changed))
            },
        )?;
        check.counts.challenge_refusals += 2;
    }
    let probe_response = probe.answer_challenge(&probe_challenge, probe_digest);
    authority
        .complete_add(probe_response.clone())
        .map_err(|error| format!("valid challenge probe refused: {error:?}"))?;
    if !check.starved("challenge") {
        expect_error_unchanged(
            &mut authority,
            BotError::ChallengeReplay,
            "challenge replay",
            |authority| authority.complete_add(probe_response),
        )?;
        check.counts.challenge_refusals += 1;
        check.counts.replay_refusals += 1;
        check.record("challenge");
    }

    add_bot(
        &mut authority,
        &owner,
        &archive,
        archive_digest,
        vec![
            BotScope::post("incident-response"),
            BotScope::command("archive-index"),
        ],
    )?;
    add_bot(
        &mut authority,
        &owner,
        &deploy,
        deploy_digest,
        vec![
            BotScope::post("operations"),
            BotScope::command("deploy-status"),
        ],
    )?;
    check.counts.bots_added = 2;
    if !check.starved("bot") {
        require(
            archive.public_key() != deploy.public_key() && archive.bot_id() != deploy.bot_id(),
            "the two bots do not have independent identities",
        )?;
        require(
            authority.bot_count() == 3,
            "durable authority did not retain the two bots and challenge probe",
        )?;
        for (bot, digest, scope) in [
            (
                &archive,
                archive_digest,
                BotScope::post("incident-response"),
            ),
            (&deploy, deploy_digest, BotScope::post("operations")),
        ] {
            let record = authority
                .bot(bot.bot_id())
                .ok_or_else(|| format!("missing bot record {:?}", bot.bot_id()))?;
            require(
                record.owner_id() == owner.human_id(),
                "bot owner binding changed",
            )?;
            require(
                record.public_key() == bot.public_key(),
                "bot key binding changed",
            )?;
            require(
                record.package_digest() == digest,
                "bot package binding changed",
            )?;
            require(
                record.lifecycle() == BotLifecycle::Enabled,
                "bot is not enabled",
            )?;
            require(
                record.scopes().contains(&scope),
                "bot scoped membership changed",
            )?;
        }
        check.counts.independent_keys = 2;
        check.record("bot");
    }

    // Persist/restart is part of the authentication claim, not merely a state
    // serialization unit test. A starved run deliberately continues on the
    // in-memory authority so the final ledger reports `restart` missing.
    let persisted = authority.persist();
    let mut operational = if check.starved("restart") {
        authority
    } else {
        let restarted = BotAuthority::restart(&persisted)
            .map_err(|error| format!("restart refused durable authority: {error:?}"))?;
        require(
            restarted.persist() == persisted,
            "restart changed durable authority",
        )?;
        require(
            restarted.human_identities() == original_humans.as_slice(),
            "restart changed human identities",
        )?;
        check.record("restart");
        restarted
    };

    let archive_session = authenticate(&mut operational, &archive, archive_digest)?;
    let deploy_session = authenticate(&mut operational, &deploy, deploy_digest)?;
    require(
        archive_session.session_id() != deploy_session.session_id(),
        "independent bots received the same session",
    )?;
    check.counts.authenticated_after_restart = 2;

    let archive_post = archive.sign_action(
        &archive_session,
        archive_digest,
        1,
        BotAction::post("incident-response", b"archive bot says alpha"),
    );
    let deploy_post = deploy.sign_action(
        &deploy_session,
        deploy_digest,
        1,
        BotAction::post("operations", b"deploy bot says beta"),
    );
    let archive_receipt = operational
        .submit(&archive_session, archive_post.clone())
        .map_err(|error| format!("archive signed post refused: {error:?}"))?;
    let deploy_receipt = operational
        .submit(&deploy_session, deploy_post.clone())
        .map_err(|error| format!("deploy signed post refused: {error:?}"))?;
    require(
        archive_receipt.message_id() != deploy_receipt.message_id(),
        "two bot posts have the same message id",
    )?;
    require(
        archive_receipt.signer_bot_id() == archive.bot_id()
            && deploy_receipt.signer_bot_id() == deploy.bot_id(),
        "post receipt does not name its bot signer",
    )?;
    require(
        operational.posts().len() == 2
            && operational.posts()[0].body() == b"archive bot says alpha"
            && operational.posts()[1].body() == b"deploy bot says beta"
            && operational.posts()[0].signature() != operational.posts()[1].signature()
            && operational
                .posts()
                .iter()
                .all(|post| post.person_signer().is_none()),
        "post bodies/signatures were not distinct non-person bot messages",
    )?;
    check.counts.signed_posts_after_restart = 2;
    check.counts.distinct_signed_messages = 2;

    if !check.starved("signer") {
        let wrong_key_post = wrong_bot.sign_action_as(
            archive.bot_id(),
            &archive_session,
            archive_digest,
            2,
            BotAction::post("incident-response", b"wrong key"),
        );
        expect_error_unchanged(
            &mut operational,
            BotError::WrongBotKey,
            "wrong bot key",
            |authority| authority.submit(&archive_session, wrong_key_post),
        )?;
        let changed_package_post = archive.sign_action(
            &archive_session,
            changed_archive_digest,
            2,
            BotAction::post("incident-response", b"changed package"),
        );
        expect_error_unchanged(
            &mut operational,
            BotError::PackageDigestMismatch,
            "changed package digest",
            |authority| authority.submit(&archive_session, changed_package_post),
        )?;
        let person_post = owner.sign_bot_action_for_test(
            archive.bot_id(),
            &archive_session,
            archive_digest,
            2,
            BotAction::post("incident-response", b"person wearing bot id"),
        );
        expect_error_unchanged(
            &mut operational,
            BotError::UserKeyImpersonation,
            "user-key impersonation",
            |authority| authority.submit(&archive_session, person_post),
        )?;
        check.counts.signer_refusals = 3;
        check.record("signer");
    }

    // Replaying either accepted signed message is rejected without appending.
    expect_error_unchanged(
        &mut operational,
        BotError::Replay,
        "archive message replay",
        |authority| authority.submit(&archive_session, archive_post.clone()),
    )?;
    expect_error_unchanged(
        &mut operational,
        BotError::Replay,
        "deploy message replay",
        |authority| authority.submit(&deploy_session, deploy_post.clone()),
    )?;
    check.counts.replay_refusals += 2;

    // Queue one fresh signed action for each bot, then revoke both in different
    // lifecycle states. Accepted retries, queued actions, authentication and
    // live session credentials must all resolve current membership first.
    let archive_queued = archive.sign_action(
        &archive_session,
        archive_digest,
        2,
        BotAction::post("incident-response", b"queued before disable"),
    );
    let deploy_queued = deploy.sign_action(
        &deploy_session,
        deploy_digest,
        2,
        BotAction::post("operations", b"queued before removal"),
    );
    let before_disable_epoch = operational.membership_epoch();
    let disable = SignedOwnerAction::disable_bot(
        operational.enclave_id(),
        operational.next_membership_epoch(),
        archive.bot_id(),
        &owner,
    );
    operational
        .apply_owner_action(disable)
        .map_err(|error| format!("authorized disable refused: {error:?}"))?;
    let before_remove_epoch = operational.membership_epoch();
    let remove = SignedOwnerAction::remove_bot(
        operational.enclave_id(),
        operational.next_membership_epoch(),
        deploy.bot_id(),
        &owner,
    );
    operational
        .apply_owner_action(remove)
        .map_err(|error| format!("authorized removal refused: {error:?}"))?;

    if !check.starved("revoke-path") {
        require(
            operational.membership_epoch() == before_remove_epoch + 1
                && before_remove_epoch == before_disable_epoch + 1,
            "disable/remove did not each advance membership",
        )?;
        require(
            operational.bot(archive.bot_id()).map(|bot| bot.lifecycle())
                == Some(BotLifecycle::Disabled)
                && operational.bot(deploy.bot_id()).map(|bot| bot.lifecycle())
                    == Some(BotLifecycle::Removed),
            "disable/remove lifecycle state was not retained",
        )?;
        require(
            operational.live_session_count(archive.bot_id()) == 0
                && operational.live_session_count(deploy.bot_id()) == 0,
            "revocation retained a live bot session",
        )?;
        for (label, session, action, error) in [
            (
                "disabled queued action",
                &archive_session,
                archive_queued.clone(),
                BotError::BotDisabled,
            ),
            (
                "disabled accepted retry",
                &archive_session,
                archive_post.clone(),
                BotError::BotDisabled,
            ),
            (
                "removed queued action",
                &deploy_session,
                deploy_queued.clone(),
                BotError::BotRemoved,
            ),
            (
                "removed accepted retry",
                &deploy_session,
                deploy_post.clone(),
                BotError::BotRemoved,
            ),
        ] {
            expect_error_unchanged(&mut operational, error, label, |authority| {
                authority.submit(session, action)
            })?;
            check.counts.revoke_path_refusals += 1;
        }
        expect_error_unchanged(
            &mut operational,
            BotError::BotDisabled,
            "disabled reauthentication",
            |authority| authority.begin_authentication(archive.bot_id(), archive_digest),
        )?;
        expect_error_unchanged(
            &mut operational,
            BotError::BotRemoved,
            "removed reauthentication",
            |authority| authority.begin_authentication(deploy.bot_id(), deploy_digest),
        )?;
        check.counts.revoke_path_refusals += 2;
        check.record("revoke-path");
    }

    require(
        operational.human_identities() == original_humans.as_slice(),
        "bot lifecycle changed human identities",
    )?;
    check.counts.preserved_human_identities = original_humans.len();

    let revoked_bytes = operational.persist();
    let mut revoked_restart = BotAuthority::restart(&revoked_bytes)
        .map_err(|error| format!("post-revocation restart failed: {error:?}"))?;
    if !check.starved("revoke-path") {
        expect_error_unchanged(
            &mut revoked_restart,
            BotError::BotDisabled,
            "disabled authentication after restart",
            |authority| authority.begin_authentication(archive.bot_id(), archive_digest),
        )?;
        expect_error_unchanged(
            &mut revoked_restart,
            BotError::BotRemoved,
            "removed authentication after restart",
            |authority| authority.begin_authentication(deploy.bot_id(), deploy_digest),
        )?;
        check.counts.revoke_path_refusals += 2;
    }

    if !check.starved("hostile-retained-credential") {
        let retained_disabled = archive.sign_action(
            &archive_session,
            archive_digest,
            99,
            BotAction::post("incident-response", b"hostile retained disable credential"),
        );
        let retained_removed = deploy.sign_action(
            &deploy_session,
            deploy_digest,
            99,
            BotAction::post("operations", b"hostile retained remove credential"),
        );
        expect_error_unchanged(
            &mut revoked_restart,
            BotError::BotDisabled,
            "hostile retained disabled credential",
            |authority| authority.submit(&archive_session, retained_disabled),
        )?;
        expect_error_unchanged(
            &mut revoked_restart,
            BotError::BotRemoved,
            "hostile retained removed credential",
            |authority| authority.submit(&deploy_session, retained_removed),
        )?;
        check.counts.hostile_retained_refusals = 2;
        check.record("hostile-retained-credential");
    }

    // Use a separate enabled bot for the positive UI authority witness; both
    // revoked bot tags, a human wearing BOT, and a claim without membership
    // authority remain red.
    if !check.starved("ui-authority") {
        let enabled_claim = BotUiClaim::new(probe.bot_id(), "Challenge probe", "BOT");
        let tag = revoked_restart
            .render_bot_tag(&enabled_claim)
            .map_err(|error| format!("authenticated enabled BOT tag refused: {error:?}"))?;
        require(
            tag.bot_id() == probe.bot_id() && tag.label() == "BOT" && tag.authenticated(),
            "BOT tag was not bound to authenticated bot authority",
        )?;
        check.counts.authenticated_ui_tags = 1;
        for (label, claim) in [
            (
                "disabled bot tag",
                BotUiClaim::new(archive.bot_id(), "Archive", "BOT"),
            ),
            (
                "removed bot tag",
                BotUiClaim::new(deploy.bot_id(), "Deploy", "BOT"),
            ),
            (
                "human wearing BOT tag",
                BotUiClaim::new(member.human_id(), "Member", "BOT"),
            ),
            (
                "unknown bot tag",
                BotUiClaim::new(wrong_bot.bot_id(), "Unknown", "BOT"),
            ),
        ] {
            let error = revoked_restart
                .render_bot_tag(&claim)
                .expect_err("UI tag without live bot authority was accepted");
            require(
                error == BotError::NoBotAuthority,
                format!("{label}: expected NoBotAuthority, received {error:?}"),
            )?;
            check.counts.refused_ui_tags += 1;
        }
        check.record("ui-authority");
    }

    // The signer row covers all three exact hostile signer/package strings;
    // this explicit row assertion prevents a partial negative sweep passing.
    if !check.starved("signer") {
        require(
            check.counts.signer_refusals == 3,
            "signer refusal sweep incomplete",
        )?;
    }
    if !check.starved("challenge") {
        require(
            check.counts.challenge_refusals == 3,
            "challenge refusal sweep incomplete",
        )?;
    }

    check.finish()?;
    println!(
        "TASK6824 bots_added={} independent_bot_keys={} authenticated_after_restart={} signed_posts_after_restart={} distinct_signed_messages={}",
        check.counts.bots_added,
        check.counts.independent_keys,
        check.counts.authenticated_after_restart,
        check.counts.signed_posts_after_restart,
        check.counts.distinct_signed_messages,
    );
    println!(
        "TASK6824 refused wrong_key=1 changed_package_digest=1 replay={} user_key_impersonation=1 unauthorized_add={} challenge_refusals={}",
        check.counts.replay_refusals,
        check.counts.owner_action_refusals,
        check.counts.challenge_refusals,
    );
    println!(
        "TASK6824 lifecycle disabled=1 removed=1 revoke_path_refusals={} hostile_retained_credentials_refused={} human_identities_preserved={}",
        check.counts.revoke_path_refusals,
        check.counts.hostile_retained_refusals,
        check.counts.preserved_human_identities,
    );
    println!(
        "TASK6824 ui authenticated_bot_tags={} ui_tags_without_bot_authority_refused={} label=BOT",
        check.counts.authenticated_ui_tags, check.counts.refused_ui_tags,
    );
    println!("TASK6824 finish_line=PASS required_dimensions=8/8");
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("TASK6824 finish_line=FAIL {error}");
        std::process::exit(1);
    }
}
