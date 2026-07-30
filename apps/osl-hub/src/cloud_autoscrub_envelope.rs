//! Surface-bound envelope checks for optional cloud AutoScrub payloads.
//!
//! The validator is deliberately small and fail-closed: an envelope is usable
//! only for the exact origin, schema version, and account binding the caller is
//! currently operating on.

use std::fmt;

const MAX_BINDING_BYTES: usize = 256;
const MAX_ORIGIN_BYTES: usize = 128;
const MAX_SUPPORTED_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Eq, PartialEq)]
pub struct CloudAutoScrubSurfaceRef {
    pub origin: String,
    pub schema_version: u32,
    pub account_binding: Vec<u8>,
}

impl CloudAutoScrubSurfaceRef {
    pub fn new(
        origin: impl Into<String>,
        schema_version: u32,
        account_binding: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            origin: origin.into(),
            schema_version,
            account_binding: account_binding.into(),
        }
    }
}

impl fmt::Debug for CloudAutoScrubSurfaceRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CloudAutoScrubSurfaceRef")
            .field("origin_present", &!self.origin.is_empty())
            .field("schema_version", &self.schema_version)
            .field("account_binding", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct CloudAutoScrubEnvelope {
    pub origin: String,
    pub schema_version: u32,
    pub account_binding: Vec<u8>,
    pub payload: Vec<u8>,
}

impl CloudAutoScrubEnvelope {
    pub fn new(
        origin: impl Into<String>,
        schema_version: u32,
        account_binding: impl Into<Vec<u8>>,
        payload: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            origin: origin.into(),
            schema_version,
            account_binding: account_binding.into(),
            payload: payload.into(),
        }
    }
}

impl fmt::Debug for CloudAutoScrubEnvelope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CloudAutoScrubEnvelope")
            .field("origin_present", &!self.origin.is_empty())
            .field("schema_version", &self.schema_version)
            .field("account_binding", &"<redacted>")
            .field("payload_len", &self.payload.len())
            .finish()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CloudAutoScrubEnvelopeError {
    InvalidOrigin,
    UnsupportedSchema,
    InvalidAccountBinding,
    EmptyPayload,
    OriginMismatch,
    SchemaMismatch,
    AccountMismatch,
}

impl fmt::Display for CloudAutoScrubEnvelopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidOrigin => "cloud AutoScrub envelope origin is invalid",
            Self::UnsupportedSchema => "cloud AutoScrub envelope schema is unsupported",
            Self::InvalidAccountBinding => "cloud AutoScrub account binding is invalid",
            Self::EmptyPayload => "cloud AutoScrub envelope payload is empty",
            Self::OriginMismatch => "cloud AutoScrub envelope origin does not match the surface",
            Self::SchemaMismatch => "cloud AutoScrub envelope schema does not match the surface",
            Self::AccountMismatch => "cloud AutoScrub envelope account does not match the surface",
        };
        f.write_str(message)
    }
}

impl std::error::Error for CloudAutoScrubEnvelopeError {}

pub fn validate_envelope(
    envelope: &CloudAutoScrubEnvelope,
) -> Result<(), CloudAutoScrubEnvelopeError> {
    validate_origin(&envelope.origin)?;
    validate_schema(envelope.schema_version)?;
    validate_account_binding(&envelope.account_binding)?;
    if envelope.payload.is_empty() {
        return Err(CloudAutoScrubEnvelopeError::EmptyPayload);
    }
    Ok(())
}

pub fn validate_envelope_for_surface(
    envelope: &CloudAutoScrubEnvelope,
    surface: &CloudAutoScrubSurfaceRef,
) -> Result<(), CloudAutoScrubEnvelopeError> {
    validate_envelope(envelope)?;
    validate_origin(&surface.origin)?;
    validate_schema(surface.schema_version)?;
    validate_account_binding(&surface.account_binding)?;
    if envelope.origin != surface.origin {
        return Err(CloudAutoScrubEnvelopeError::OriginMismatch);
    }
    if envelope.schema_version != surface.schema_version {
        return Err(CloudAutoScrubEnvelopeError::SchemaMismatch);
    }
    if envelope.account_binding != surface.account_binding {
        return Err(CloudAutoScrubEnvelopeError::AccountMismatch);
    }
    Ok(())
}

fn validate_origin(origin: &str) -> Result<(), CloudAutoScrubEnvelopeError> {
    if origin.is_empty() || origin.len() > MAX_ORIGIN_BYTES {
        return Err(CloudAutoScrubEnvelopeError::InvalidOrigin);
    }
    Ok(())
}

fn validate_schema(schema_version: u32) -> Result<(), CloudAutoScrubEnvelopeError> {
    if schema_version == 0 || schema_version > MAX_SUPPORTED_SCHEMA_VERSION {
        return Err(CloudAutoScrubEnvelopeError::UnsupportedSchema);
    }
    Ok(())
}

fn validate_account_binding(binding: &[u8]) -> Result<(), CloudAutoScrubEnvelopeError> {
    if binding.is_empty() || binding.len() > MAX_BINDING_BYTES {
        return Err(CloudAutoScrubEnvelopeError::InvalidAccountBinding);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surface() -> CloudAutoScrubSurfaceRef {
        CloudAutoScrubSurfaceRef::new("desktop-local", 1, b"account-binding".to_vec())
    }

    fn envelope() -> CloudAutoScrubEnvelope {
        CloudAutoScrubEnvelope::new("desktop-local", 1, b"account-binding".to_vec(), b"payload")
    }

    #[test]
    fn validate_envelope_for_surface_refuses_origin_schema_account_mismatch() {
        assert_eq!(
            validate_envelope_for_surface(&envelope(), &surface()),
            Ok(())
        );

        let wrong_origin = CloudAutoScrubEnvelope::new(
            "browser-import",
            1,
            b"account-binding".to_vec(),
            b"payload",
        );
        assert_eq!(
            validate_envelope_for_surface(&wrong_origin, &surface()),
            Err(CloudAutoScrubEnvelopeError::OriginMismatch)
        );

        let schema_two_surface =
            CloudAutoScrubSurfaceRef::new("desktop-local", 2, b"account-binding".to_vec());
        let wrong_schema = CloudAutoScrubEnvelope::new(
            "desktop-local",
            1,
            b"account-binding".to_vec(),
            b"payload",
        );
        assert_eq!(
            validate_envelope_for_surface(&wrong_schema, &schema_two_surface),
            Err(CloudAutoScrubEnvelopeError::SchemaMismatch)
        );

        let wrong_account =
            CloudAutoScrubEnvelope::new("desktop-local", 1, b"other-account".to_vec(), b"payload");
        assert_eq!(
            validate_envelope_for_surface(&wrong_account, &surface()),
            Err(CloudAutoScrubEnvelopeError::AccountMismatch)
        );
    }
}
