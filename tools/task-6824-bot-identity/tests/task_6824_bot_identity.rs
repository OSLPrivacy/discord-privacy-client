use std::fmt::Debug;

use task_6824_bot_identity::{
    BotAction, BotAuthority, BotCredentials, BotDeclaration, BotError, BotLifecycle, BotScope,
    BotUiClaim, HumanCredentials, PackageDigest, SignedOwnerAction,
};

fn add_bot(
    authority: &mut BotAuthority,
    owner: &HumanCredentials,
    bot: &BotCredentials,
    digest: PackageDigest,
    scopes: Vec<BotScope>,
) {
    let declaration = BotDeclaration::new(
        bot.bot_id(),
        owner.human_id(),
        bot.public_key(),
        digest,
        scopes,
    );
    let action = SignedOwnerAction::add_bot(
        authority.enclave_id(),
        authority.next_membership_epoch(),
        declaration,
        owner,
    );
    let challenge = authority
        .begin_add(action)
        .expect("the authorized owner starts bot enrollment");
    let response = bot.answer_challenge(&challenge, digest);
    authority
        .complete_add(response)
        .expect("the declared bot proves possession of its own key and package");
}

fn authenticate(
    authority: &mut BotAuthority,
    bot: &BotCredentials,
    digest: PackageDigest,
) -> task_6824_bot_identity::BotSession {
    let challenge = authority
        .begin_authentication(bot.bot_id(), digest)
        .expect("enabled bot receives a fresh challenge");
    let response = bot.answer_challenge(&challenge, digest);
    authority
        .complete_authentication(response)
        .expect("bot challenge response authenticates")
}

fn assert_unchanged<T: Debug>(
    authority: &mut BotAuthority,
    expected: BotError,
    operation: impl FnOnce(&mut BotAuthority) -> Result<T, BotError>,
) {
    let before = authority.persist();
    assert_eq!(operation(authority).unwrap_err(), expected);
    assert_eq!(
        authority.persist(),
        before,
        "a refusal changed durable authority"
    );
}

#[test]
fn two_independently_keyed_bots_authenticate_and_post_after_restart() {
    let owner = HumanCredentials::generate("human-owner");
    let member = HumanCredentials::generate("human-member");
    let mut authority = BotAuthority::new(
        "enclave-6824",
        owner.public_identity(),
        [member.public_identity()],
    );
    let original_humans = authority.human_identities().to_vec();

    let archive = BotCredentials::generate("bot-archive");
    let deploy = BotCredentials::generate("bot-deploy");
    assert_ne!(archive.public_key(), deploy.public_key());
    assert_ne!(archive.bot_id(), deploy.bot_id());

    let archive_digest = PackageDigest::sha256(b"archive package 1.0.0");
    let deploy_digest = PackageDigest::sha256(b"deploy package 4.2.1");
    add_bot(
        &mut authority,
        &owner,
        &archive,
        archive_digest,
        vec![
            BotScope::post("incident-response"),
            BotScope::command("archive-index"),
        ],
    );
    add_bot(
        &mut authority,
        &owner,
        &deploy,
        deploy_digest,
        vec![
            BotScope::post("operations"),
            BotScope::command("deploy-status"),
        ],
    );

    assert_eq!(authority.bot_count(), 2);
    assert_eq!(authority.membership_epoch(), 2);
    for (bot, digest, expected_scope) in [
        (
            &archive,
            archive_digest,
            BotScope::post("incident-response"),
        ),
        (&deploy, deploy_digest, BotScope::post("operations")),
    ] {
        let record = authority.bot(bot.bot_id()).expect("bot membership record");
        assert_eq!(record.bot_id(), bot.bot_id());
        assert_eq!(record.owner_id(), owner.human_id());
        assert_eq!(record.public_key(), bot.public_key());
        assert_eq!(record.package_digest(), digest);
        assert_eq!(record.lifecycle(), BotLifecycle::Enabled);
        assert!(record.scopes().contains(&expected_scope));
        assert_eq!(
            record.membership_epoch(),
            authority.bot_epoch(bot.bot_id()).unwrap()
        );
    }
    assert_eq!(authority.human_identities(), original_humans.as_slice());

    let persisted = authority.persist();
    let mut restarted = BotAuthority::restart(&persisted).expect("bot authority survives restart");
    assert_eq!(restarted.persist(), persisted);
    assert_eq!(restarted.human_identities(), original_humans.as_slice());

    let archive_session = authenticate(&mut restarted, &archive, archive_digest);
    let deploy_session = authenticate(&mut restarted, &deploy, deploy_digest);
    assert_ne!(archive_session.session_id(), deploy_session.session_id());

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
    let archive_receipt = restarted
        .submit(&archive_session, archive_post.clone())
        .expect("archive post is accepted as the bot");
    let deploy_receipt = restarted
        .submit(&deploy_session, deploy_post.clone())
        .expect("deploy post is accepted as the bot");

    assert_ne!(archive_receipt.message_id(), deploy_receipt.message_id());
    assert_eq!(archive_receipt.signer_bot_id(), archive.bot_id());
    assert_eq!(deploy_receipt.signer_bot_id(), deploy.bot_id());
    assert_eq!(restarted.posts().len(), 2);
    assert_eq!(restarted.posts()[0].body(), b"archive bot says alpha");
    assert_eq!(restarted.posts()[1].body(), b"deploy bot says beta");
    assert_ne!(
        restarted.posts()[0].signature(),
        restarted.posts()[1].signature()
    );
    assert!(restarted
        .posts()
        .iter()
        .all(|post| post.person_signer().is_none()));

    let archive_command = archive.sign_action(
        &archive_session,
        archive_digest,
        2,
        BotAction::command("archive-index", b"run=incremental"),
    );
    let deploy_command = deploy.sign_action(
        &deploy_session,
        deploy_digest,
        2,
        BotAction::command("deploy-status", b"environment=staging"),
    );
    restarted.submit(&archive_session, archive_command).unwrap();
    restarted.submit(&deploy_session, deploy_command).unwrap();
    assert_eq!(restarted.commands().len(), 2);
    assert!(restarted
        .commands()
        .iter()
        .all(|command| command.person_signer().is_none()));

    assert_unchanged(&mut restarted, BotError::Replay, |authority| {
        authority.submit(&archive_session, archive_post)
    });
    assert_unchanged(&mut restarted, BotError::Replay, |authority| {
        authority.submit(&deploy_session, deploy_post)
    });
}

