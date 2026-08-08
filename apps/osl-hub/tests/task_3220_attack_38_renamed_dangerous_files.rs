//! TASK 3220 / attack 38: dangerous bytes renamed to harmless extensions stay
//! inert in the attachment tray and travel only through the opaque byte path.
//!
//! `TASK3220_FIXTURE` deliberately selects the manifest used by the exact same
//! check. The saved four-kind manifest must therefore turn this test red before
//! any zero-side-effect result can be accepted.

#![cfg(all(feature = "core", target_os = "linux"))]

use crypto::aead;
use osl_privacy_hub::attachment_formats::accepted_attachment_mime;
use osl_privacy_hub::osl_chat_drag_drop::OslChatAttachmentTray;
use osl_privacy_hub::peer_attachment_io;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;

const DEFAULT_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/task_3220/all_five.json"
);

const REQUIRED_KINDS: [DangerousKind; 5] = [
    DangerousKind::Program,
    DangerousKind::Archive,
    DangerousKind::DiskImage,
    DangerousKind::Script,
    DangerousKind::BrokenPicture,
];

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(rename_all = "snake_case")]
enum DangerousKind {
    Program,
    Archive,
    DiskImage,
    Script,
    BrokenPicture,
}

impl DangerousKind {
    fn label(self) -> &'static str {
        match self {
            Self::Program => "program",
            Self::Archive => "archive",
            Self::DiskImage => "disk_image",
            Self::Script => "script",
            Self::BrokenPicture => "broken_picture",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FixtureManifest {
    files: Vec<FixtureSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FixtureSpec {
    kind: DangerousKind,
    original_name: String,
    renamed_name: String,
}

struct MaterializedFile {
    kind: DangerousKind,
    path: PathBuf,
    bytes: Vec<u8>,
}

#[test]
fn task_3220_renamed_dangerous_files_stay_inert_and_send_as_plain_bytes() {
    let fixture_path = std::env::var_os("TASK3220_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_FIXTURE));
    let specs = load_and_validate_manifest(&fixture_path).unwrap_or_else(|error| panic!("{error}"));
    let root = tempfile::Builder::new()
        .prefix("osl-task-3220-")
        .tempdir()
        .expect("create isolated attack root");
    let files =
        materialize_renamed_files(root.path(), &specs).unwrap_or_else(|error| panic!("{error}"));

    let detected = files
        .iter()
        .map(|file| detect_dangerous_kind(&file.bytes))
        .collect::<Result<BTreeSet<_>, _>>()
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(detected, REQUIRED_KINDS.into_iter().collect());

    let process_list_before = descendant_processes();
    let mut tray = OslChatAttachmentTray::default();
    let receipt = tray
        .accept_dropped_files(files.iter().map(|file| file.path.as_path()))
        .expect("all five renamed regular files enter the tray");
    assert_eq!(receipt.accepted_file_count, 5);
    assert_eq!(receipt.tray_file_count, 5);
    assert_eq!(receipt.messages_created, 0);
    assert_eq!(tray.attachments().len(), 5);

    let send_root = root.path().join("send-staging");
    fs::create_dir_all(&send_root).expect("create isolated send staging root");
    let mut plain_byte_send_count = 0usize;
    for (index, file) in files.iter().enumerate() {
        let filename = file
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("renamed fixture has a UTF-8 filename");
        let mime = accepted_attachment_mime(filename)
            .unwrap_or_else(|| panic!("harmless ending must be accepted: {filename}"));
        let key_bytes = [u8::try_from(index + 1).expect("five files fit in u8"); 32];
        let mut source = File::open(&file.path).expect("open renamed fixture as opaque bytes");
        let sealed = peer_attachment_io::encrypt_file(
            &send_root,
            &mut source,
            filename,
            mime,
            aead::Key::from_bytes(key_bytes),
            vec![u8::try_from(index + 11).expect("five content ids fit in u8"); 16],
            u32::try_from(index).expect("five attachment indexes fit in u32"),
        )
        .expect("stream opaque attachment bytes");
        let mut sealed_file = File::open(sealed.path()).expect("reopen sealed byte stream");
        let recovered = peer_attachment_io::decrypt_file(
            &send_root,
            &mut sealed_file,
            filename,
            mime,
            aead::Key::from_bytes(key_bytes),
        )
        .expect("recover the opaque byte stream");
        assert_eq!(
            fs::read(recovered.path().expect("recovered plaintext path"))
                .expect("read recovered opaque bytes"),
            file.bytes,
            "{} must be sent byte-for-byte without interpretation",
            file.kind.label()
        );
        recovered.remove_now().expect("remove recovered plaintext");
        peer_attachment_io::remove_staged_file(sealed).expect("remove sealed staging file");
        plain_byte_send_count += 1;
        println!(
            "TASK3220_PLAIN_BYTE_SEND kind={} filename={} bytes={} mode=plain_bytes",
            file.kind.label(),
            filename,
            file.bytes.len()
        );
    }

    let process_list_after = descendant_processes();
    let program_start_count = process_list_after.difference(&process_list_before).count();
    let archive_open_count = usize::from(root.path().join("archive-opened.marker").exists());
    let script_start_count = usize::from(root.path().join("script-started.marker").exists());

    assert_eq!(process_list_after, process_list_before);
    assert_eq!(program_start_count, 0);
    assert_eq!(archive_open_count, 0);
    assert_eq!(script_start_count, 0);
    assert_eq!(plain_byte_send_count, 5);

    println!("TASK3220_FIXTURE={}", fixture_path.display());
    println!("TASK3220_DANGEROUS_KIND_COUNT={}", detected.len());
    println!("TASK3220_TRAY_FILE_COUNT={}", tray.attachments().len());
    println!("TASK3220_MESSAGES_CREATED={}", receipt.messages_created);
    println!("TASK3220_PROGRAM_START_COUNT={program_start_count}");
    println!("TASK3220_SCRIPT_START_COUNT={script_start_count}");
    println!("TASK3220_ARCHIVE_OPEN_COUNT={archive_open_count}");
    println!("TASK3220_PROCESS_LIST_BEFORE={process_list_before:?}");
    println!("TASK3220_PROCESS_LIST_AFTER={process_list_after:?}");
    println!("TASK3220_PLAIN_BYTE_SEND_COUNT={plain_byte_send_count}");
}

fn load_and_validate_manifest(path: &Path) -> Result<Vec<FixtureSpec>, String> {
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "TASK3220 could not read fixture {}: {error}",
            path.display()
        )
    })?;
    let manifest: FixtureManifest = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "TASK3220 could not parse fixture {}: {error}",
            path.display()
        )
    })?;
    let kinds = manifest
        .files
        .iter()
        .map(|file| file.kind)
        .collect::<Vec<_>>();
    for required in REQUIRED_KINDS {
        let count = kinds.iter().filter(|kind| **kind == required).count();
        if count != 1 {
            return Err(format!(
                "TASK3220 missing dangerous file kind={} expected=1 actual={count}",
                required.label()
            ));
        }
    }
    if manifest.files.len() != REQUIRED_KINDS.len() {
        return Err(format!(
            "TASK3220 dangerous fixture count expected=5 actual={}",
            manifest.files.len()
        ));
    }
    for spec in &manifest.files {
        validate_names(spec)?;
    }
    Ok(manifest.files)
}

