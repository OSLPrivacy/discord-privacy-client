//! Startup loading for the signed profiles that ship with the desktop app.
//!
//! Selector hot updates will provide the optional cached document to this same
//! loader boundary. Until then, a profile is usable only when its compiled-in
//! signed document verifies at the current time.

use adapter_profile::{
    load_signed_adapter_profile_or_compiled_in, signal_default_profile,
    signal_default_trusted_signing_key_b64, whatsapp_default_profile,
    whatsapp_default_trusted_signing_key_b64, LoadedSignedAdapterProfile,
    SignedAdapterProfileLoaderError,
};
use std::time::{SystemTime, UNIX_EPOCH};

/// Verified native-adapter profiles retained for the lifetime of this process.
pub struct RuntimeAdapterProfiles {
    signal: LoadedSignedAdapterProfile,
    whatsapp: LoadedSignedAdapterProfile,
}

impl RuntimeAdapterProfiles {
    pub fn signal(&self) -> &LoadedSignedAdapterProfile {
        &self.signal
    }

    pub fn whatsapp(&self) -> &LoadedSignedAdapterProfile {
        &self.whatsapp
    }
}

/// Load every built-in adapter profile at process boot.
///
/// A failed signature or validity check prevents startup; this deliberately
/// does not substitute an unsigned or expired fallback profile.
pub fn load_verified_adapter_profiles(
    now_unix_seconds: u64,
) -> Result<RuntimeAdapterProfiles, SignedAdapterProfileLoaderError> {
    let signal = load_signed_adapter_profile_or_compiled_in(
        None,
        signal_default_trusted_signing_key_b64(),
        &signal_default_profile(),
        signal_default_trusted_signing_key_b64(),
        now_unix_seconds,
    )?;
    let whatsapp = load_signed_adapter_profile_or_compiled_in(
        None,
        whatsapp_default_trusted_signing_key_b64(),
        &whatsapp_default_profile(),
        whatsapp_default_trusted_signing_key_b64(),
        now_unix_seconds,
    )?;

    Ok(RuntimeAdapterProfiles { signal, whatsapp })
}

/// Runtime wrapper that takes the system clock only at the startup boundary.
pub fn load_verified_adapter_profiles_at_boot(
) -> Result<RuntimeAdapterProfiles, SignedAdapterProfileLoaderError> {
    let now_unix_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    load_verified_adapter_profiles(now_unix_seconds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapter_profile::AdapterProfileSource;

    const VALID_NOW: u64 = 1_800_000_000;
    const EXPIRED_NOW: u64 = 2_000_000_000;

    #[test]
    fn boot_loads_the_shipped_profiles_as_verified_compiled_in_documents() {
        let profiles = load_verified_adapter_profiles(VALID_NOW).unwrap();

        assert_eq!(profiles.signal().source(), AdapterProfileSource::CompiledIn);
        assert_eq!(profiles.signal().payload().app.stable_id, "signal");
        assert_eq!(
            profiles.whatsapp().source(),
            AdapterProfileSource::CompiledIn
        );
        assert_eq!(profiles.whatsapp().payload().app.stable_id, "whatsapp");
    }

    #[test]
    fn boot_refuses_expired_compiled_in_profiles() {
        assert!(load_verified_adapter_profiles(EXPIRED_NOW).is_err());
    }
}
