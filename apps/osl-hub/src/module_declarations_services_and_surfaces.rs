// Services, web surfaces, and carrier-specific UI declarations. Append related work here.
#[cfg(feature = "core")]
pub mod server_records;
pub mod service_connections;
#[cfg(feature = "core")]
pub mod service_host;
#[cfg(feature = "core")]
pub mod services;
#[cfg(feature = "core")]
pub mod shipping_email_receive;
pub mod shipping_receive;
#[cfg(feature = "core")]
pub mod update_apply;
#[cfg(feature = "core")]
pub mod update_state_backup;
pub mod updates;
pub mod visual_binding;
pub mod web_surface_adapter;
pub mod website_driver;
pub mod whatsapp_accessibility;
pub mod whatsapp_qa_host;
#[cfg(feature = "core")]
pub mod whatsapp_qa_pairing;
pub mod whatsapp_qa_transport;
pub mod whatsapp_window_composer;
/// Receiver-backed protected/normal display state for marked X DM and post
/// rows. Kept separate from the web adapter so only receiving-job evidence can
/// populate protected text.
pub mod x_eye_state;
pub mod x_public_cover;
#[cfg(feature = "core")]
pub mod x_shipping_eye;
pub mod x_whitelist;
/// Hermetic records for the direct X active-window discovery command.
pub mod x_window_composer;