fn validate_names(spec: &FixtureSpec) -> Result<(), String> {
    let expected_original_extension = match spec.kind {
        DangerousKind::Program => "exe",
        DangerousKind::Archive => "zip",
        DangerousKind::DiskImage => "iso",
        DangerousKind::Script => "sh",
        DangerousKind::BrokenPicture => "png",
    };
    let original_extension = Path::new(&spec.original_name)
        .extension()
        .and_then(|extension| extension.to_str());
    if original_extension != Some(expected_original_extension) {
        return Err(format!(
            "TASK3220 {} fixture does not have its dangerous original ending",
            spec.kind.label()
        ));
    }
    if accepted_attachment_mime(&spec.renamed_name).is_none() {
        return Err(format!(
            "TASK3220 {} fixture does not have a supported harmless ending",
            spec.kind.label()
        ));
    }
    Ok(())
}

fn materialize_renamed_files(
    root: &Path,
    specs: &[FixtureSpec],
) -> Result<Vec<MaterializedFile>, String> {
    let mut files = Vec::with_capacity(specs.len());
    for spec in specs {
        let original = root.join(&spec.original_name);
        match spec.kind {
            DangerousKind::Program => materialize_program(&original)?,
            DangerousKind::Archive => materialize_archive(&original)?,
            DangerousKind::DiskImage => materialize_disk_image(&original)?,
            DangerousKind::Script => materialize_script(root, &original)?,
            DangerousKind::BrokenPicture => materialize_broken_picture(&original)?,
        }
        let renamed = root.join(&spec.renamed_name);
        fs::rename(&original, &renamed).map_err(|error| {
            format!(
                "TASK3220 could not rename {} to {}: {error}",
                original.display(),
                renamed.display()
            )
        })?;
        if original.exists() || !renamed.is_file() {
            return Err(format!(
                "TASK3220 rename did not replace dangerous ending for {}",
                spec.kind.label()
            ));
        }
        let bytes = fs::read(&renamed)
            .map_err(|error| format!("TASK3220 could not read {}: {error}", renamed.display()))?;
        let detected = detect_dangerous_kind(&bytes)?;
        if detected != spec.kind {
            return Err(format!(
                "TASK3220 byte detector expected={} actual={}",
                spec.kind.label(),
                detected.label()
            ));
        }
        files.push(MaterializedFile {
            kind: spec.kind,
            path: renamed,
            bytes,
        });
    }
    Ok(files)
}

