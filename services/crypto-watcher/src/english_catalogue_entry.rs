//! Shipping service entry point for the one English catalogue.

use osl_english_catalogue::{CatalogueError, EnglishCatalogue};

pub const SERVICE_CATALOGUE_CALLER: &str = "service.crypto-watcher.startup";

pub fn load_packaged_service_catalogue() -> Result<EnglishCatalogue, CatalogueError> {
    let catalogue = EnglishCatalogue::packaged(SERVICE_CATALOGUE_CALLER)?;
    catalogue.resolve(
        "service.catalogue.loaded",
        [
            ("caller", catalogue.caller()),
            ("version", catalogue.version()),
        ],
    )?;
    Ok(catalogue)
}

/// Loads external bytes through the exact service production entry point used
/// at process startup.
pub fn load_external_service_catalogue(source: &str) -> Result<EnglishCatalogue, CatalogueError> {
    EnglishCatalogue::load(source, SERVICE_CATALOGUE_CALLER)
}