#[test]
fn add_authentication_signer_digest_scope_and_owner_faults_are_refused() {
    let owner = HumanCredentials::generate("human-owner");
    let member = HumanCredentials::generate("human-member");
    let mut authority = BotAuthority::new(
        "enclave-6824-hostile",
        owner.public_identity(),
        [member.public_identity()],
    );
    let bot = BotCredentials::generate("bot-report");
    let wrong_bot = BotCredentials::generate("bot-wrong-key");
    let digest = PackageDigest::sha256(b"report package 1");
    let changed_digest = PackageDigest::sha256(b"report package changed after approval");
    let declaration = BotDeclaration::new(
        bot.bot_id(),
        owner.human_id(),
        bot.public_key(),
        digest,
        vec![BotScope::post("reports"), BotScope::command("summarize")],
    );

    let unauthorized = SignedOwnerAction::add_bot(
        authority.enclave_id(),
        authority.next_membership_epoch(),
        declaration.clone(),
        &member,
    );
    assert_unchanged(&mut authority, BotError::UnauthorizedOwner, |authority| {
        authority.begin_add(unauthorized)
    });

    let add = SignedOwnerAction::add_bot(
        authority.enclave_id(),
        authority.next_membership_epoch(),
        declaration,
        &owner,
    );
    let challenge = authority.begin_add(add).unwrap();
    let pending = authority.persist();
    assert_eq!(
        authority
            .complete_add(wrong_bot.answer_challenge(&challenge, digest))
            .unwrap_err(),
        BotError::WrongBotKey
    );
    assert_eq!(authority.persist(), pending);
    assert_eq!(
        authority
            .complete_add(bot.answer_challenge(&challenge, changed_digest))
            .unwrap_err(),
        BotError::PackageDigestMismatch
    );
    assert_eq!(authority.persist(), pending);

    let correct_response = bot.answer_challenge(&challenge, digest);
    authority.complete_add(correct_response.clone()).unwrap();
    assert_unchanged(&mut authority, BotError::ChallengeReplay, |authority| {
        authority.complete_add(correct_response)
    });

    assert_unchanged(
        &mut authority,
        BotError::PackageDigestMismatch,
        |authority| authority.begin_authentication(bot.bot_id(), changed_digest),
    );
    let auth_challenge = authority
        .begin_authentication(bot.bot_id(), digest)
        .unwrap();
    let auth_pending = authority.persist();
    assert_eq!(
        authority
            .complete_authentication(wrong_bot.answer_challenge(&auth_challenge, digest))
            .unwrap_err(),
        BotError::UnknownBot
    );
    assert_eq!(authority.persist(), auth_pending);
    let auth_response = bot.answer_challenge(&auth_challenge, digest);
    let session = authority
        .complete_authentication(auth_response.clone())
        .unwrap();
    assert_unchanged(&mut authority, BotError::ChallengeReplay, |authority| {
        authority.complete_authentication(auth_response)
    });

    let wrong_key_post = wrong_bot.sign_action_as(
        bot.bot_id(),
        &session,
        digest,
        1,
        BotAction::post("reports", b"wrong key"),
    );
    assert_unchanged(&mut authority, BotError::WrongBotKey, |authority| {
        authority.submit(&session, wrong_key_post)
    });

    let changed_package_post = bot.sign_action(
        &session,
        changed_digest,
        1,
        BotAction::post("reports", b"changed package"),
    );
    assert_unchanged(
        &mut authority,
        BotError::PackageDigestMismatch,
        |authority| authority.submit(&session, changed_package_post),
    );

    let person_signed_post = owner.sign_bot_action_for_test(
        bot.bot_id(),
        &session,
        digest,
        1,
        BotAction::post("reports", b"person wearing a bot id"),
    );
    assert_unchanged(
        &mut authority,
        BotError::UserKeyImpersonation,
        |authority| authority.submit(&session, person_signed_post),
    );

    let out_of_scope = bot.sign_action(
        &session,
        digest,
        1,
        BotAction::post("owner-private", b"scope escalation"),
    );
    assert_unchanged(&mut authority, BotError::OutOfScope, |authority| {
        authority.submit(&session, out_of_scope)
    });

    let valid = bot.sign_action(
        &session,
        digest,
        1,
        BotAction::post("reports", b"one authentic report"),
    );
    authority.submit(&session, valid.clone()).unwrap();
    assert_unchanged(&mut authority, BotError::Replay, |authority| {
        authority.submit(&session, valid)
    });
    assert_eq!(authority.posts().len(), 1);
}

