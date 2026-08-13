//! TASK 5181 - the final save of a download that the TASK 5166 quarantine
//! cleared.
//!
//! One function joins the two boundaries, so no caller can reach the second
//! without having passed the first:
//!
//! ```text
//!   quarantine.scan_and_release()   <- TASK 5166: real AMSI, exact clean scan
//!            |  Withheld  -> nothing is placed, no Attachment Services call
//!            v  Exposed
//!   place the bytes at the path the user chose
//!            v
//!   Windows Attachment Services (IAttachmentExecute::Save)   <- TASK 5181
//!            |  Marked            -> ZoneId=3 verified on the file
//!            |  PlatformLimited   -> saved, mark impossible here, said plainly
//!            v  failure           -> the file is removed again
//! ```
//!
//! The zone mark is defense in depth. It is applied *after* the scan and it can
//! never stand in for one: a withheld download never reaches this module's save
//! step at all, and no zone outcome can turn a detection into a delivery.

use std::fs;
use std::path::{Path, PathBuf};

use crate::download_zone_handoff::{
    AttachmentServicesSaver, ZoneHandoffFailure, ZoneHandoffOutcome, ZoneHandoffRequest,
};
use crate::protected_download_quarantine::{
    AmsiProvider, CleanScan, ProtectedDownloadOutcome, ProtectedDownloadQuarantine,
    QuarantinedDownload, WithheldDownload,
};

/// A download that reached the user's chosen path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeliveredDownload {
    pub path: PathBuf,
    pub bytes: u64,
    pub scan: CleanScan,
    pub zone: ZoneHandoffOutcome,
}

impl DeliveredDownload {
    pub fn zone_marked(&self) -> bool {
        self.zone.zone_marked()
    }

    pub fn save_calls(&self) -> u32 {
        self.zone.save_calls()
    }
}

/// The handoff did not happen. The bytes are taken back off the destination:
/// OSL does not leave a downloaded file on disk without the mark Windows needs
/// in order to keep defending it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingZoneHandoff {
    pub failure: ZoneHandoffFailure,
    pub message: String,
    pub destination: PathBuf,
    pub bytes_at_destination: u64,
    pub scan: CleanScan,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FinalSaveOutcome {
    Delivered(DeliveredDownload),
    /// The quarantine refused. No Attachment Services call was made and nothing
    /// was written to the destination.
    Withheld(WithheldDownload),
    ZoneHandoffMissing(MissingZoneHandoff),
    /// The bytes could not be placed at the chosen path at all.
    NotPlaced(String),
}

impl FinalSaveOutcome {
    pub fn name(&self) -> &'static str {
        match self {
            FinalSaveOutcome::Delivered(_) => "delivered",
            FinalSaveOutcome::Withheld(withheld) => withheld.reason.name(),
            FinalSaveOutcome::ZoneHandoffMissing(missing) => missing.failure.name(),
            FinalSaveOutcome::NotPlaced(_) => "not_placed",
        }
    }

    pub fn message(&self) -> String {
        match self {
            FinalSaveOutcome::Delivered(delivered) => delivered.zone.message(),
            FinalSaveOutcome::Withheld(withheld) => withheld.local_reason_text.clone(),
            FinalSaveOutcome::ZoneHandoffMissing(missing) => missing.message.clone(),
            FinalSaveOutcome::NotPlaced(detail) => {
                format!("This download could not be saved to the chosen path: {detail}")
            }
        }
    }

    /// Only a verified `Zone.Identifier` counts.
    pub fn zone_marked(&self) -> bool {
        matches!(self, FinalSaveOutcome::Delivered(delivered) if delivered.zone_marked())
    }
}

/// The shipping path: scan in quarantine, and only on an exact clean scan place
/// the bytes and hand the final save to Windows Attachment Services.
#[allow(clippy::too_many_arguments)]
pub fn deliver_after_clean_scan(
    quarantine: &ProtectedDownloadQuarantine,
    held: QuarantinedDownload,
    provider: &dyn AmsiProvider,
    saver: &dyn AttachmentServicesSaver,
    chosen_path: &Path,
    source_url: &str,
    referrer_url: &str,
    now_unix: u64,
) -> FinalSaveOutcome {
    let exposed = match quarantine.scan_and_release(held, provider, now_unix) {
        ProtectedDownloadOutcome::Exposed(exposed) => exposed,
        // The one and only early return. Nothing below here runs for a download
        // the quarantine refused, so a detection never reaches the save call or
        // the destination.
        ProtectedDownloadOutcome::Withheld(withheld) => {
            return FinalSaveOutcome::Withheld(withheld)
        }
    };

    if let Err(detail) = place_at(&exposed.path, chosen_path) {
        return FinalSaveOutcome::NotPlaced(detail);
    }

    let file_name = chosen_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let request = ZoneHandoffRequest {
        local_path: chosen_path.to_owned(),
        file_name,
        source_url: source_url.to_owned(),
        referrer_url: referrer_url.to_owned(),
    };

    match saver.save(&request) {
        Ok(zone) => {
            let bytes = fs::metadata(chosen_path).map(|meta| meta.len()).unwrap_or(0);
            FinalSaveOutcome::Delivered(DeliveredDownload {
                path: chosen_path.to_owned(),
                bytes,
                scan: exposed.scan,
                zone,
            })
        }
        Err(failure) => {
            let _ = fs::remove_file(chosen_path);
            let bytes_at_destination = fs::metadata(chosen_path).map(|m| m.len()).unwrap_or(0);
            FinalSaveOutcome::ZoneHandoffMissing(MissingZoneHandoff {
                message: failure.message(),
                failure,
                destination: chosen_path.to_owned(),
                bytes_at_destination,
                scan: exposed.scan,
            })
        }
    }
}

/// Put the released bytes at the chosen path. A rename when the quarantine and
/// the destination share a volume; otherwise a copy to a sibling temporary that
/// is renamed into place, so no partial file is ever visible under the final
/// name.
fn place_at(released: &Path, chosen_path: &Path) -> Result<(), String> {
    if let Some(parent) = chosen_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("the destination folder could not be created: {error}"))?;
    }
    if fs::rename(released, chosen_path).is_ok() {
        return Ok(());
    }
    let partial = chosen_path.with_extension("osl-part");
    let _ = fs::remove_file(&partial);
    fs::copy(released, &partial)
        .map_err(|error| format!("the download could not be written to the destination: {error}"))?;
    if let Ok(file) = fs::File::open(&partial) {
        let _ = file.sync_all();
    }
    fs::rename(&partial, chosen_path).map_err(|error| {
        let _ = fs::remove_file(&partial);
        format!("the download could not be moved into place: {error}")
    })?;
    fs::remove_file(released)
        .map_err(|error| format!("the released copy could not be removed: {error}"))?;
    Ok(())
}
