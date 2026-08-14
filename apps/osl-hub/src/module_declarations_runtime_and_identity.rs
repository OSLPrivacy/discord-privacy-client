// Runtime orchestration, identity, chat delivery, and realtime declarations.
// Append related work here.
pub mod placement;

#[cfg(feature = "core")]
pub mod broker;
#[cfg(feature = "core")]
pub mod burn_job_fence;
pub mod chat_app_timer_policy;
pub mod chat_capture_protection;
/// **The claim state.** What OSL may publicly say about each ruled surface, and
/// why — the owner gate `PLAN.md` r4-5 calls "the claim-state gap". Not behind a
/// feature: `native_apps` derives every public support label from it, and the
/// publication gates that read it must exist in every build that can compile the
/// native adapters.
pub mod claim_state;
#[cfg(feature = "core")]
pub mod cleanup;
#[cfg(feature = "core")]
pub mod core_bridge;
#[cfg(feature = "core")]
pub mod deadman;
#[cfg(feature = "core")]
pub mod destruct_ack_rollup;
#[cfg(feature = "core")]
pub mod device_transfer;
#[cfg(all(feature = "core", feature = "discord-qa-shell"))]
pub mod discord_qa_identity;
#[cfg(all(feature = "core", feature = "discord-qa-shell"))]
pub mod discord_qa_inbound_receipt;
#[cfg(feature = "core")]
pub mod eager_fetch;
#[cfg(feature = "core")]
pub mod eager_fetch_retry;
pub mod identity_binding_verifier;
#[cfg(feature = "core")]
pub mod identity_registry;
#[cfg(feature = "core")]
pub mod inbound_receipts;
pub mod isolated_worker;
#[cfg(feature = "core")]
pub mod mass_cleanup;
pub mod osl_chat_conversations;
pub mod osl_chat_delivery;
pub mod osl_chat_queue;
pub mod realtime_client;
pub mod realtime_decoy;
pub mod realtime_pipe;
pub mod realtime_resume;
pub mod realtime_subscription;
#[cfg(feature = "core")]
pub mod receipt_emit;
pub mod row_who_wrote_it;
#[cfg(feature = "core")]
pub mod runtime_switches;
