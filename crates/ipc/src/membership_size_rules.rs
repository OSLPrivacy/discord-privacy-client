//! Authoritative, product-specific membership size rules.
//!
//! Group chats and Enclaves deliberately do not share a limiter. A group-chat
//! roster includes its creator and stops at 20 total people. Enclave admission
//! is unlimited; the measured removal threshold is disclosure/progress policy
//! only and is never consulted by [`enforce_enclave_candidate_size`].

use serde::{Deserialize, Serialize};

pub const MEMBERSHIP_SIZE_RULES_SCHEMA: &str = "osl.membership-size-rules.v1";
pub const GROUP_CHAT_MAX_PEOPLE: usize = 20;
pub const GROUP_CHAT_FULL_ERROR: &str = "Group chats hold at most 20 people";
pub const GROUP_CHAT_SETTINGS_COPY: &str =
    "Group chats can have up to 20 people, including the creator.";
pub const GROUP_CHAT_HELP_COPY: &str =
    "A group chat admits 20 total people. Person 21 is not added.";
pub const ENCLAVE_SETTINGS_COPY: &str = "Enclaves have no member limit.";
pub const ENCLAVE_HELP_COPY: &str = "Enclave admission has no maximum. At the measured removal threshold, OSL warns that removal takes time and shows re-key progress; the threshold never blocks a member.";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MembershipProduct {
    GroupChat,
    Enclave,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GroupChatAdmissionRule {
    pub maximum_people_including_creator: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EnclaveAdmissionRule {
    pub maximum_people: Option<usize>,
}

/// The group-chat rule has its own authority and is not derived from Enclave
/// removal measurements.
pub const fn group_chat_admission_rule() -> GroupChatAdmissionRule {
    GroupChatAdmissionRule {
        maximum_people_including_creator: GROUP_CHAT_MAX_PEOPLE,
    }
}

/// The Enclave rule is independently authoritative and permanently expresses
/// no maximum. Its removal threshold is supplied only to the disclosure DTO.
pub const fn enclave_admission_rule() -> EnclaveAdmissionRule {
    EnclaveAdmissionRule {
        maximum_people: None,
    }
}

pub fn enforce_group_chat_candidate_size(candidate_people: usize) -> Result<(), &'static str> {
    if candidate_people > group_chat_admission_rule().maximum_people_including_creator {
        Err(GROUP_CHAT_FULL_ERROR)
    } else {
        Ok(())
    }
}

pub fn enforce_enclave_candidate_size(_candidate_people: usize) -> Result<(), &'static str> {
    debug_assert_eq!(enclave_admission_rule().maximum_people, None);
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupChatSizeRuleDto {
    pub maximum_people_including_creator: usize,
    pub settings_copy: &'static str,
    pub help_copy: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnclaveSizeRuleDto {
    pub maximum_people: Option<usize>,
    pub measured_removal_threshold: usize,
    pub settings_copy: &'static str,
    pub help_copy: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MembershipSizeRulesDto {
    pub schema: &'static str,
    pub group_chat: GroupChatSizeRuleDto,
    pub enclave: EnclaveSizeRuleDto,
}

/// Backend command payload used by Settings and help surfaces. The caller
/// supplies gate 6576's measured N; N is serialized only under Enclave removal
/// disclosure and cannot affect either admission function.
pub fn cmd_osl_membership_size_rules(
    measured_removal_threshold: usize,
) -> Result<MembershipSizeRulesDto, String> {
    if measured_removal_threshold == 0 {
        return Err("OSL: measured Enclave removal threshold must be positive".to_owned());
    }
    Ok(MembershipSizeRulesDto {
        schema: MEMBERSHIP_SIZE_RULES_SCHEMA,
        group_chat: GroupChatSizeRuleDto {
            maximum_people_including_creator: group_chat_admission_rule()
                .maximum_people_including_creator,
            settings_copy: GROUP_CHAT_SETTINGS_COPY,
            help_copy: GROUP_CHAT_HELP_COPY,
        },
        enclave: EnclaveSizeRuleDto {
            maximum_people: enclave_admission_rule().maximum_people,
            measured_removal_threshold,
            settings_copy: ENCLAVE_SETTINGS_COPY,
            help_copy: ENCLAVE_HELP_COPY,
        },
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SizeConsumerBinding {
    pub consumer: &'static str,
    pub product: MembershipProduct,
    pub maximum_people: Option<usize>,
}

const GROUP_CHAT_SIZE_CONSUMERS: [SizeConsumerBinding; 10] = [
    group_binding("schema.group_chat"),
    group_binding("command.membership_size_rules.group_chat"),
    group_binding("settings.group_chat"),
    group_binding("help.group_chat"),
    group_binding("membership.scope.admit"),
    group_binding("membership.scope.replace"),
    group_binding("membership_service.join"),
    group_binding("membership_service.reopen"),
    group_binding("command.create_group_conversation"),
    group_binding("command.membership_update_and_send_seed"),
];

const ENCLAVE_SIZE_CONSUMERS: [SizeConsumerBinding; 8] = [
    enclave_binding("schema.enclave"),
    enclave_binding("command.membership_size_rules.enclave"),
    enclave_binding("settings.enclave"),
    enclave_binding("help.enclave"),
    enclave_binding("membership_service.join"),
    enclave_binding("hub.named_enclave_registry"),
    enclave_binding("hub.enclave_conversation_context"),
    enclave_binding("enclave_removal.begin"),
];

const fn group_binding(consumer: &'static str) -> SizeConsumerBinding {
    SizeConsumerBinding {
        consumer,
        product: MembershipProduct::GroupChat,
        maximum_people: Some(GROUP_CHAT_MAX_PEOPLE),
    }
}

const fn enclave_binding(consumer: &'static str) -> SizeConsumerBinding {
    SizeConsumerBinding {
        consumer,
        product: MembershipProduct::Enclave,
        maximum_people: None,
    }
}

pub const fn group_chat_size_consumer_inventory() -> &'static [SizeConsumerBinding] {
    &GROUP_CHAT_SIZE_CONSUMERS
}

pub const fn enclave_size_consumer_inventory() -> &'static [SizeConsumerBinding] {
    &ENCLAVE_SIZE_CONSUMERS
}
