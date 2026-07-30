//! Deny-by-default WebAssembly command extensions stored in the encrypted asset vault.

use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read};
use wasmi::{Config, Engine, Linker, Module, Store, StoreLimitsBuilder};
use zeroize::Zeroize;

const MAX_PACKAGE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_MODULE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_MEMORY_BYTES: usize = 32 * 1024 * 1024;
const EXECUTION_FUEL: u64 = 2_000_000;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OslPluginManifest {
    pub manifest_version: u8,
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub kind: String,
    pub permissions: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OslPluginInspection {
    pub manifest: OslPluginManifest,
    pub entrypoint: &'static str,
    pub memory_limit_bytes: usize,
    pub fuel_limit: u64,
    pub ambient_access: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OslPluginRunReceipt {
    pub result: i64,
    pub fuel_consumed: u64,
    pub memory_limit_bytes: usize,
    pub ambient_access: bool,
}

pub fn inspect(asset_id: &str) -> Result<OslPluginInspection, String> {
    let (manifest, mut module) = package(asset_id)?;
    validate_module(&module)?;
    module.zeroize();
    Ok(OslPluginInspection {
        manifest,
        entrypoint: "osl_run",
        memory_limit_bytes: MAX_MEMORY_BYTES,
        fuel_limit: EXECUTION_FUEL,
        ambient_access: false,
    })
}

pub fn run(asset_id: &str, input: i64) -> Result<OslPluginRunReceipt, String> {
    let (_manifest, mut bytes) = package(asset_id)?;
    let result = execute_module(&bytes, input);
    bytes.zeroize();
    result
}

fn execute_module(bytes: &[u8], input: i64) -> Result<OslPluginRunReceipt, String> {
    let engine = engine()?;
    let module =
        Module::new(&engine, bytes).map_err(|_| "The extension module is invalid".to_owned())?;
    if module.imports().next().is_some() {
        return Err("This extension requests undeclared host imports; OSL command extensions currently receive no ambient APIs".into());
    }
    let limits = StoreLimitsBuilder::new()
        .memory_size(MAX_MEMORY_BYTES)
        .memories(1)
        .tables(1)
        .instances(1)
        .table_elements(10_000)
        .trap_on_grow_failure(true)
        .build();
    let mut store = Store::new(&engine, limits);
    store.limiter(|limits| limits);
    store
        .set_fuel(EXECUTION_FUEL)
        .map_err(|_| "The extension execution budget could not be applied".to_owned())?;
    let instance = Linker::new(&engine)
        .instantiate_and_start(&mut store, &module)
        .map_err(|_| "The extension could not start inside its sandbox".to_owned())?;
    let function = instance
        .get_typed_func::<i64, i64>(&store, "osl_run")
        .map_err(|_| "The extension must export osl_run(i64) -> i64".to_owned())?;
    let result = function
        .call(&mut store, input)
        .map_err(|_| "The extension trapped or exhausted its execution budget".to_owned())?;
    if result.unsigned_abs() > 9_007_199_254_740_991 {
        return Err("The extension returned an integer outside OSL's portable safe range".into());
    }
    let remaining = store.get_fuel().unwrap_or(0);
    Ok(OslPluginRunReceipt {
        result,
        fuel_consumed: EXECUTION_FUEL.saturating_sub(remaining),
        memory_limit_bytes: MAX_MEMORY_BYTES,
        ambient_access: false,
    })
}

fn engine() -> Result<Engine, String> {
    let mut config = Config::default();
    config.consume_fuel(true);
    Ok(Engine::new(&config))
}

fn validate_module(bytes: &[u8]) -> Result<(), String> {
    let engine = engine()?;
    let module =
        Module::new(&engine, bytes).map_err(|_| "The extension module is invalid".to_owned())?;
    if module.imports().next().is_some() {
        return Err("Extensions with host, WASI, filesystem, network, process, clock, random, or environment imports are denied".into());
    }
    Ok(())
}

fn package(asset_id: &str) -> Result<(OslPluginManifest, Vec<u8>), String> {
    let (asset, mut package) = crate::osl_assets::read_bounded(asset_id, MAX_PACKAGE_BYTES)?;
    if !asset.name.to_ascii_lowercase().ends_with(".oslmod") {
        package.zeroize();
        return Err("OSL extensions must use a .oslmod package".into());
    }
    let result = read_package(&package);
    package.zeroize();
    result
}

fn read_entry<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
    maximum: u64,
) -> Result<Vec<u8>, String> {
    let file = archive
        .by_name(name)
        .map_err(|_| format!("The extension package is missing {name}"))?;
    if file.size() > maximum {
        return Err(format!("The extension {name} exceeds its sandbox limit"));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(file.size()).unwrap_or(0));
    file.take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| format!("The extension {name} could not be read"))?;
    if bytes.len() as u64 > maximum {
        bytes.zeroize();
        return Err(format!("The extension {name} exceeds its sandbox limit"));
    }
    Ok(bytes)
}

