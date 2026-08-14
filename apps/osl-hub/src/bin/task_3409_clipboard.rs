use osl_privacy_hub::protected_clipboard::{
    write_finished_cover_text_to_clipboard, FinishedCoverText,
};
use sha2::{Digest, Sha256};
use std::env;
use std::process::{Command, Stdio};

const DEFAULT_PRIVATE_TEXT: &str = "SECRET3409NEVERCLIP";

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find_map(|window| (window[0] == name).then(|| window[1].clone()))
}

fn private_words(private_text: &str) -> Vec<String> {
    private_text
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| word.to_ascii_lowercase())
        .collect()
}

fn count_private_word_hits(clipboard: &str, private_text: &str) -> usize {
    let haystack = clipboard.to_ascii_lowercase();
    private_words(private_text)
        .iter()
        .map(|word| haystack.matches(word).count())
        .sum()
}

fn finished_cover_text(private_text: &str) -> Result<FinishedCoverText, String> {
    let digest = Sha256::digest(private_text.as_bytes());
    let mut pointer = [0u8; stego::TOKEN_ID_BYTES];
    pointer.copy_from_slice(&digest[..stego::TOKEN_ID_BYTES]);

    let detector_key = Sha256::digest(b"OSL task 3409 protected clipboard detector");
    let cipher = stego::ConversationCipher::from_salt(b"OSL task 3409 protected clipboard");
    FinishedCoverText::new(stego::encode_token(&cipher, &detector_key, &pointer))
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn read_clipboard_text() -> Result<String, String> {
    if let Some(value) = command_output("wl-paste", &["--no-newline"]) {
        return Ok(value);
    }
    if let Some(value) = command_output("xclip", &["-selection", "clipboard", "-o"]) {
        return Ok(value);
    }
    if let Some(value) = command_output("xsel", &["--clipboard", "--output"]) {
        return Ok(value);
    }
    if let Some(value) = command_output("pbpaste", &[]) {
        return Ok(value);
    }
    if let Some(value) = command_output(
        "powershell.exe",
        &["-NoProfile", "-Command", "Get-Clipboard -Raw"],
    ) {
        return Ok(value);
    }
    if let Some(value) = command_output("pwsh", &["-NoProfile", "-Command", "Get-Clipboard -Raw"]) {
        return Ok(value);
    }
    Err("no clipboard reader returned text".to_owned())
}

fn main() {
    let args = env::args().collect::<Vec<_>>();
    let private_text =
        arg_value(&args, "--private-text").unwrap_or_else(|| DEFAULT_PRIVATE_TEXT.to_owned());
    let cover_text = finished_cover_text(&private_text).unwrap_or_else(|error| {
        eprintln!("cover_error={error}");
        std::process::exit(2);
    });

    let break_copy_private = args.iter().any(|arg| arg == "--break-copy-private");
    let write_result = if break_copy_private {
        osl_privacy_hub::invite_clipboard::write_desktop_clipboard_text(&private_text)
    } else {
        write_finished_cover_text_to_clipboard(&cover_text)
    };
    if let Err(error) = write_result {
        eprintln!("clipboard_write_error={error}");
        std::process::exit(2);
    }

    let clipboard = read_clipboard_text().unwrap_or_else(|error| {
        eprintln!("clipboard_read_error={error}");
        std::process::exit(2);
    });
    let private_hit_count = count_private_word_hits(&clipboard, &private_text);

    println!("private_text={private_text}");
    println!("cover_text={}", cover_text.as_str());
    println!("clipboard={clipboard}");
    println!(
        "clipboard_equals_cover_text={}",
        clipboard == cover_text.as_str()
    );
    println!("clipboard_private_word_hits={private_hit_count}");

    if break_copy_private {
        std::process::exit(if private_hit_count == 0 { 1 } else { 10 });
    }
    if clipboard != cover_text.as_str() || private_hit_count != 0 {
        std::process::exit(1);
    }
}
