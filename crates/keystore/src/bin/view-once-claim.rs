use keystore::{identity_from_entropy, KeyServerClient};

fn main() {
    match run() {
        Ok(line) => println!("{line}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<String, String> {
    let mut keyserver_url = None;
    let mut recipient_id = None;
    let mut content_id = None;
    let mut entropy_hex = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let slot = match arg.as_str() {
            "--keyserver-url" => &mut keyserver_url,
            "--recipient-id" => &mut recipient_id,
            "--content-id" => &mut content_id,
            "--identity-entropy-hex" => &mut entropy_hex,
            other => return Err(format!("OSL: unknown argument: {other}")),
        };
        *slot = Some(
            args.next()
                .ok_or_else(|| format!("OSL: missing value for {arg}"))?,
        );
    }

    let keyserver_url = keyserver_url.ok_or("OSL: missing --keyserver-url")?;
    let recipient_id = recipient_id.ok_or("OSL: missing --recipient-id")?;
    let content_id = content_id.ok_or("OSL: missing --content-id")?;
    let entropy = parse_entropy(&entropy_hex.ok_or("OSL: missing --identity-entropy-hex")?)?;
    let identity = identity_from_entropy(entropy, recipient_id.clone());
    let client = KeyServerClient::new(&keyserver_url).map_err(|error| error.to_string())?;
    let claimed = client
        .claim_wrapped_key_opened(&identity, &content_id)
        .map_err(|error| format!("OSL: view-once claim failed: {error}"))?;
    if !claimed.opened || claimed.content_id != content_id {
        return Err("OSL: view-once claim response did not match the record".to_owned());
    }
    Ok(format!(
        "OSL: view-once claim opened=true content_id={}",
        claimed.content_id
    ))
}

fn parse_entropy(value: &str) -> Result<[u8; 16], String> {
    if value.len() != 32 {
        return Err("OSL: --identity-entropy-hex must be 16 bytes".to_owned());
    }
    let mut out = [0u8; 16];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        let text =
            std::str::from_utf8(chunk).map_err(|_| "OSL: entropy hex is invalid".to_owned())?;
        out[index] =
            u8::from_str_radix(text, 16).map_err(|_| "OSL: entropy hex is invalid".to_owned())?;
    }
    Ok(out)
}