fn materialize_program(path: &Path) -> Result<(), String> {
    let source = [Path::new("/usr/bin/true"), Path::new("/bin/true")]
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| "TASK3220 could not find the system ELF program fixture".to_owned())?;
    fs::copy(source, path)
        .map(|_| ())
        .map_err(|error| format!("TASK3220 could not copy program fixture: {error}"))
}

fn materialize_archive(path: &Path) -> Result<(), String> {
    let file = File::create(path)
        .map_err(|error| format!("TASK3220 could not create archive fixture: {error}"))?;
    let mut archive = zip::ZipWriter::new(file);
    archive
        .start_file("archive-opened.marker", SimpleFileOptions::default())
        .map_err(|error| format!("TASK3220 could not start archive entry: {error}"))?;
    archive
        .write_all(b"TASK3220 archive payload must never be extracted")
        .map_err(|error| format!("TASK3220 could not write archive entry: {error}"))?;
    archive
        .finish()
        .map(|_| ())
        .map_err(|error| format!("TASK3220 could not finish archive fixture: {error}"))
}

fn materialize_disk_image(path: &Path) -> Result<(), String> {
    let mut bytes = vec![0u8; 0x9000];
    bytes[0x8000..0x8007].copy_from_slice(b"\x01CD001\x01");
    const MARKER: &[u8] = b"TASK3220-DISK-IMAGE";
    bytes[0x8100..0x8100 + MARKER.len()].copy_from_slice(MARKER);
    fs::write(path, bytes)
        .map_err(|error| format!("TASK3220 could not create disk-image fixture: {error}"))
}

fn materialize_script(root: &Path, path: &Path) -> Result<(), String> {
    let marker = root.join("script-started.marker");
    fs::write(
        path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' TASK3220-SCRIPT-STARTED > '{}'\n",
            marker.display()
        ),
    )
    .map_err(|error| format!("TASK3220 could not create script fixture: {error}"))?;
    let mut permissions = fs::metadata(path)
        .map_err(|error| format!("TASK3220 could not stat script fixture: {error}"))?
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)
        .map_err(|error| format!("TASK3220 could not chmod script fixture: {error}"))
}

fn materialize_broken_picture(path: &Path) -> Result<(), String> {
    fs::write(
        path,
        b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01TRUNCATED-PICTURE-BYTES",
    )
    .map_err(|error| format!("TASK3220 could not create broken-picture fixture: {error}"))
}

fn detect_dangerous_kind(bytes: &[u8]) -> Result<DangerousKind, String> {
    if bytes.starts_with(b"\x7fELF") {
        return Ok(DangerousKind::Program);
    }
    if bytes.starts_with(b"PK\x03\x04") {
        return Ok(DangerousKind::Archive);
    }
    if bytes.get(0x8000..0x8007) == Some(b"\x01CD001\x01") {
        return Ok(DangerousKind::DiskImage);
    }
    if bytes.starts_with(b"#!") {
        return Ok(DangerousKind::Script);
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && !bytes.windows(4).any(|window| window == b"IEND")
    {
        return Ok(DangerousKind::BrokenPicture);
    }
    Err("TASK3220 fixture bytes do not identify one dangerous kind".to_owned())
}

fn descendant_processes() -> BTreeSet<u32> {
    let mut descendants = BTreeSet::new();
    let mut pending = vec![std::process::id()];
    while let Some(parent) = pending.pop() {
        let children_path = format!("/proc/{parent}/task/{parent}/children");
        let Ok(children) = fs::read_to_string(children_path) else {
            continue;
        };
        for child in children.split_whitespace() {
            if let Ok(pid) = child.parse::<u32>() {
                if descendants.insert(pid) {
                    pending.push(pid);
                }
            }
        }
    }
    descendants
}