#[test]
fn disable_and_remove_revoke_live_queued_retry_restart_and_retained_credentials() {
    let owner = HumanCredentials::generate("human-owner");
    let member = HumanCredentials::generate("human-member");
    let mut authority = BotAuthority::new(
        "enclave-6824-revoke",
        owner.public_identity(),
        [member.public_identity()],
    );
    let humans_before = authority.human_identities().to_vec();
    let disable_bot = BotCredentials::generate("bot-disable");
    let remove_bot = BotCredentials::generate("bot-remove");
    let disable_digest = PackageDigest::sha256(b"disable package");
    let remove_digest = PackageDigest::sha256(b"remove package");
    add_bot(
        &mut authority,
        &owner,
        &disable_bot,
        disable_digest,
        vec![BotScope::post("automation")],
    );
    add_bot(
        &mut authority,
        &owner,
        &remove_bot,
        remove_digest,
        vec![BotScope::post("automation")],
    );
    let disable_session = authenticate(&mut authority, &disable_bot, disable_digest);
    let remove_session = authenticate(&mut authority, &remove_bot, remove_digest);

    let disable_accepted = disable_bot.sign_action(
        &disable_session,
        disable_digest,
        1,
        BotAction::post("automation", b"accepted before disable"),
    );
    authority
        .submit(&disable_session, disable_accepted.clone())
        .unwrap();
    let disable_queued = disable_bot.sign_action(
        &disable_session,
        disable_digest,
        2,
        BotAction::post("automation", b"queued before disable"),
    );
    let remove_accepted = remove_bot.sign_action(
        &remove_session,
        remove_digest,
        1,
        BotAction::post("automation", b"accepted before removal"),
    );
    authority
        .submit(&remove_session, remove_accepted.clone())
        .unwrap();
    let remove_queued = remove_bot.sign_action(
        &remove_session,
        remove_digest,
        2,
        BotAction::post("automation", b"queued before removal"),
    );

    let before_disable_epoch = authority.membership_epoch();
    let disable = SignedOwnerAction::disable_bot(
        authority.enclave_id(),
        authority.next_membership_epoch(),
        disable_bot.bot_id(),
        &owner,
    );
    authority.apply_owner_action(disable).unwrap();
    assert_eq!(authority.membership_epoch(), before_disable_epoch + 1);
    assert_eq!(
        authority.bot(disable_bot.bot_id()).unwrap().lifecycle(),
        BotLifecycle::Disabled
    );
    assert_eq!(authority.live_session_count(disable_bot.bot_id()), 0);
    assert_eq!(authority.human_identities(), humans_before.as_slice());
    for action in [disable_queued, disable_accepted] {
        assert_unchanged(&mut authority, BotError::BotDisabled, |authority| {
            authority.submit(&disable_session, action)
        });
    }
    assert_unchanged(&mut authority, BotError::BotDisabled, |authority| {
        authority.begin_authentication(disable_bot.bot_id(), disable_digest)
    });

    let before_remove_epoch = authority.membership_epoch();
    let remove = SignedOwnerAction::remove_bot(
        authority.enclave_id(),
        authority.next_membership_epoch(),
        remove_bot.bot_id(),
        &owner,
    );
    authority.apply_owner_action(remove).unwrap();
    assert_eq!(authority.membership_epoch(), before_remove_epoch + 1);
    assert_eq!(
        authority.bot(remove_bot.bot_id()).unwrap().lifecycle(),
        BotLifecycle::Removed
    );
    assert_eq!(authority.live_session_count(remove_bot.bot_id()), 0);
    assert_eq!(authority.human_identities(), humans_before.as_slice());
    for action in [remove_queued, remove_accepted] {
        assert_unchanged(&mut authority, BotError::BotRemoved, |authority| {
            authority.submit(&remove_session, action)
        });
    }
    assert_unchanged(&mut authority, BotError::BotRemoved, |authority| {
        authority.begin_authentication(remove_bot.bot_id(), remove_digest)
    });

    let persisted = authority.persist();
    let mut restarted = BotAuthority::restart(&persisted).unwrap();
    assert_eq!(restarted.human_identities(), humans_before.as_slice());
    assert_unchanged(&mut restarted, BotError::BotDisabled, |authority| {
        authority.begin_authentication(disable_bot.bot_id(), disable_digest)
    });
    assert_unchanged(&mut restarted, BotError::BotRemoved, |authority| {
        authority.begin_authentication(remove_bot.bot_id(), remove_digest)
    });

    let retained_disable = disable_bot.sign_action(
        &disable_session,
        disable_digest,
        99,
        BotAction::post("automation", b"hostile retained disable credential"),
    );
    let retained_remove = remove_bot.sign_action(
        &remove_session,
        remove_digest,
        99,
        BotAction::post("automation", b"hostile retained remove credential"),
    );
    assert_unchanged(&mut restarted, BotError::BotDisabled, |authority| {
        authority.submit(&disable_session, retained_disable)
    });
    assert_unchanged(&mut restarted, BotError::BotRemoved, |authority| {
        authority.submit(&remove_session, retained_remove)
    });
    assert_eq!(restarted.posts().len(), 2, "revoked paths appended a post");
}

