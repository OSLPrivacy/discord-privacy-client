use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllowedPlaceRecord {
    pub app: String,
    pub account: String,
    pub kind: String,
    pub stable_id: String,
}

impl AllowedPlaceRecord {
    pub fn signal(
        account: impl Into<String>,
        kind: crate::auto_whitelist_rules::SignalWhitelistKind,
        place: impl Into<String>,
    ) -> Self {
        let account = account.into();
        let place = place.into();
        let kind_id = kind.allowed_place_kind();
        Self {
            app: "signal".to_owned(),
            stable_id: format!("signal:{account}:{kind_id}:{place}"),
            account,
            kind: kind_id.to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllowedPlaceQuery {
    pub app: String,
    pub account: String,
    pub kind: String,
    pub stable_id: String,
}

impl From<AllowedPlaceRecord> for AllowedPlaceQuery {
    fn from(record: AllowedPlaceRecord) -> Self {
        Self {
            app: record.app,
            account: record.account,
            kind: record.kind,
            stable_id: record.stable_id,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedAllowedPlaces {
    #[serde(default)]
    places: BTreeMap<String, AllowedPlaceRecord>,
}

impl SavedAllowedPlaces {
    pub fn save(&mut self, record: AllowedPlaceRecord) -> AllowedPlaceRecord {
        self.places.insert(record.stable_id.clone(), record.clone());
        record
    }

    pub fn query(&self, stable_id: &str) -> Option<AllowedPlaceQuery> {
        self.places.get(stable_id).cloned().map(Into::into)
    }
}

pub fn load_allowed_places(path: &Path) -> SavedAllowedPlaces {
    let Ok(blob) = std::fs::read(path) else {
        return SavedAllowedPlaces::default();
    };
    let plain = match crate::main_password::maybe_decrypt_file(path, &blob) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "OSL: load allowed_places.json decrypt failed");
            return SavedAllowedPlaces::default();
        }
    };
    serde_json::from_slice(&plain).unwrap_or_default()
}

pub fn write_allowed_places(path: &Path, places: &SavedAllowedPlaces) -> Result<(), String> {
    let body = serde_json::to_vec_pretty(places)
        .map_err(|e| format!("OSL: serialize allowed_places: {e}"))?;
    let out = crate::main_password::maybe_encrypt(&body)
        .map_err(|e| format!("OSL: encrypt allowed_places: {e}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &out).map_err(|e| format!("OSL: write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("OSL: rename {}: {e}", path.display()))?;
    Ok(())
}
