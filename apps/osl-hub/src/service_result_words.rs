//! Packaged Windows rendering for service-owned result codes.

use osl_english_catalogue::{EnglishCatalogue, ServiceResultEnvelope};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct ServiceResultFields {
    reason_code: String,
    #[serde(default)]
    parameters: BTreeMap<String, String>,
}

pub fn render_service_reason(
    reason_code: &str,
    parameters: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
) -> String {
    let result = ServiceResultEnvelope {
        reason_code: reason_code.to_owned(),
        parameters: parameters
            .into_iter()
            .map(|(name, value)| (name.into(), value.into()))
            .collect::<BTreeMap<_, _>>(),
    };
    match EnglishCatalogue::packaged("windows.service-result")
        .and_then(|catalogue| catalogue.resolve_service_result(&result))
    {
        Ok(resolved) => resolved.value,
        // Unknown codes must fail visibly; this is intentionally the strict
        // resolver error and never a friendly literal fallback.
        Err(error) => error.to_string(),
    }
}

pub fn render_service_reason_without_parameters(reason_code: &str) -> String {
    render_service_reason(reason_code, std::iter::empty::<(String, String)>())
}

/// Render the stable fields from a real service response. Other protocol
/// fields (ids, timestamps, status flags) are intentionally ignored; English
/// fallback fields are never consulted.
pub fn render_service_response_body(body: &str) -> Option<String> {
    let wire = serde_json::from_str::<ServiceResultFields>(body).ok()?;
    Some(render_service_reason(
        wire.reason_code.as_str(),
        wire.parameters,
    ))
}