#[test]
fn bot_ui_tag_requires_live_authenticated_bot_authority() {
    let owner = HumanCredentials::generate("human-owner");
    let member = HumanCredentials::generate("human-member");
    let mut authority = BotAuthority::new(
        "enclave-6824-ui",
        owner.public_identity(),
        [member.public_identity()],
    );
    let bot = BotCredentials::generate("bot-ui");
    let digest = PackageDigest::sha256(b"ui bot package");

    let forged = BotUiClaim::new(bot.bot_id(), "Helper", "BOT");
    assert_eq!(
        authority.render_bot_tag(&forged).unwrap_err(),
        BotError::NoBotAuthority
    );
    add_bot(
        &mut authority,
        &owner,
        &bot,
        digest,
        vec![BotScope::post("general")],
    );
    let tag = authority.render_bot_tag(&forged).unwrap();
    assert_eq!(tag.bot_id(), bot.bot_id());
    assert_eq!(tag.label(), "BOT");
    assert!(tag.authenticated());

    let disable = SignedOwnerAction::disable_bot(
        authority.enclave_id(),
        authority.next_membership_epoch(),
        bot.bot_id(),
        &owner,
    );
    authority.apply_owner_action(disable).unwrap();
    assert_eq!(
        authority.render_bot_tag(&forged).unwrap_err(),
        BotError::NoBotAuthority
    );

    let human_wearing_bot_tag = BotUiClaim::new(member.human_id(), "Member", "BOT");
    assert_eq!(
        authority
            .render_bot_tag(&human_wearing_bot_tag)
            .unwrap_err(),
        BotError::NoBotAuthority
    );
}
