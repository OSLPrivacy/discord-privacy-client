//! Shipping Windows entry point for the one English catalogue.

use osl_english_catalogue::{CatalogueError, EnglishCatalogue};

pub const WINDOWS_CATALOGUE_CALLER: &str = "windows.desktop.startup";

pub fn load_packaged_windows_catalogue() -> Result<EnglishCatalogue, CatalogueError> {
    let catalogue = EnglishCatalogue::packaged(WINDOWS_CATALOGUE_CALLER)?;
    catalogue.resolve(
        "windows.catalogue.loaded",
        [
            ("caller", catalogue.caller()),
            ("version", catalogue.version()),
        ],
    )?;
    Ok(catalogue)
}

/// Loads externally supplied bytes through the exact Windows production entry
/// point. Release acceptance uses this to prove a packaged copy cannot become
/// partial or permissive before rendering.
pub fn load_external_windows_catalogue(source: &str) -> Result<EnglishCatalogue, CatalogueError> {
    EnglishCatalogue::load(source, WINDOWS_CATALOGUE_CALLER)
}
