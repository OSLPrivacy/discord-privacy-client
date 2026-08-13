//! Authoritative lifecycle and dock projection for an active encrypted call.
//!
//! The renderer is intentionally not represented here.  This owner holds the
//! `MediaRoomClient`, capture/device state and call clock together, and derives
//! the complete dock snapshot from those values.  Dropping or leaving the
//! session drops the media client (and therefore its zeroizing secrets).

use crate::error::Error;
use crate::media_room::{
    AuthenticatedDeviceId, AuthenticatedMediaEpochPackage, EncryptedMediaFrame, MediaRoomClient,
    RelayMediaPacket, SignedInDevice,
};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn invalid(message: impl Into<String>) -> Error {
    Error::Internal(message.into())
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VoiceConnectionState {
    Connecting,
    Connected,
    Reconnecting,
    DeviceLost,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VoiceDeviceChoice {
    pub id: String,
    pub label: String,
    pub available: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerifiedVoiceParticipant {
    pub account_id: String,
    pub display_name: String,
    pub verification: String,
    pub speaking: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VoiceCallSnapshot {
    pub session_id: String,
    pub revision: u64,
    pub room_label: String,
    pub connection: VoiceConnectionState,
    pub elapsed_ms: u64,
    pub participants: Vec<VerifiedVoiceParticipant>,
    pub muted: bool,
    pub deafened: bool,
    pub expanded: bool,
    pub input_devices: Vec<VoiceDeviceChoice>,
    pub output_devices: Vec<VoiceDeviceChoice>,
    pub selected_input_device_id: Option<String>,
    pub selected_output_device_id: Option<String>,
}

/// One native call owner. Route changes and renderer remounts only ask for a
/// new snapshot; they cannot reconstruct or replace any field in this struct.
pub struct ActiveVoiceCallSession {
    session_id: String,
    room_label: String,
    started_at_ms: u64,
    now_ms: u64,
    revision: u64,
    connection: VoiceConnectionState,
    muted: bool,
    deafened: bool,
    expanded: bool,
    input_devices: Vec<VoiceDeviceChoice>,
    output_devices: Vec<VoiceDeviceChoice>,
    selected_input_device_id: Option<String>,
    selected_output_device_id: Option<String>,
    capture_active: bool,
    join_count: u64,
    reconnect_count: u64,
    panel_mount_count: u64,
    route_visits: BTreeMap<String, u64>,
    client: Option<MediaRoomClient>,
}

impl ActiveVoiceCallSession {
    #[allow(clippy::too_many_arguments)]
    pub fn join_authenticated(
        session_id: impl Into<String>,
        room_label: impl Into<String>,
        started_at_ms: u64,
        package: &AuthenticatedMediaEpochPackage,
        authority_public_key: [u8; 32],
        device: SignedInDevice,
        input_devices: Vec<VoiceDeviceChoice>,
        output_devices: Vec<VoiceDeviceChoice>,
        selected_input_device_id: impl Into<String>,
        selected_output_device_id: impl Into<String>,
    ) -> Result<Self> {
        let session_id = session_id.into();
        let room_label = room_label.into();
        if session_id.is_empty() || room_label.is_empty() {
            return Err(invalid("voice call session identity is empty"));
        }
        let selected_input_device_id = selected_input_device_id.into();
        let selected_output_device_id = selected_output_device_id.into();
        require_available(&input_devices, &selected_input_device_id, "input")?;
        require_available(&output_devices, &selected_output_device_id, "output")?;
        let client = MediaRoomClient::join_authenticated(package, authority_public_key, device)?;
        Ok(Self {
            session_id,
            room_label,
            started_at_ms,
            now_ms: started_at_ms,
            revision: 1,
            connection: VoiceConnectionState::Connected,
            muted: false,
            deafened: false,
            expanded: false,
            input_devices,
            output_devices,
            selected_input_device_id: Some(selected_input_device_id),
            selected_output_device_id: Some(selected_output_device_id),
            capture_active: true,
            join_count: 1,
            reconnect_count: 0,
            panel_mount_count: 1,
            route_visits: BTreeMap::new(),
            client: Some(client),
        })
    }

    pub fn snapshot(&self) -> Option<VoiceCallSnapshot> {
        let client = self.client.as_ref()?;
        let participants = client
            .current_members()
            .into_iter()
            .map(|member| VerifiedVoiceParticipant {
                display_name: member.identity.account_id.clone(),
                account_id: member.identity.account_id,
                verification: "verified".to_string(),
                speaking: false,
            })
            .collect();
        Some(VoiceCallSnapshot {
            session_id: self.session_id.clone(),
            revision: self.revision,
            room_label: self.room_label.clone(),
            connection: self.connection,
            elapsed_ms: self.now_ms.saturating_sub(self.started_at_ms),
            participants,
            muted: self.muted,
            deafened: self.deafened,
            expanded: self.expanded,
            input_devices: self.input_devices.clone(),
            output_devices: self.output_devices.clone(),
            selected_input_device_id: self.selected_input_device_id.clone(),
            selected_output_device_id: self.selected_output_device_id.clone(),
        })
    }

    pub fn advance_clock(&mut self, now_ms: u64) -> Result<()> {
        if now_ms < self.now_ms {
            return Err(invalid("voice call clock moved backwards"));
        }
        self.now_ms = now_ms;
        self.bump();
        Ok(())
    }

    pub fn navigate(&mut self, primary_surface: &str) -> Result<()> {
        const PRIMARY: [&str; 7] = [
            "home",
            "inbox",
            "people",
            "privacy",
            "activity",
            "connections",
            "settings",
        ];
        if !PRIMARY.contains(&primary_surface) {
            return Err(invalid(format!(
                "unknown primary voice dock surface {primary_surface}"
            )));
        }
        *self
            .route_visits
            .entry(primary_surface.to_string())
            .or_default() += 1;
        self.panel_mount_count += 1;
        self.bump();
        Ok(())
    }

    pub fn toggle_mute(&mut self, expected_session_id: &str) -> Result<()> {
        self.require_attached(expected_session_id)?;
        self.muted = !self.muted;
        self.capture_active = !self.muted && self.selected_input_device_id.is_some();
        self.bump();
        Ok(())
    }

    pub fn toggle_deafen(&mut self, expected_session_id: &str) -> Result<()> {
        self.require_attached(expected_session_id)?;
        self.deafened = !self.deafened;
        self.bump();
        Ok(())
    }

    pub fn set_expanded(&mut self, expected_session_id: &str, expanded: bool) -> Result<()> {
        self.require_attached(expected_session_id)?;
        self.expanded = expanded;
        self.bump();
        Ok(())
    }

    pub fn choose_input_device(
        &mut self,
        expected_session_id: &str,
        device_id: &str,
    ) -> Result<()> {
        self.require_attached(expected_session_id)?;
        require_available(&self.input_devices, device_id, "input")?;
        self.selected_input_device_id = Some(device_id.to_string());
        self.capture_active = !self.muted;
        self.bump();
        Ok(())
    }

    pub fn choose_output_device(
        &mut self,
        expected_session_id: &str,
        device_id: &str,
    ) -> Result<()> {
        self.require_attached(expected_session_id)?;
        require_available(&self.output_devices, device_id, "output")?;
        self.selected_output_device_id = Some(device_id.to_string());
        self.bump();
        Ok(())
    }

    pub fn lose_input_device(&mut self, device_id: &str) -> Result<()> {
        let device = self
            .input_devices
            .iter_mut()
            .find(|device| device.id == device_id)
            .ok_or_else(|| invalid("lost voice input device is unknown"))?;
        device.available = false;
        if self.selected_input_device_id.as_deref() == Some(device_id) {
            self.selected_input_device_id = None;
            self.capture_active = false;
            self.connection = VoiceConnectionState::DeviceLost;
        }
        self.bump();
        Ok(())
    }

    pub fn reconnect(&mut self, expected_session_id: &str) -> Result<()> {
        self.require_attached(expected_session_id)?;
        if self.selected_input_device_id.is_none() {
            return Err(invalid(
                "voice call cannot reconnect without an input device",
            ));
        }
        self.connection = VoiceConnectionState::Connected;
        self.capture_active = !self.muted;
        self.reconnect_count += 1;
        self.bump();
        Ok(())
    }

    /// Renderer restart: remount the projection while retaining this native
    /// owner.  It must not increment join or reconnect counts.
    pub fn restart_panel(&mut self) {
        self.panel_mount_count += 1;
        self.bump();
    }

    pub fn send_audio(
        &mut self,
        sequence: u64,
        samples: &[u8],
    ) -> Result<Option<EncryptedMediaFrame>> {
        let Some(client) = self.client.as_mut() else {
            return Err(invalid("voice call media keys are torn down"));
        };
        if self.connection != VoiceConnectionState::Connected || !self.capture_active || self.muted
        {
            return Ok(None);
        }
        client.send_media_frame(sequence, samples).map(Some)
    }

    pub fn receive_audio(&mut self, packet: &RelayMediaPacket) -> Result<Option<Vec<u8>>> {
        let Some(client) = self.client.as_mut() else {
            return Err(invalid("voice call media keys are torn down"));
        };
        if self.connection != VoiceConnectionState::Connected || self.deafened {
            return Ok(None);
        }
        client.receive_media_frame(packet).map(Some)
    }

    pub fn leave(&mut self, expected_session_id: &str) -> Result<()> {
        self.require_attached(expected_session_id)?;
        self.capture_active = false;
        self.selected_input_device_id = None;
        self.selected_output_device_id = None;
        self.client.take();
        self.bump();
        Ok(())
    }

    pub fn capture_active(&self) -> bool {
        self.capture_active
    }

    pub fn media_keys_live(&self) -> bool {
        self.client.is_some()
    }

    pub fn join_count(&self) -> u64 {
        self.join_count
    }

    pub fn reconnect_count(&self) -> u64 {
        self.reconnect_count
    }

    pub fn panel_mount_count(&self) -> u64 {
        self.panel_mount_count
    }

    pub fn primary_surface_count(&self) -> usize {
        self.route_visits.len()
    }

    pub fn identity(&self) -> Option<&AuthenticatedDeviceId> {
        self.client.as_ref().map(MediaRoomClient::identity)
    }

    fn require_attached(&self, expected_session_id: &str) -> Result<()> {
        if self.client.is_none() || expected_session_id != self.session_id {
            return Err(invalid(
                "voice call panel is detached from the active encrypted session",
            ));
        }
        Ok(())
    }

    fn bump(&mut self) {
        self.revision = self.revision.saturating_add(1);
    }
}

fn require_available(devices: &[VoiceDeviceChoice], id: &str, kind: &str) -> Result<()> {
    if devices
        .iter()
        .any(|device| device.id == id && device.available)
    {
        Ok(())
    } else {
        Err(invalid(format!("voice {kind} device is unavailable")))
    }
}
