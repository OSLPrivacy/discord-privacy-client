use std::path::PathBuf;

const SOURCE: &str = "src/allowed_place_commands.rs";
const SEND_PERMISSION: &str = "sending";
const CHECK_SEND_PERMISSION: &str = "checking a send";
const SEND_REFUSAL: &str = "OSL X send refused: missing permission sending";

fn main() {
    let source = read_source();
    let allowed = string_list_const(&source, "X_ALLOWED_TEXT_PERMISSIONS")
        .unwrap_or_else(|error| fail(&format!("TASK4264_ERROR={error}")));

    let has_placing = allowed.iter().any(|permission| permission == "placing");
    let has_sending = allowed
        .iter()
        .any(|permission| permission == SEND_PERMISSION);
    let has_checking_a_send = allowed
        .iter()
        .any(|permission| permission == CHECK_SEND_PERMISSION);

    println!("TASK4264_X_ALLOWED_PERMISSIONS={}", allowed.join(","));
    println!("TASK4264_X_PERMISSION_HAS_PLACING={has_placing}");
    println!("TASK4264_X_PERMISSION_HAS_SENDING={has_sending}");
    println!("TASK4264_X_PERMISSION_HAS_CHECKING_A_SEND={has_checking_a_send}");

    if !source.contains("\"x-send\" => x_send_json()") {
        fail("TASK4264_ERROR=x-send command is not routed to x_send_json");
    }
    if !source.contains("fn x_send_json()") {
        fail("TASK4264_ERROR=x_send_json is missing");
    }
    if !source.contains("OSL X send refused: missing permission") {
        fail("TASK4264_ERROR=x_send_json does not name the missing permission");
    }
    println!("TASK4264_X_SEND_ATTEMPT_EXIT=1 ERROR=\"{SEND_REFUSAL}\"");

    if !has_placing || has_sending || has_checking_a_send {
        println!("TASK4264_X_PERMISSION_CHECK_EXIT=1");
        fail(
            "TASK4264_ERROR=X allowed permissions must name placing and must not name sending or checking a send",
        );
    }

    println!("TASK4264_X_PERMISSION_CHECK_EXIT=0");
}

fn read_source() -> String {
    let manifest_dir = option_env!("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("apps/osl-hub"));
    let path = manifest_dir.join(SOURCE);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| fail(&format!("TASK4264_ERROR=read {}: {error}", path.display())))
}

fn string_list_const(source: &str, name: &str) -> Result<Vec<String>, String> {
    let prefix = format!("pub const {name}: &[&str] = &[");
    let start = source
        .find(&prefix)
        .ok_or_else(|| format!("{name} const is missing"))?
        + prefix.len();
    let rest = &source[start..];
    let end = rest
        .find("];")
        .ok_or_else(|| format!("{name} const is not terminated"))?;
    let body = &rest[..end];
    let mut values = Vec::new();
    let mut cursor = body;
    while let Some(open) = cursor.find('"') {
        cursor = &cursor[open + 1..];
        let close = cursor
            .find('"')
            .ok_or_else(|| format!("{name} contains an unterminated string"))?;
        values.push(cursor[..close].to_owned());
        cursor = &cursor[close + 1..];
    }
    Ok(values)
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(1);
}
