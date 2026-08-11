use osl_english_catalogue::{
    EnglishCatalogue, ServiceResultEnvelope, PACKAGED_ENGLISH_CATALOGUE, SERVICE_REASON_CODES,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "../../../apps/osl-hub/src/service_result_words.rs"]
mod windows_service_result_words;

#[derive(Debug, Deserialize)]
struct Inventory {
    routes: Vec<Route>,
}

#[derive(Debug, Deserialize)]
struct Route {
    method: String,
    route: String,
    classification: String,
    kind: String,
    constructor: String,
}

#[derive(Debug, Deserialize)]
struct Traffic {
    observations: Vec<Observation>,
}

#[derive(Debug, Deserialize)]
struct Observation {
    method: String,
    route: String,
    body: Value,
    client_entry_point: String,
    resolver_calls: u32,
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fail(route: &str, field: &str, detail: impl AsRef<str>) -> String {
    format!(
        "TASK5213b deployed_route=\"{route}\" field={field}: {}",
        detail.as_ref()
    )
}

fn load_traffic(path: Option<&Path>) -> Result<Traffic, String> {
    let bytes = if let Some(path) = path {
        fs::read(path).map_err(|e| format!("traffic read: {e}"))?
    } else {
        let output = Command::new("node")
            .arg(root().join("scripts/task-5213-runtime.mjs"))
            .current_dir(root())
            .output()
            .map_err(|e| format!("runtime traffic launch: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "runtime traffic exited {:?}: {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        output.stdout
    };
    serde_json::from_slice(&bytes).map_err(|e| format!("runtime traffic JSON: {e}"))
}

fn deployed_inventory() -> Result<Inventory, String> {
    let output = Command::new("node")
        .arg(root().join("scripts/generate-task-5213-inventory.mjs"))
        .current_dir(root())
        .output()
        .map_err(|e| format!("deployed inventory launch: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "deployed inventory exited {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("deployed inventory JSON: {e}"))
}

fn object_has_forbidden_person_field(body: &Value) -> Option<String> {
    fn visit(value: &Value, prefix: &str) -> Option<String> {
        match value {
            Value::Object(object) => {
                for (name, child) in object {
                    let path = if prefix.is_empty() {
                        name.clone()
                    } else {
                        format!("{prefix}.{name}")
                    };
                    if matches!(
                        name.as_str(),
                        "error" | "message" | "detail" | "description" | "hint" | "text"
                    ) {
                        return Some(path);
                    }
                    if let Some(found) = visit(child, &path) {
                        return Some(found);
                    }
                }
                None
            }
            Value::Array(items) => items
                .iter()
                .enumerate()
                .find_map(|(index, child)| visit(child, &format!("{prefix}[{index}]"))),
            _ => None,
        }
    }
    visit(body, "")
}

fn expected_key(code: &str) -> Result<&'static str, String> {
    let found: Vec<_> = SERVICE_REASON_CODES
        .iter()
        .filter(|spec| spec.reason_code == code)
        .collect();
    if found.len() != 1 {
        return Err(format!(
            "reason_code {code:?} maps to {} catalogue keys",
            found.len()
        ));
    }
    Ok(found[0].catalogue_key)
}

pub fn check(
    inventory_path: Option<&Path>,
    traffic_path: Option<&Path>,
    catalogue_path: Option<&Path>,
) -> Result<String, String> {
    let inventory_path = inventory_path
        .map(PathBuf::from)
        .unwrap_or_else(|| root().join("contracts/service-person-results.generated.json"));
    let inventory: Inventory = serde_json::from_slice(
        &fs::read(&inventory_path).map_err(|e| format!("inventory read: {e}"))?,
    )
    .map_err(|e| format!("inventory JSON: {e}"))?;
    let deployed_inventory = deployed_inventory()?;

    let mut inventory_keys = BTreeMap::new();
    for route in &inventory.routes {
        let name = format!("{} {}", route.method, route.route);
        if inventory_keys.insert(name.clone(), route).is_some() {
            return Err(fail(&name, "inventory.route", "duplicate"));
        }
        if !matches!(
            route.kind.as_str(),
            "relay" | "key_server" | "storage" | "payment_voucher"
        ) {
            return Err(fail(&name, "kind", &route.kind));
        }
        if !root().join(&route.constructor).is_file() {
            return Err(fail(&name, "constructor", &route.constructor));
        }
    }
    let deployed_count = deployed_inventory.routes.len();
    for deployed_route in &deployed_inventory.routes {
        let name = format!("{} {}", deployed_route.method, deployed_route.route);
        let route = inventory_keys
            .get(&name)
            .ok_or_else(|| fail(&name, "inventory.route", "omitted"))?;
        if route.classification != deployed_route.classification {
            return Err(fail(
                &name,
                "classification",
                format!(
                    "expected {}, got {}",
                    deployed_route.classification, route.classification
                ),
            ));
        }
        if route.kind != deployed_route.kind {
            return Err(fail(&name, "kind", &route.kind));
        }
    }
    if inventory_keys.len() != deployed_count {
        let unexpected = inventory_keys
            .keys()
            .find(|name| {
                !deployed_inventory
                    .routes
                    .iter()
                    .any(|route| name.as_str() == format!("{} {}", route.method, route.route))
            })
            .cloned()
            .unwrap_or_else(|| "<unknown>".to_owned());
        return Err(fail(&unexpected, "inventory.route", "unexpected"));
    }

    let key_index = fs::read_to_string(root().join("keyserver-cf/src/index.ts"))
        .map_err(|e| format!("keyserver dispatch read: {e}"))?;
    let store_index = fs::read_to_string(root().join("cipher-store-cf/src/index.ts"))
        .map_err(|e| format!("storage dispatch read: {e}"))?;
    for (name, source) in [
        ("keyserver-cf", key_index.as_str()),
        ("cipher-store-cf", store_index.as_str()),
    ] {
        if !source.contains("adaptPersonFacingResponse(request, await dispatch")
            || !source.contains("adaptPersonFacingResponse(request, serverError")
        {
            return Err(fail(
                name,
                "response_adapter",
                "not wired around deployed dispatch",
            ));
        }
    }
    for (path, required) in [
        (
            "apps/osl-hub/src/main.rs",
            "fn resolve_person_service_result(",
        ),
        (
            "apps/osl-hub/src/hub_command_surface.rs",
            "resolve_person_service_result,",
        ),
        (
            "apps/osl-hub/permissions/hub.toml",
            "commands.allow = [\"resolve_person_service_result\"]",
        ),
        (
            "apps/osl-hub/capabilities/hub.json",
            "allow-resolve-person-service-result",
        ),
    ] {
        let source = fs::read_to_string(root().join(path))
            .map_err(|error| format!("packaged client source {path}: {error}"))?;
        if !source.contains(required) {
            return Err(fail(
                "windows.packaged-client",
                "client_entry_point",
                format!("{path} missing {required}"),
            ));
        }
    }

    let traffic = load_traffic(traffic_path)?;
    let catalogue_source = if let Some(path) = catalogue_path {
        fs::read_to_string(path).map_err(|e| format!("catalogue read: {e}"))?
    } else {
        PACKAGED_ENGLISH_CATALOGUE.to_owned()
    };
    let catalogue = EnglishCatalogue::load(&catalogue_source, "windows.packaged-runtime")
        .map_err(|e| fail("<catalogue>", "catalogue.fallback", e.to_string()))?;
    let reason_code_set: BTreeSet<_> = SERVICE_REASON_CODES
        .iter()
        .map(|spec| spec.reason_code)
        .collect();
    let service_key_set: BTreeSet<_> = SERVICE_REASON_CODES
        .iter()
        .map(|spec| spec.catalogue_key)
        .collect();
    if reason_code_set.len() != SERVICE_REASON_CODES.len()
        || service_key_set.len() != SERVICE_REASON_CODES.len()
    {
        return Err(fail(
            "<catalogue>",
            "catalogue_key",
            "service reason codes are not one-to-one with catalogue keys",
        ));
    }
    for spec in SERVICE_REASON_CODES {
        let parameters = if spec.reason_code == "relay_recipient_inbox_full" {
            BTreeMap::from([("scope".to_owned(), "recipient".to_owned())])
        } else {
            BTreeMap::new()
        };
        let resolved = catalogue
            .resolve_service_result(&ServiceResultEnvelope {
                reason_code: spec.reason_code.to_owned(),
                parameters,
            })
            .map_err(|error| fail("<catalogue>", "catalogue_key", error.to_string()))?;
        if resolved.key != spec.catalogue_key {
            return Err(fail(
                "<catalogue>",
                "catalogue_key",
                format!("{} resolved as {}", spec.reason_code, resolved.key),
            ));
        }
    }
    let mut rendered = Vec::new();
    let mut codes = BTreeSet::new();
    let person_routes: BTreeSet<String> = inventory
        .routes
        .iter()
        .filter(|route| route.classification == "person")
        .map(|route| format!("{} {}", route.method, route.route))
        .collect();

    for observation in &traffic.observations {
        let deployed = format!("{} {}", observation.method, observation.route);
        let normalized = normalize_runtime_route(&deployed);
        if !person_routes.contains(&normalized) {
            return Err(fail(
                &deployed,
                "classification",
                "runtime person traffic is not classified person",
            ));
        }
        if let Some(field) = object_has_forbidden_person_field(&observation.body) {
            return Err(fail(
                &deployed,
                &field,
                "person-facing English schema field",
            ));
        }
        let reason_code = observation
            .body
            .get("reason_code")
            .and_then(Value::as_str)
            .ok_or_else(|| fail(&deployed, "reason_code", "missing stable code"))?;
        if !reason_code
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            || reason_code.contains(' ')
        {
            return Err(fail(&deployed, "reason_code", reason_code));
        }
        let parameters = observation
            .body
            .get("parameters")
            .and_then(Value::as_object)
            .ok_or_else(|| fail(&deployed, "parameters", "missing object"))?;
        let parameters = parameters
            .iter()
            .map(|(k, v)| {
                v.as_str()
                    .map(|v| (k.clone(), v.to_owned()))
                    .ok_or_else(|| fail(&deployed, &format!("parameters.{k}"), "must be a string"))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        if observation.resolver_calls != 1 {
            return Err(fail(
                &deployed,
                "resolver_calls",
                observation.resolver_calls.to_string(),
            ));
        }
        if observation.client_entry_point != "windows.resolve_person_service_result" {
            return Err(fail(
                &deployed,
                "client_entry_point",
                &observation.client_entry_point,
            ));
        }
        let expected = expected_key(reason_code).map_err(|e| fail(&deployed, "reason_code", e))?;
        let resolved = catalogue
            .resolve_service_result(&ServiceResultEnvelope {
                reason_code: reason_code.to_owned(),
                parameters: parameters.clone(),
            })
            .map_err(|e| fail(&deployed, "reason_code", e.to_string()))?;
        if resolved.key != expected {
            return Err(fail(
                &deployed,
                "catalogue_key",
                format!("expected {expected}, got {}", resolved.key),
            ));
        }
        if resolved.fallback.is_some() {
            return Err(fail(&deployed, "fallback", "literal fallback enabled"));
        }
        let production_value =
            windows_service_result_words::render_service_reason(reason_code, parameters.clone());
        if production_value != resolved.value {
            return Err(fail(
                &deployed,
                "client_entry_point",
                "shipping Windows renderer disagrees with strict resolver",
            ));
        }
        codes.insert(reason_code.to_owned());
        rendered.push((reason_code.to_owned(), resolved.value));
    }

    let observed_routes: BTreeSet<_> = traffic
        .observations
        .iter()
        .map(|observation| {
            normalize_runtime_route(&format!("{} {}", observation.method, observation.route))
        })
        .collect();
    for person_route in &person_routes {
        if !observed_routes.contains(person_route) {
            return Err(fail(person_route, "runtime.traffic", "route unobserved"));
        }
    }

    let escaped = catalogue
        .resolve_service_result(&ServiceResultEnvelope {
            reason_code: "relay_recipient_inbox_full".to_owned(),
            parameters: BTreeMap::from([("scope".to_owned(), "<b>&\"'".to_owned())]),
        })
        .map_err(|e| e.to_string())?;
    if escaped.value.contains("<b>") || !escaped.value.contains("&lt;b&gt;&amp;&quot;&#39;") {
        return Err(fail(
            "POST /v1/control-inbox",
            "parameters.scope",
            "not HTML escaped",
        ));
    }

    let unknown = catalogue
        .resolve_service_result(&ServiceResultEnvelope {
            reason_code: "unknown_from_service".to_owned(),
            parameters: BTreeMap::new(),
        })
        .unwrap_err();
    if unknown.fallback() != "disabled" || !unknown.to_string().contains("unknown_from_service") {
        return Err(fail(
            "<runtime>",
            "reason_code",
            "unknown code did not fail visibly",
        ));
    }

    let changed_source = catalogue_source
        .replace("Sent privately.", "Sent privately [changed].")
        .replace(
            "Encrypted storage is temporarily full.",
            "Encrypted storage is temporarily full [changed].",
        )
        .replace(
            "Your activation code is active.",
            "Your activation code is active [changed].",
        );
    let changed_catalogue = EnglishCatalogue::load(&changed_source, "windows.packaged-runtime")
        .map_err(|e| e.to_string())?;
    let traffic_again = load_traffic(traffic_path)?;
    let mut changed_results = 0usize;
    for (index, observation) in traffic_again.observations.iter().enumerate() {
        let code = observation.body["reason_code"].as_str().unwrap();
        let params = observation.body["parameters"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
            .collect::<BTreeMap<_, _>>();
        let value = changed_catalogue
            .resolve_service_result(&ServiceResultEnvelope {
                reason_code: code.to_owned(),
                parameters: params,
            })
            .map_err(|e| e.to_string())?
            .value;
        if value != rendered[index].1 {
            changed_results += 1;
        }
    }
    if changed_results != 3 {
        return Err(fail(
            "<runtime>",
            "catalogue.change_count",
            format!("expected 3, got {changed_results}"),
        ));
    }

    let storage_rendered = SERVICE_REASON_CODES
        .iter()
        .filter(|spec| {
            spec.reason_code.starts_with("storage_upload_")
                || spec.reason_code.starts_with("storage_fetch_")
                || spec.reason_code.starts_with("storage_delete_")
        })
        .count();
    if storage_rendered != 36 {
        return Err(fail(
            "<runtime>",
            "storage.results",
            storage_rendered.to_string(),
        ));
    }

    Ok(format!(
        "TASK5213 PASS deployed_routes={} person_routes={} runtime_results={} reason_codes={} catalogue_keys={} resolver_calls={} escaped_parameters=1 english_schema_fields=0 unknown_fallback=disabled storage_rendered_results={} changed_keys=3 changed_results={}",
        deployed_count,
        person_routes.len(),
        traffic.observations.len(),
        codes.len(),
        SERVICE_REASON_CODES.len(),
        traffic.observations.len(),
        storage_rendered,
        changed_results,
    ))
}

fn normalize_runtime_route(value: &str) -> String {
    let mut parts = value.splitn(2, ' ');
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");
    let normalized = if path.starts_with("/v/") && path.ends_with("/fetch") {
        "/v/:id/fetch".to_owned()
    } else if path.starts_with("/v1/link/") && path.ends_with("/status") {
        "/v1/link/:id/status".to_owned()
    } else if path.starts_with("/v1/link/") {
        "/v1/link/:id".to_owned()
    } else if path.starts_with("/v1/blob/") && path.ends_with("/ack") {
        "/v1/blob/:id/ack".to_owned()
    } else if path.starts_with("/v1/blob/") {
        "/v1/blob/:id".to_owned()
    } else if path.starts_with("/v1/attachment/") && path.contains("/part/") {
        "/v1/attachment/:id/part/:part".to_owned()
    } else if path.starts_with("/v1/attachment/") && path.ends_with("/complete") {
        "/v1/attachment/:id/complete".to_owned()
    } else if path.starts_with("/v1/attachment/") && path != "/v1/attachment/session" {
        "/v1/attachment/:id".to_owned()
    } else {
        path.to_owned()
    };
    format!("{method} {normalized}")
}