fn read_package(bytes: &[u8]) -> Result<(OslPluginManifest, Vec<u8>), String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| "The extension package is not a valid ZIP container".to_owned())?;
    if archive.len() > 16 {
        return Err("The extension package contains too many files".into());
    }
    let mut manifest_bytes = read_entry(&mut archive, "manifest.json", MAX_MANIFEST_BYTES)?;
    let manifest = serde_json::from_slice::<OslPluginManifest>(&manifest_bytes)
        .map_err(|_| "The extension manifest is malformed".to_owned())?;
    manifest_bytes.zeroize();
    validate_manifest(&manifest)?;
    let module = read_entry(&mut archive, "module.wasm", MAX_MODULE_BYTES)?;
    Ok((manifest, module))
}

fn validate_manifest(manifest: &OslPluginManifest) -> Result<(), String> {
    if manifest.manifest_version != 1
        || !valid_id(&manifest.id)
        || !valid_version(&manifest.version)
        || manifest.name.is_empty()
        || manifest.name.chars().count() > 80
        || manifest.description.chars().count() > 240
        || manifest.kind != "command-pack"
        || manifest.permissions != ["ui:command"]
    {
        return Err("This extension manifest is invalid or requests capabilities unavailable to sandboxed command packs".into());
    }
    Ok(())
}

fn valid_id(value: &str) -> bool {
    (3..=64).contains(&value.len())
        && value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || index > 0 && matches!(byte, b'.' | b'-')
        })
}

fn valid_version(value: &str) -> bool {
    let (core, suffix) = value
        .split_once('-')
        .map_or((value, None), |(core, suffix)| (core, Some(suffix)));
    core.split('.').count() == 3
        && core
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        && suffix.is_none_or(|suffix| !suffix.is_empty())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_contract_allows_only_no_ambient_command_extensions() {
        let manifest = OslPluginManifest {
            manifest_version: 1,
            id: "org.example.counter".into(),
            name: "Counter".into(),
            version: "1.0.0".into(),
            description: "Pure command".into(),
            kind: "command-pack".into(),
            permissions: vec!["ui:command".into()],
        };
        assert!(validate_manifest(&manifest).is_ok());
        let mut denied = manifest.clone();
        denied.permissions.push("assets:read-selected".into());
        assert!(validate_manifest(&denied).is_err());
    }

    #[test]
    fn malformed_modules_are_rejected_before_execution() {
        assert!(validate_module(b"not wasm").is_err());
    }

    #[test]
    fn executes_the_bounded_pure_command_abi() {
        let module = [
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x06, 0x01, 0x60, 0x01, 0x7e,
            0x01, 0x7e, 0x03, 0x02, 0x01, 0x00, 0x07, 0x0b, 0x01, 0x07, b'o', b's', b'l', b'_',
            b'r', b'u', b'n', 0x00, 0x00, 0x0a, 0x09, 0x01, 0x07, 0x00, 0x20, 0x00, 0x42, 0x01,
            0x7c, 0x0b,
        ];
        let receipt = execute_module(&module, 41).unwrap();
        assert_eq!(receipt.result, 42);
        assert!(receipt.fuel_consumed > 0);
        assert!(!receipt.ambient_access);
    }
}
