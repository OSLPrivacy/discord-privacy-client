//! The one shipping arrival seam.
//!
//! A carrier adapter may turn an authenticated carrier event into an
//! [`ArrivedMessageRow`], but it must not open/persist the event itself.  This
//! is deliberately small: the job records the calling service before it gives
//! the row to the product-specific opener, which makes the attribution part of
//! the normal row rather than a Discord-only side channel.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShippingService {
    Discord,
    Email,
    OslChats,
    X,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArrivedMessageRow<P> {
    pub service: ShippingService,
    pub carrier_row_id: String,
    pub payload: P,
}

impl<P> ArrivedMessageRow<P> {
    pub fn email(carrier_row_id: impl Into<String>, payload: P) -> Self {
        Self {
            service: ShippingService::Email,
            carrier_row_id: carrier_row_id.into(),
            payload,
        }
    }

    pub fn osl_chats(carrier_row_id: impl Into<String>, payload: P) -> Self {
        Self {
            service: ShippingService::OslChats,
            carrier_row_id: carrier_row_id.into(),
            payload,
        }
    }

    pub fn x(carrier_row_id: impl Into<String>, payload: P) -> Self {
        Self {
            service: ShippingService::X,
            carrier_row_id: carrier_row_id.into(),
            payload,
        }
    }
}

/// The small, shared model behind the protected eye control.  It accepts only
/// rows the shipping job has already attributed to a real carrier and keeps
/// the 3504 record untouched, so email and first-party chat do not grow two
/// incompatible display formats.
#[cfg(feature = "core")]
#[derive(Default)]
pub struct ShippingProtectedOverlay {
    rows: Vec<crate::broker::ProtectedRowRecord>,
}

#[cfg(feature = "core")]
impl ShippingProtectedOverlay {
    pub fn matching_rows(&self) -> &[crate::broker::ProtectedRowRecord] {
        &self.rows
    }

    pub fn record_opened(
        &mut self,
        row: ArrivedMessageRow<crate::broker::ProtectedRowRecord>,
    ) -> Result<(), String> {
        if row.service == ShippingService::Discord
            || row.payload.app_id.is_empty()
            || row.payload.app_row_id != row.carrier_row_id
            || row.payload.cover_text.is_empty()
            || row.payload.private_words.is_empty()
            || !row.payload.opened
        {
            return Err("protected overlay received an invalid shipping record".to_owned());
        }
        if self.rows.iter().any(|existing| {
            existing.app_id == row.payload.app_id && existing.app_row_id == row.payload.app_row_id
        }) {
            return Err("protected overlay row was already received".to_owned());
        }
        self.rows.push(row.payload);
        Ok(())
    }

    pub fn closed_cover(&self, app_row_id: &str) -> Result<&str, String> {
        self.rows
            .iter()
            .find(|row| row.app_row_id == app_row_id)
            .map(|row| row.cover_text.as_str())
            .ok_or_else(|| "protected overlay row is unavailable".to_owned())
    }

    pub fn open_private_words(&self, app_row_id: &str) -> Result<&str, String> {
        self.rows
            .iter()
            .find(|row| row.app_row_id == app_row_id && row.opened)
            .map(|row| row.private_words.as_str())
            .ok_or_else(|| "protected overlay row is unavailable or unopened".to_owned())
    }
}

/// Test/QA instrumentation for the shipping seam.  It intentionally records
/// only service and carrier-row identity: it must never retain message bodies.
#[derive(Default, Debug, Eq, PartialEq)]
pub struct ShippingReceiveJournal {
    services: Vec<ShippingService>,
    carrier_row_ids: Vec<String>,
}

impl ShippingReceiveJournal {
    pub fn services(&self) -> &[ShippingService] {
        &self.services
    }

    pub fn carrier_row_ids(&self) -> &[String] {
        &self.carrier_row_ids
    }
}

/// The sole shipping receive job. Every arrived row enters here, irrespective
/// of carrier; adapters own shaping a row, while the supplied opener owns the
/// carrier-specific authenticated payload handling.
pub fn receive_arrived_message<P, F>(
    row: ArrivedMessageRow<P>,
    journal: &mut ShippingReceiveJournal,
    open: F,
) -> Result<(), String>
where
    F: FnOnce(ArrivedMessageRow<P>) -> Result<(), String>,
{
    journal.services.push(row.service);
    journal.carrier_row_ids.push(row.carrier_row_id.clone());
    open(row)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_records_the_calling_service_before_opening_the_row() {
        let mut journal = ShippingReceiveJournal::default();
        receive_arrived_message(
            ArrivedMessageRow::osl_chats("row-3505a", ()),
            &mut journal,
            |_| Ok(()),
        )
        .expect("arrival is opened by the one shipping job");
        assert_eq!(journal.services(), &[ShippingService::OslChats]);
        assert_eq!(journal.carrier_row_ids(), &["row-3505a"]);
    }
}
