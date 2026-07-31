//! Bounded, in-memory attachment extraction for Scrub.
//!
//! Every route is selected from bytes, never a filename or caller MIME type.
//! Parsers do not write files or execute attachment content. Archive expansion,
//! entry count, recursion, and extracted text are globally bounded. This is a
//! memory-safe parser boundary, not a claim of an OS process sandbox.

use std::collections::BTreeSet;
use std::io::{Cursor, Read};
use std::path::{Component, Path};

use base64::Engine as _;
use calamine::Reader as _;
use flate2::read::GzDecoder;
use quick_xml::events::Event;
use quick_xml::Reader as XmlReader;
use serde::{Deserialize, Serialize};

pub const MAX_ATTACHMENTS_PER_MESSAGE: usize = 16;
pub const MAX_ATTACHMENT_BYTES: usize = 8 * 1024 * 1024;
const MAX_TOTAL_EXPANDED_BYTES: usize = 16 * 1024 * 1024;
const MAX_EXTRACTED_TEXT_BYTES: usize = 4 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 256;
const MAX_ARCHIVE_DEPTH: usize = 4;
const MAX_ATTACHMENT_ID_BYTES: usize = 128;
const MAX_DISPLAY_NAME_BYTES: usize = 256;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalAttachmentCandidate {
    pub attachment_id: String,
    pub display_name: String,
    pub content_base64: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UninspectedReason {
    Unsupported,
    Encrypted,
    Malformed,
    LimitExceeded,
    UnsafeArchiveEntry,
    ModelNotInstalled,
    DependencyNotInstalled,
    ImageOnlyPdfNeedsOcr,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninspectedAttachment {
    pub attachment_id: String,
    pub path: String,
    pub detected_type: String,
    pub reason: UninspectedReason,
    pub detail: String,
}

#[derive(Clone, Debug)]
pub struct ExtractedAttachmentText {
    pub path: String,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct ExplicitMediaSignal {
    pub path: String,
    pub confidence: u8,
    pub reason: &'static str,
}

#[derive(Clone, Debug)]
pub struct MediaFrame {
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct ImageClassification {
    pub explicit: bool,
    pub confidence: u8,
}

/// Local-only explicit/NSFW image classification capability.
pub trait LocalImageClassifier: Send + Sync {
    fn classify(&self, image: &[u8]) -> Result<ImageClassification, String>;
}

/// Local-only OCR capability for images or extracted video frames.
pub trait LocalOcrEngine: Send + Sync {
    fn extract_text(&self, image: &[u8]) -> Result<String, String>;
}

/// Local-only video frame extraction capability (normally backed by ffmpeg).
pub trait LocalVideoFrameExtractor: Send + Sync {
    fn keyframes(&self, video: &[u8]) -> Result<Vec<MediaFrame>, String>;
}

/// Local-only audio-to-transcript capability (normally backed by whisper.cpp).
pub trait LocalAudioTranscriber: Send + Sync {
    fn transcribe(&self, media: &[u8]) -> Result<String, String>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalMediaCapabilityFlags {
    pub image_classifier: bool,
    pub video_frame_extractor: bool,
    pub ocr: bool,
    pub speech_to_text: bool,
}

#[derive(Clone, Copy, Default)]
pub struct AttachmentAnalyzers<'a> {
    pub image_classifier: Option<&'a dyn LocalImageClassifier>,
    pub ocr: Option<&'a dyn LocalOcrEngine>,
    pub video_frames: Option<&'a dyn LocalVideoFrameExtractor>,
    pub speech_to_text: Option<&'a dyn LocalAudioTranscriber>,
}

impl AttachmentAnalyzers<'_> {
    pub fn capability_flags(self) -> LocalMediaCapabilityFlags {
        LocalMediaCapabilityFlags {
            image_classifier: self.image_classifier.is_some(),
            video_frame_extractor: self.video_frames.is_some(),
            ocr: self.ocr.is_some(),
            speech_to_text: self.speech_to_text.is_some(),
        }
    }
}

#[derive(Default)]
pub struct AttachmentScanOutput {
    pub attachments_scanned: usize,
    pub images_checked: bool,
    pub videos_checked: bool,
    pub attachment_types_scanned: Vec<String>,
    pub uninspected_attachments: Vec<UninspectedAttachment>,
    pub extracted_text: Vec<ExtractedAttachmentText>,
    pub explicit_media_signals: Vec<ExplicitMediaSignal>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DetectedType {
    PlainText,
    Markdown,
    Csv,
    Rtf,
    Pdf,
    Docx,
    Xlsx,
    Pptx,
    Zip,
    Tar,
    Gzip,
    Image,
    Video,
    Audio,
    OleCompound,
    Opaque,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AttachmentHandler {
    Text,
    Rtf,
    Pdf,
    Ooxml,
    Xlsx,
    Zip,
    Tar,
    Gzip,
    Image,
    Video,
    Audio,
    OleCompound,
    Unsupported,
}

/// The byte detector and extractor routing table are deliberately separate so
/// adding a recognized format cannot accidentally imply that it was scanned.
const ATTACHMENT_HANDLER_REGISTRY: &[(DetectedType, AttachmentHandler)] = &[
    (DetectedType::PlainText, AttachmentHandler::Text),
    (DetectedType::Markdown, AttachmentHandler::Text),
    (DetectedType::Csv, AttachmentHandler::Text),
    (DetectedType::Rtf, AttachmentHandler::Rtf),
    // PDF parsing remains disabled until it runs in a separately bounded
    // worker process. In-process parsers cannot provide a wall-clock or
    // memory ceiling for hostile files.
    (DetectedType::Pdf, AttachmentHandler::Unsupported),
    (DetectedType::Docx, AttachmentHandler::Ooxml),
    // XLSX parsing remains disabled for the same reason: ZIP limits applied
    // outside calamine do not bound its internal workbook allocations.
    (DetectedType::Xlsx, AttachmentHandler::Unsupported),
    (DetectedType::Pptx, AttachmentHandler::Ooxml),
    (DetectedType::Zip, AttachmentHandler::Zip),
    (DetectedType::Tar, AttachmentHandler::Tar),
    (DetectedType::Gzip, AttachmentHandler::Gzip),
    (DetectedType::Image, AttachmentHandler::Image),
    (DetectedType::Video, AttachmentHandler::Video),
    (DetectedType::Audio, AttachmentHandler::Audio),
    (DetectedType::OleCompound, AttachmentHandler::OleCompound),
    (DetectedType::Opaque, AttachmentHandler::Unsupported),
];

fn handler_for(detected: DetectedType) -> AttachmentHandler {
    ATTACHMENT_HANDLER_REGISTRY
        .iter()
        .find_map(|(kind, handler)| (*kind == detected).then_some(*handler))
        .unwrap_or(AttachmentHandler::Unsupported)
}

impl DetectedType {
    fn label(self) -> &'static str {
        match self {
            Self::PlainText => "plain_text",
            Self::Markdown => "markdown",
            Self::Csv => "csv",
            Self::Rtf => "rtf",
            Self::Pdf => "pdf",
            Self::Docx => "docx",
            Self::Xlsx => "xlsx",
            Self::Pptx => "pptx",
            Self::Zip => "zip",
            Self::Tar => "tar",
            Self::Gzip => "gzip",
            Self::Image => "image",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::OleCompound => "ole_compound_document",
            Self::Opaque => "opaque",
        }
    }
}

struct Budget {
    expanded_bytes: usize,
    extracted_text_bytes: usize,
    archive_entries: usize,
}

pub fn scan_attachments(
    attachments: &[LocalAttachmentCandidate],
    analyzers: AttachmentAnalyzers<'_>,
) -> AttachmentScanOutput {
    let mut output = AttachmentScanOutput::default();
    let mut budget = Budget {
        expanded_bytes: 0,
        extracted_text_bytes: 0,
        archive_entries: 0,
    };
    if attachments.len() > MAX_ATTACHMENTS_PER_MESSAGE {
        output.uninspected_attachments.push(uninspected(
            "message",
            "attachments",
            "collection",
            UninspectedReason::LimitExceeded,
            format!("Attachment count exceeds the {MAX_ATTACHMENTS_PER_MESSAGE}-item limit"),
        ));
        for attachment in attachments.iter().skip(MAX_ATTACHMENTS_PER_MESSAGE) {
            output.uninspected_attachments.push(uninspected(
                safe_display_path(&attachment.attachment_id),
                safe_display_path(&attachment.display_name),
                "not_detected",
                UninspectedReason::LimitExceeded,
                "Attachment was not opened because the per-message attachment-count limit was reached"
                    .into(),
            ));
        }
    }
    for attachment in attachments.iter().take(MAX_ATTACHMENTS_PER_MESSAGE) {
        if !valid_attachment_metadata(attachment) {
            output.uninspected_attachments.push(uninspected(
                safe_display_path(&attachment.attachment_id),
                safe_display_path(&attachment.display_name),
                "unknown",
                UninspectedReason::Malformed,
                "Attachment metadata or base64 encoding is invalid".into(),
            ));
            continue;
        }
        let padding = attachment
            .content_base64
            .as_bytes()
            .iter()
            .rev()
            .take_while(|byte| **byte == b'=')
            .count();
        let estimated =
            (attachment.content_base64.len().saturating_mul(3) / 4).saturating_sub(padding);
        if estimated > MAX_ATTACHMENT_BYTES {
            output.uninspected_attachments.push(uninspected(
                &attachment.attachment_id,
                &attachment.display_name,
                "unknown",
                UninspectedReason::LimitExceeded,
                format!("Attachment exceeds the {MAX_ATTACHMENT_BYTES}-byte input limit"),
            ));
            continue;
        }
        let bytes =
            match base64::engine::general_purpose::STANDARD.decode(&attachment.content_base64) {
                Ok(bytes) if bytes.len() <= MAX_ATTACHMENT_BYTES => bytes,
                _ => {
                    output.uninspected_attachments.push(uninspected(
                        &attachment.attachment_id,
                        &attachment.display_name,
                        "unknown",
                        UninspectedReason::Malformed,
                        "Attachment base64 could not be decoded within the input limit".into(),
                    ));
                    continue;
                }
            };
        let scanned_before = output.attachment_types_scanned.len();
        process_bytes(
            &attachment.attachment_id,
            &attachment.display_name,
            &bytes,
            0,
            analyzers,
            &mut budget,
            &mut output,
        );
        if output.attachment_types_scanned.len() > scanned_before {
            output.attachments_scanned = output.attachments_scanned.saturating_add(1);
        }
    }
    output.attachment_types_scanned.sort();
    output.attachment_types_scanned.dedup();
    output.images_checked = output
        .attachment_types_scanned
        .iter()
        .any(|kind| kind == "image")
        && !output
            .uninspected_attachments
            .iter()
            .any(|item| item.detected_type == "image");
    output.videos_checked = output
        .attachment_types_scanned
        .iter()
        .any(|kind| kind == "video")
        && !output
            .uninspected_attachments
            .iter()
            .any(|item| item.detected_type == "video");
    output
}

fn process_bytes(
    attachment_id: &str,
    path: &str,
    bytes: &[u8],
    depth: usize,
    analyzers: AttachmentAnalyzers<'_>,
    budget: &mut Budget,
    output: &mut AttachmentScanOutput,
) {
    if depth > MAX_ARCHIVE_DEPTH {
        output.uninspected_attachments.push(uninspected(
            attachment_id,
            path,
            detect_type(bytes).label(),
            UninspectedReason::LimitExceeded,
            format!("Archive recursion exceeds the depth limit of {MAX_ARCHIVE_DEPTH}"),
        ));
        return;
    }
    if depth == 0 && !consume_expanded(bytes.len(), budget) {
        output.uninspected_attachments.push(uninspected(
            attachment_id,
            path,
            detect_type(bytes).label(),
            UninspectedReason::LimitExceeded,
            format!("Expanded attachment data exceeds {MAX_TOTAL_EXPANDED_BYTES} bytes"),
        ));
        return;
    }

    let detected = detect_type(bytes);
    match handler_for(detected) {
        AttachmentHandler::Text => {
            if let Ok(text) = std::str::from_utf8(bytes) {
                add_text(attachment_id, path, detected, text, budget, output);
            } else {
                mark_malformed(
                    attachment_id,
                    path,
                    detected,
                    "Text is not valid UTF-8",
                    output,
                );
            }
        }
        AttachmentHandler::Rtf => match extract_rtf(bytes) {
            Ok(text) => add_text(attachment_id, path, detected, &text, budget, output),
            Err(detail) => mark_malformed(attachment_id, path, detected, &detail, output),
        },
        AttachmentHandler::Pdf => extract_pdf(attachment_id, path, bytes, budget, output),
        AttachmentHandler::Ooxml => extract_ooxml(
            attachment_id,
            path,
            bytes,
            detected,
            depth,
            analyzers,
            budget,
            output,
        ),
        AttachmentHandler::Xlsx => {
            extract_xlsx(attachment_id, path, bytes, depth, analyzers, budget, output)
        }
        AttachmentHandler::Zip => {
            extract_zip_archive(attachment_id, path, bytes, depth, analyzers, budget, output)
        }
        AttachmentHandler::Tar => {
            extract_tar_archive(attachment_id, path, bytes, depth, analyzers, budget, output)
        }
        AttachmentHandler::Gzip => {
            extract_gzip(attachment_id, path, bytes, depth, analyzers, budget, output)
        }
        AttachmentHandler::Image => {
            inspect_image(attachment_id, path, bytes, analyzers, budget, output)
        }
        AttachmentHandler::Video => {
            inspect_video(attachment_id, path, bytes, analyzers, budget, output)
        }
        AttachmentHandler::Audio => {
            inspect_audio(attachment_id, path, bytes, analyzers, budget, output)
        }
        AttachmentHandler::OleCompound => {
            let encrypted = contains_utf16le(bytes, "EncryptedPackage")
                || contains_utf16le(bytes, "EncryptionInfo");
            output.uninspected_attachments.push(uninspected(
                attachment_id,
                path,
                detected.label(),
                if encrypted {
                    UninspectedReason::Encrypted
                } else {
                    UninspectedReason::Unsupported
                },
                if encrypted {
                    "Encrypted Office compound document cannot be inspected".into()
                } else {
                    "Legacy OLE/CFB document is outside the pure OOXML extraction path".into()
                },
            ));
        }
        AttachmentHandler::Unsupported => {
            let detail = match detected {
                DetectedType::Pdf | DetectedType::Xlsx => {
                    "This format is held until its parser runs in a time- and memory-bounded worker"
                }
                _ => "No registered byte-signature extractor supports this content",
            };
            output.uninspected_attachments.push(uninspected(
                attachment_id,
                path,
                detected.label(),
                UninspectedReason::Unsupported,
                detail.into(),
            ));
        }
    }
}

fn detect_type(bytes: &[u8]) -> DetectedType {
    if bytes[..bytes.len().min(1_024)]
        .windows(5)
        .any(|window| window == b"%PDF-")
    {
        return DetectedType::Pdf;
    }
    if bytes.starts_with(b"{\\rtf") {
        return DetectedType::Rtf;
    }
    if bytes.starts_with(&[0x1f, 0x8b]) {
        return DetectedType::Gzip;
    }
    if bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]) {
        return DetectedType::OleCompound;
    }
    if is_zip(bytes) {
        return detect_zip_container(bytes);
    }
    if is_tar(bytes) {
        return DetectedType::Tar;
    }
    if is_image(bytes) {
        return DetectedType::Image;
    }
    if is_video(bytes) {
        return DetectedType::Video;
    }
    if is_audio(bytes) {
        return DetectedType::Audio;
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        if looks_like_text(text) {
            if looks_like_csv(text) {
                return DetectedType::Csv;
            }
            if looks_like_markdown(text) {
                return DetectedType::Markdown;
            }
            return DetectedType::PlainText;
        }
    }
    DetectedType::Opaque
}

fn detect_zip_container(bytes: &[u8]) -> DetectedType {
    let Ok(mut zip) = zip::ZipArchive::new(Cursor::new(bytes)) else {
        return DetectedType::Zip;
    };
    let mut names = BTreeSet::new();
    for index in 0..zip.len().min(64) {
        if let Ok(file) = zip.by_index(index) {
            names.insert(file.name().replace('\\', "/"));
        }
    }
    if names.contains("word/document.xml") {
        DetectedType::Docx
    } else if names.contains("xl/workbook.xml") {
        DetectedType::Xlsx
    } else if names.contains("ppt/presentation.xml") {
        DetectedType::Pptx
    } else {
        DetectedType::Zip
    }
}

fn extract_pdf(
    attachment_id: &str,
    path: &str,
    bytes: &[u8],
    budget: &mut Budget,
    output: &mut AttachmentScanOutput,
) {
    if bytes.windows(8).any(|window| window == b"/Encrypt") {
        output.uninspected_attachments.push(uninspected(
            attachment_id,
            path,
            "pdf",
            UninspectedReason::Encrypted,
            "The PDF declares encryption".into(),
        ));
        return;
    }
    match std::panic::catch_unwind(|| pdf_extract::extract_text_from_mem(bytes)) {
        Ok(Ok(text)) if !text.trim().is_empty() => {
            add_text(
                attachment_id,
                path,
                DetectedType::Pdf,
                &text,
                budget,
                output,
            );
            output.uninspected_attachments.push(uninspected(
                attachment_id,
                path,
                "image",
                UninspectedReason::ModelNotInstalled,
                "PDF text was extracted, but page images were not OCR/classified because local OCR/image models are not installed".into(),
            ));
        }
        Ok(Ok(_)) => output.uninspected_attachments.push(uninspected(
            attachment_id,
            path,
            "image",
            UninspectedReason::ImageOnlyPdfNeedsOcr,
            "No text layer was found; a local OCR model is required".into(),
        )),
        Ok(Err(_)) | Err(_) => mark_malformed(
            attachment_id,
            path,
            DetectedType::Pdf,
            "The bounded PDF parser could not extract this file",
            output,
        ),
    }
}

fn extract_ooxml(
    attachment_id: &str,
    path: &str,
    bytes: &[u8],
    detected: DetectedType,
    depth: usize,
    analyzers: AttachmentAnalyzers<'_>,
    budget: &mut Budget,
    output: &mut AttachmentScanOutput,
) {
    let Ok(mut zip) = zip::ZipArchive::new(Cursor::new(bytes)) else {
        mark_malformed(
            attachment_id,
            path,
            detected,
            "OOXML ZIP is malformed",
            output,
        );
        return;
    };
    let mut found_text_part = false;
    for index in 0..zip.len() {
        if !consume_archive_entry(budget) {
            output.uninspected_attachments.push(uninspected(
                attachment_id,
                path,
                detected.label(),
                UninspectedReason::LimitExceeded,
                format!("Archive exceeds the {MAX_ARCHIVE_ENTRIES}-entry limit"),
            ));
            return;
        }
        let Ok(mut file) = zip.by_index(index) else {
            output.uninspected_attachments.push(uninspected(
                attachment_id,
                join_path(path, &format!("member:{index}")),
                detected.label(),
                UninspectedReason::Malformed,
                "OOXML member metadata could not be decoded".into(),
            ));
            continue;
        };
        let Some(safe_name) = safe_zip_name(&file) else {
            output.uninspected_attachments.push(uninspected(
                attachment_id,
                join_path(path, file.name()),
                detected.label(),
                UninspectedReason::UnsafeArchiveEntry,
                "OOXML member has an unsafe path or non-regular file type".into(),
            ));
            continue;
        };
        if file.encrypted() {
            output.uninspected_attachments.push(uninspected(
                attachment_id,
                join_path(path, &safe_name),
                detected.label(),
                UninspectedReason::Encrypted,
                "Encrypted OOXML member cannot be inspected".into(),
            ));
            continue;
        }
        let relevant_xml = match detected {
            DetectedType::Docx => safe_name.starts_with("word/") && safe_name.ends_with(".xml"),
            DetectedType::Pptx => safe_name.starts_with("ppt/") && safe_name.ends_with(".xml"),
            _ => false,
        };
        let embedded = is_ooxml_embedded(&safe_name);
        if !relevant_xml && !embedded {
            continue;
        }
        let Some(member) = read_bounded(&mut file, budget) else {
            output.uninspected_attachments.push(uninspected(
                attachment_id,
                join_path(path, &safe_name),
                detected.label(),
                UninspectedReason::LimitExceeded,
                "OOXML member exceeds the remaining expansion budget".into(),
            ));
            continue;
        };
        if relevant_xml {
            match extract_xml_text(&member) {
                Ok(text) if !text.trim().is_empty() => {
                    found_text_part = true;
                    add_text(
                        attachment_id,
                        &join_path(path, &safe_name),
                        detected,
                        &text,
                        budget,
                        output,
                    );
                }
                Ok(_) => {}
                Err(_) => output.uninspected_attachments.push(uninspected(
                    attachment_id,
                    join_path(path, &safe_name),
                    detected.label(),
                    UninspectedReason::Malformed,
                    "OOXML text member could not be decoded".into(),
                )),
            }
        } else {
            process_bytes(
                attachment_id,
                &join_path(path, &safe_name),
                &member,
                depth + 1,
                analyzers,
                budget,
                output,
            );
        }
    }
    if !found_text_part {
        mark_malformed(
            attachment_id,
            path,
            detected,
            "No readable OOXML text parts were found",
            output,
        );
    }
}

fn extract_xlsx(
    attachment_id: &str,
    path: &str,
    bytes: &[u8],
    depth: usize,
    analyzers: AttachmentAnalyzers<'_>,
    budget: &mut Budget,
    output: &mut AttachmentScanOutput,
) {
    let mut found = false;
    match calamine::Xlsx::new(Cursor::new(bytes)) {
        Ok(mut workbook) => {
            let names = workbook.sheet_names().to_vec();
            for name in names {
                match workbook.worksheet_range(&name) {
                    Ok(range) => {
                        let mut text = String::new();
                        for row in range.rows() {
                            for cell in row {
                                let value = cell.to_string();
                                if !value.is_empty() {
                                    text.push_str(&value);
                                    text.push('\t');
                                }
                            }
                            text.push('\n');
                        }
                        if !text.trim().is_empty() {
                            found = true;
                            add_text(
                                attachment_id,
                                &join_path(path, &format!("sheet:{name}")),
                                DetectedType::Xlsx,
                                &text,
                                budget,
                                output,
                            );
                        }
                    }
                    Err(_) => output.uninspected_attachments.push(uninspected(
                        attachment_id,
                        join_path(path, &format!("sheet:{name}")),
                        "xlsx",
                        UninspectedReason::Malformed,
                        "Spreadsheet sheet could not be decoded".into(),
                    )),
                }
            }
        }
        Err(_) => {
            mark_malformed(
                attachment_id,
                path,
                DetectedType::Xlsx,
                "Spreadsheet workbook is malformed or encrypted",
                output,
            );
            return;
        }
    }
    extract_ooxml_embedded(attachment_id, path, bytes, depth, analyzers, budget, output);
    if !found {
        output.uninspected_attachments.push(uninspected(
            attachment_id,
            path,
            "xlsx",
            UninspectedReason::Malformed,
            "No readable spreadsheet cell values were found".into(),
        ));
    }
}

fn extract_ooxml_embedded(
    attachment_id: &str,
    path: &str,
    bytes: &[u8],
    depth: usize,
    analyzers: AttachmentAnalyzers<'_>,
    budget: &mut Budget,
    output: &mut AttachmentScanOutput,
) {
    let Ok(mut zip) = zip::ZipArchive::new(Cursor::new(bytes)) else {
        return;
    };
    for index in 0..zip.len() {
        let Ok(mut file) = zip.by_index(index) else {
            continue;
        };
        let Some(name) = safe_zip_name(&file) else {
            continue;
        };
        if !is_ooxml_embedded(&name) {
            continue;
        }
        if !consume_archive_entry(budget) {
            output.uninspected_attachments.push(uninspected(
                attachment_id,
                path,
                "xlsx",
                UninspectedReason::LimitExceeded,
                format!("Archive exceeds the {MAX_ARCHIVE_ENTRIES}-entry limit"),
            ));
            return;
        }
        let Some(member) = read_bounded(&mut file, budget) else {
            continue;
        };
        process_bytes(
            attachment_id,
            &join_path(path, &name),
            &member,
            depth + 1,
            analyzers,
            budget,
            output,
        );
    }
}

fn extract_zip_archive(
    attachment_id: &str,
    path: &str,
    bytes: &[u8],
    depth: usize,
    analyzers: AttachmentAnalyzers<'_>,
    budget: &mut Budget,
    output: &mut AttachmentScanOutput,
) {
    let Ok(mut zip) = zip::ZipArchive::new(Cursor::new(bytes)) else {
        mark_malformed(
            attachment_id,
            path,
            DetectedType::Zip,
            "ZIP archive is malformed",
            output,
        );
        return;
    };
    note_scanned(DetectedType::Zip, output);
    for index in 0..zip.len() {
        if !consume_archive_entry(budget) {
            output.uninspected_attachments.push(uninspected(
                attachment_id,
                path,
                "zip",
                UninspectedReason::LimitExceeded,
                format!("Archive exceeds the {MAX_ARCHIVE_ENTRIES}-entry limit"),
            ));
            return;
        }
        let Ok(mut file) = zip.by_index(index) else {
            continue;
        };
        if file.is_dir() {
            continue;
        }
        let Some(name) = safe_zip_name(&file) else {
            output.uninspected_attachments.push(uninspected(
                attachment_id,
                join_path(path, file.name()),
                "zip_entry",
                UninspectedReason::UnsafeArchiveEntry,
                "ZIP entry path escapes the archive or is not a regular file".into(),
            ));
            continue;
        };
        if file.encrypted() {
            output.uninspected_attachments.push(uninspected(
                attachment_id,
                join_path(path, &name),
                "zip_entry",
                UninspectedReason::Encrypted,
                "Encrypted ZIP entry cannot be inspected".into(),
            ));
            continue;
        }
        let Some(member) = read_bounded(&mut file, budget) else {
            output.uninspected_attachments.push(uninspected(
                attachment_id,
                join_path(path, &name),
                "zip_entry",
                UninspectedReason::LimitExceeded,
                "ZIP entry exceeds the remaining expansion budget".into(),
            ));
            continue;
        };
        process_bytes(
            attachment_id,
            &join_path(path, &name),
            &member,
            depth + 1,
            analyzers,
            budget,
            output,
        );
    }
}

fn extract_tar_archive(
    attachment_id: &str,
    path: &str,
    bytes: &[u8],
    depth: usize,
    analyzers: AttachmentAnalyzers<'_>,
    budget: &mut Budget,
    output: &mut AttachmentScanOutput,
) {
    let mut archive = tar::Archive::new(Cursor::new(bytes));
    let Ok(entries) = archive.entries() else {
        mark_malformed(
            attachment_id,
            path,
            DetectedType::Tar,
            "TAR archive is malformed",
            output,
        );
        return;
    };
    note_scanned(DetectedType::Tar, output);
    for entry in entries {
        if !consume_archive_entry(budget) {
            output.uninspected_attachments.push(uninspected(
                attachment_id,
                path,
                "tar",
                UninspectedReason::LimitExceeded,
                format!("Archive exceeds the {MAX_ARCHIVE_ENTRIES}-entry limit"),
            ));
            return;
        }
        let Ok(mut entry) = entry else {
            mark_malformed(
                attachment_id,
                path,
                DetectedType::Tar,
                "TAR entry is malformed",
                output,
            );
            continue;
        };
        let name = entry
            .path()
            .ok()
            .and_then(|value| safe_relative_path(&value));
        let Some(name) = name else {
            output.uninspected_attachments.push(uninspected(
                attachment_id,
                path,
                "tar_entry",
                UninspectedReason::UnsafeArchiveEntry,
                "TAR entry has an unsafe path".into(),
            ));
            continue;
        };
        if entry.header().entry_type().is_dir() {
            continue;
        }
        if !entry.header().entry_type().is_file() {
            output.uninspected_attachments.push(uninspected(
                attachment_id,
                join_path(path, &name),
                "tar_entry",
                UninspectedReason::UnsafeArchiveEntry,
                "TAR links, devices, FIFOs, and special entries are rejected".into(),
            ));
            continue;
        }
        let Some(member) = read_bounded(&mut entry, budget) else {
            output.uninspected_attachments.push(uninspected(
                attachment_id,
                join_path(path, &name),
                "tar_entry",
                UninspectedReason::LimitExceeded,
                "TAR entry exceeds the remaining expansion budget".into(),
            ));
            continue;
        };
        process_bytes(
            attachment_id,
            &join_path(path, &name),
            &member,
            depth + 1,
            analyzers,
            budget,
            output,
        );
    }
}

fn extract_gzip(
    attachment_id: &str,
    path: &str,
    bytes: &[u8],
    depth: usize,
    analyzers: AttachmentAnalyzers<'_>,
    budget: &mut Budget,
    output: &mut AttachmentScanOutput,
) {
    let mut decoder = GzDecoder::new(bytes);
    let remaining = MAX_TOTAL_EXPANDED_BYTES.saturating_sub(budget.expanded_bytes);
    let mut decoded = Vec::new();
    if decoder
        .by_ref()
        .take((remaining as u64).saturating_add(1))
        .read_to_end(&mut decoded)
        .is_err()
    {
        mark_malformed(
            attachment_id,
            path,
            DetectedType::Gzip,
            "Gzip stream is malformed",
            output,
        );
        return;
    }
    if decoded.len() > remaining {
        output.uninspected_attachments.push(uninspected(
            attachment_id,
            path,
            "gzip",
            UninspectedReason::LimitExceeded,
            "Gzip output exceeds the remaining expansion budget".into(),
        ));
        return;
    }
    budget.expanded_bytes = budget.expanded_bytes.saturating_add(decoded.len());
    if decoded.is_empty() {
        mark_malformed(
            attachment_id,
            path,
            DetectedType::Gzip,
            "Gzip stream is empty or malformed",
            output,
        );
        return;
    }
    note_scanned(DetectedType::Gzip, output);
    process_bytes(
        attachment_id,
        &join_path(path, "gzip-payload"),
        &decoded,
        depth + 1,
        analyzers,
        budget,
        output,
    );
}

fn inspect_image(
    attachment_id: &str,
    path: &str,
    bytes: &[u8],
    analyzers: AttachmentAnalyzers<'_>,
    budget: &mut Budget,
    output: &mut AttachmentScanOutput,
) {
    let (Some(classifier), Some(ocr)) = (analyzers.image_classifier, analyzers.ocr) else {
        output.uninspected_attachments.push(uninspected(
            attachment_id,
            path,
            "image",
            UninspectedReason::ModelNotInstalled,
            "Image explicit-content classification and OCR require a verified local model pack and Tesseract language data".into(),
        ));
        return;
    };
    match (classifier.classify(bytes), ocr.extract_text(bytes)) {
        (Ok(classification), Ok(text)) => {
            output.images_checked = true;
            note_scanned(DetectedType::Image, output);
            if classification.explicit {
                output.explicit_media_signals.push(ExplicitMediaSignal {
                    path: path.into(),
                    confidence: classification.confidence.min(100),
                    reason:
                        "A local image classifier marked this attachment as potentially explicit.",
                });
            }
            if !text.trim().is_empty() {
                add_text(
                    attachment_id,
                    path,
                    DetectedType::Image,
                    &text,
                    budget,
                    output,
                );
            }
        }
        _ => output.uninspected_attachments.push(uninspected(
            attachment_id,
            path,
            "image",
            UninspectedReason::Malformed,
            "Installed local image classifier or OCR capability failed closed".into(),
        )),
    }
}

fn inspect_video(
    attachment_id: &str,
    path: &str,
    bytes: &[u8],
    analyzers: AttachmentAnalyzers<'_>,
    budget: &mut Budget,
    output: &mut AttachmentScanOutput,
) {
    let (Some(frames), Some(classifier), Some(ocr), Some(transcriber)) = (
        analyzers.video_frames,
        analyzers.image_classifier,
        analyzers.ocr,
        analyzers.speech_to_text,
    ) else {
        output.uninspected_attachments.push(uninspected(
            attachment_id,
            path,
            "video",
            UninspectedReason::DependencyNotInstalled,
            "Video deep inspection requires ffmpeg/ffprobe, the verified image model pack, Tesseract language data, and whisper.cpp with a local model".into(),
        ));
        return;
    };
    let Ok(keyframes) = frames.keyframes(bytes) else {
        mark_malformed(
            attachment_id,
            path,
            DetectedType::Video,
            "Local frame extraction failed closed",
            output,
        );
        return;
    };
    if keyframes.len() > 32 {
        output.uninspected_attachments.push(uninspected(
            attachment_id,
            path,
            "video",
            UninspectedReason::LimitExceeded,
            "Frame extractor returned more than the 32-keyframe limit".into(),
        ));
        return;
    }
    for (index, frame) in keyframes.iter().enumerate() {
        let Ok(classification) = classifier.classify(&frame.bytes) else {
            mark_malformed(
                attachment_id,
                path,
                DetectedType::Video,
                "Keyframe classification failed closed",
                output,
            );
            return;
        };
        if classification.explicit {
            output.explicit_media_signals.push(ExplicitMediaSignal {
                path: format!("{path}/keyframe-{index}"),
                confidence: classification.confidence.min(100),
                reason: "A local image classifier marked a video keyframe as potentially explicit.",
            });
        }
        let Ok(text) = ocr.extract_text(&frame.bytes) else {
            mark_malformed(
                attachment_id,
                path,
                DetectedType::Video,
                "Keyframe OCR failed closed",
                output,
            );
            return;
        };
        if !text.trim().is_empty() {
            add_text(
                attachment_id,
                &format!("{path}/keyframe-{index}"),
                DetectedType::Video,
                &text,
                budget,
                output,
            );
        }
    }
    let Ok(transcript) = transcriber.transcribe(bytes) else {
        mark_malformed(
            attachment_id,
            path,
            DetectedType::Video,
            "Audio transcription failed closed",
            output,
        );
        return;
    };
    if !transcript.trim().is_empty() {
        add_text(
            attachment_id,
            &format!("{path}/audio-transcript"),
            DetectedType::Video,
            &transcript,
            budget,
            output,
        );
    }
    output.videos_checked = true;
    note_scanned(DetectedType::Video, output);
}

fn inspect_audio(
    attachment_id: &str,
    path: &str,
    bytes: &[u8],
    analyzers: AttachmentAnalyzers<'_>,
    budget: &mut Budget,
    output: &mut AttachmentScanOutput,
) {
    let Some(transcriber) = analyzers.speech_to_text else {
        output.uninspected_attachments.push(uninspected(
            attachment_id,
            path,
            "audio",
            UninspectedReason::ModelNotInstalled,
            "Audio transcription requires whisper.cpp and a verified local Whisper model".into(),
        ));
        return;
    };
    match transcriber.transcribe(bytes) {
        Ok(text) => {
            note_scanned(DetectedType::Audio, output);
            if !text.trim().is_empty() {
                add_text(
                    attachment_id,
                    path,
                    DetectedType::Audio,
                    &text,
                    budget,
                    output,
                );
            }
        }
        Err(_) => mark_malformed(
            attachment_id,
            path,
            DetectedType::Audio,
            "Audio transcription failed closed",
            output,
        ),
    }
}

fn add_text(
    attachment_id: &str,
    path: &str,
    detected: DetectedType,
    text: &str,
    budget: &mut Budget,
    output: &mut AttachmentScanOutput,
) {
    if text.trim().is_empty() {
        return;
    }
    let remaining = MAX_EXTRACTED_TEXT_BYTES.saturating_sub(budget.extracted_text_bytes);
    if text.len() > remaining {
        output.uninspected_attachments.push(uninspected(
            attachment_id,
            path,
            detected.label(),
            UninspectedReason::LimitExceeded,
            format!("Extracted text exceeds the {MAX_EXTRACTED_TEXT_BYTES}-byte scan limit"),
        ));
        return;
    }
    budget.extracted_text_bytes += text.len();
    output.extracted_text.push(ExtractedAttachmentText {
        path: path.into(),
        text: text.into(),
    });
    note_scanned(detected, output);
}

fn note_scanned(detected: DetectedType, output: &mut AttachmentScanOutput) {
    output
        .attachment_types_scanned
        .push(detected.label().into());
}

fn mark_malformed(
    attachment_id: &str,
    path: &str,
    detected: DetectedType,
    detail: &str,
    output: &mut AttachmentScanOutput,
) {
    output.uninspected_attachments.push(uninspected(
        attachment_id,
        path,
        detected.label(),
        UninspectedReason::Malformed,
        detail.into(),
    ));
}

fn uninspected(
    attachment_id: impl Into<String>,
    path: impl Into<String>,
    detected_type: impl Into<String>,
    reason: UninspectedReason,
    detail: String,
) -> UninspectedAttachment {
    UninspectedAttachment {
        attachment_id: attachment_id.into(),
        path: path.into(),
        detected_type: detected_type.into(),
        reason,
        detail,
    }
}

fn valid_attachment_metadata(value: &LocalAttachmentCandidate) -> bool {
    !value.attachment_id.is_empty()
        && value.attachment_id.len() <= MAX_ATTACHMENT_ID_BYTES
        && !value.attachment_id.chars().any(unsafe_metadata_char)
        && !value.display_name.is_empty()
        && value.display_name.len() <= MAX_DISPLAY_NAME_BYTES
        && !value.display_name.chars().any(unsafe_metadata_char)
        && !value.content_base64.is_empty()
        && value.content_base64.len() <= (MAX_ATTACHMENT_BYTES * 4 / 3 + 8)
}

fn safe_display_path(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|character| {
            if unsafe_metadata_char(character) {
                '\u{fffd}'
            } else {
                character
            }
        })
        .take(MAX_DISPLAY_NAME_BYTES)
        .collect();
    if sanitized.is_empty() {
        "attachment".into()
    } else {
        sanitized
    }
}

fn consume_expanded(bytes: usize, budget: &mut Budget) -> bool {
    let Some(next) = budget.expanded_bytes.checked_add(bytes) else {
        return false;
    };
    if next > MAX_TOTAL_EXPANDED_BYTES {
        return false;
    }
    budget.expanded_bytes = next;
    true
}

fn consume_archive_entry(budget: &mut Budget) -> bool {
    if budget.archive_entries >= MAX_ARCHIVE_ENTRIES {
        return false;
    }
    budget.archive_entries += 1;
    true
}

fn read_bounded(reader: &mut impl Read, budget: &mut Budget) -> Option<Vec<u8>> {
    let remaining = MAX_TOTAL_EXPANDED_BYTES.saturating_sub(budget.expanded_bytes);
    let mut bytes = Vec::new();
    reader
        .take((remaining as u64).saturating_add(1))
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > remaining {
        return None;
    }
    budget.expanded_bytes = budget.expanded_bytes.saturating_add(bytes.len());
    Some(bytes)
}

fn safe_zip_name<R: Read>(file: &zip::read::ZipFile<'_, R>) -> Option<String> {
    let path = file.enclosed_name()?;
    let name = safe_relative_path(&path)?;
    if let Some(mode) = file.unix_mode() {
        let kind = mode & 0o170000;
        if kind != 0 && kind != 0o100000 && !file.is_dir() {
            return None;
        }
    }
    Some(name)
}

fn safe_relative_path(path: &Path) -> Option<String> {
    if path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return None;
    }
    let rendered = path.to_string_lossy().replace('\\', "/");
    (!rendered.is_empty() && rendered.len() <= 512 && !rendered.chars().any(unsafe_metadata_char))
        .then_some(rendered)
}

fn join_path(parent: &str, child: &str) -> String {
    let child = child.trim_start_matches(['/', '\\']);
    format!("{parent}/{child}")
        .chars()
        .map(|value| {
            if unsafe_metadata_char(value) {
                '\u{fffd}'
            } else {
                value
            }
        })
        .take(768)
        .collect()
}

fn unsafe_metadata_char(value: char) -> bool {
    value.is_control()
        || matches!(
            value,
            '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
        )
}

fn extract_xml_text(bytes: &[u8]) -> Result<String, String> {
    let mut reader = XmlReader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut text = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Text(value)) => {
                let decoded = value.decode().map_err(|_| "invalid XML text")?;
                if !decoded.trim().is_empty() {
                    text.push_str(&decoded);
                    text.push(' ');
                }
            }
            Ok(Event::CData(value)) => {
                let decoded = value.decode().map_err(|_| "invalid XML CDATA")?;
                text.push_str(&decoded);
                text.push(' ');
            }
            Ok(Event::Eof) => break,
            Err(_) => return Err("malformed XML".into()),
            _ => {}
        }
        if text.len() > MAX_EXTRACTED_TEXT_BYTES {
            return Err("XML text exceeds extraction limit".into());
        }
    }
    Ok(text)
}

fn extract_rtf(bytes: &[u8]) -> Result<String, String> {
    let input = std::str::from_utf8(bytes).map_err(|_| "RTF is not valid UTF-8")?;
    if !input.starts_with("{\\rtf") {
        return Err("RTF header is missing".into());
    }
    let mut output = String::new();
    let mut chars = input.chars().peekable();
    let mut depth = 0usize;
    while let Some(character) = chars.next() {
        match character {
            '{' => depth = depth.saturating_add(1),
            '}' => {
                if depth == 0 {
                    return Err("RTF group is unbalanced".into());
                }
                depth -= 1;
            }
            '\\' => match chars.peek().copied() {
                Some('\\' | '{' | '}') => output.push(chars.next().unwrap_or_default()),
                Some('\'') => {
                    chars.next();
                    let hex: String = chars.by_ref().take(2).collect();
                    if let Ok(value) = u8::from_str_radix(&hex, 16) {
                        output.push(char::from(value));
                    }
                }
                Some(_) => {
                    let mut word = String::new();
                    while chars
                        .peek()
                        .is_some_and(|value| value.is_ascii_alphabetic())
                    {
                        word.push(chars.next().unwrap_or_default());
                    }
                    let mut number = String::new();
                    while chars
                        .peek()
                        .is_some_and(|value| value.is_ascii_digit() || *value == '-')
                    {
                        number.push(chars.next().unwrap_or_default());
                    }
                    if chars.peek() == Some(&' ') {
                        chars.next();
                    }
                    match word.as_str() {
                        "par" | "line" => output.push('\n'),
                        "tab" => output.push('\t'),
                        "u" => {
                            if let Ok(value) = number.parse::<i32>() {
                                let scalar = if value < 0 { value + 65_536 } else { value };
                                if let Some(value) = char::from_u32(scalar as u32) {
                                    output.push(value);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                None => {}
            },
            '\r' | '\n' => {}
            value if !value.is_control() => output.push(value),
            _ => {}
        }
        if output.len() > MAX_EXTRACTED_TEXT_BYTES {
            return Err("RTF text exceeds extraction limit".into());
        }
    }
    if depth != 0 {
        return Err("RTF group is unbalanced".into());
    }
    Ok(output)
}

fn is_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08")
}

fn is_tar(bytes: &[u8]) -> bool {
    bytes.len() >= 512 && &bytes[257..262] == b"ustar"
}

fn is_image(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(b"\xff\xd8\xff")
        || bytes.starts_with(b"GIF87a")
        || bytes.starts_with(b"GIF89a")
        || (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"))
        || bytes.starts_with(b"BM")
        || bytes.starts_with(b"II*\0")
        || bytes.starts_with(b"MM\0*")
}

fn is_video(bytes: &[u8]) -> bool {
    (bytes.len() >= 12 && bytes.get(4..8) == Some(b"ftyp"))
        || bytes.starts_with(b"\x1a\x45\xdf\xa3")
        || (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"AVI "))
        || bytes.starts_with(b"\x00\x00\x01\xba")
}

fn is_audio(bytes: &[u8]) -> bool {
    bytes.starts_with(b"ID3")
        || bytes.starts_with(b"fLaC")
        || bytes.starts_with(b"OggS")
        || (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE"))
        || bytes.starts_with(&[0xff, 0xfb])
}

fn looks_like_text(text: &str) -> bool {
    !text.is_empty()
        && !text.contains('\0')
        && text
            .chars()
            .filter(|value| value.is_control() && !matches!(value, '\n' | '\r' | '\t'))
            .count()
            <= text.chars().count() / 100 + 1
}

fn looks_like_csv(text: &str) -> bool {
    let lines: Vec<_> = text
        .lines()
        .take(8)
        .filter(|line| !line.is_empty())
        .collect();
    lines.len() >= 2
        && [',', '\t', ';'].iter().any(|delimiter| {
            let count = lines[0].matches(*delimiter).count();
            count > 0
                && lines
                    .iter()
                    .all(|line| line.matches(*delimiter).count() == count)
        })
}

fn looks_like_markdown(text: &str) -> bool {
    text.lines().take(32).any(|line| {
        line.starts_with("# ")
            || line.starts_with("## ")
            || line.starts_with("- ")
            || line.starts_with("* ")
            || line.starts_with("> ")
            || (line.contains('[') && line.contains("]("))
    })
}

fn is_ooxml_embedded(name: &str) -> bool {
    [
        "word/media/",
        "word/embeddings/",
        "ppt/media/",
        "ppt/embeddings/",
        "xl/media/",
        "xl/embeddings/",
    ]
    .iter()
    .any(|prefix| name.starts_with(prefix))
}

fn contains_utf16le(bytes: &[u8], needle: &str) -> bool {
    let encoded: Vec<u8> = needle.encode_utf16().flat_map(u16::to_le_bytes).collect();
    bytes.windows(encoded.len()).any(|window| window == encoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Document, Object, Stream};
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn attachment(bytes: &[u8]) -> LocalAttachmentCandidate {
        LocalAttachmentCandidate {
            attachment_id: "attachment-1".into(),
            display_name: "misleading.bin".into(),
            content_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        }
    }

    fn zip_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut bytes);
            for (name, value) in entries {
                writer
                    .start_file(*name, SimpleFileOptions::default())
                    .expect("start ZIP entry");
                writer.write_all(value).expect("write ZIP entry");
            }
            writer.finish().expect("finish ZIP");
        }
        bytes.into_inner()
    }

    fn text_pdf(text: &str) -> Vec<u8> {
        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();
        let font_id = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Courier",
        });
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 12.into()]),
                Operation::new("Td", vec![100.into(), 700.into()]),
                Operation::new("Tj", vec![Object::string_literal(text)]),
                Operation::new("ET", vec![]),
            ],
        };
        let content_id = document.add_object(Stream::new(
            dictionary! {},
            content.encode().expect("encode PDF content"),
        ));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        document.save_to(&mut bytes).expect("save PDF");
        bytes
    }

    #[test]
    fn byte_signatures_override_misleading_names() {
        let result = scan_attachments(
            &[attachment(b"{\\rtf1 password: secret-value}")],
            AttachmentAnalyzers::default(),
        );
        assert!(result.attachment_types_scanned.contains(&"rtf".into()));
        assert!(result.extracted_text[0].text.contains("password"));
    }

    #[test]
    fn detects_registered_types_from_bytes_and_container_members() {
        assert_eq!(detect_type(b"%PDF-1.7\n"), DetectedType::Pdf);
        assert_eq!(detect_type(b"\x1f\x8b\x08\0"), DetectedType::Gzip);
        assert_eq!(detect_type(b"\x89PNG\r\n\x1a\n"), DetectedType::Image);
        assert_eq!(
            detect_type(b"\x00\x00\x00\x18ftypisom"),
            DetectedType::Video
        );
        assert_eq!(
            detect_type(&zip_with(&[("word/document.xml", b"<w:document/>")])),
            DetectedType::Docx
        );
        assert_eq!(ATTACHMENT_HANDLER_REGISTRY.len(), 16);
    }

    #[test]
    fn extracts_plain_markdown_csv_and_rtf_text() {
        for (bytes, expected) in [
            (b"password: alpha".as_slice(), "plain_text"),
            (b"# title\nsecret: beta".as_slice(), "markdown"),
            (b"kind,value\nsecret,gamma".as_slice(), "csv"),
            (b"{\\rtf1 secret: delta}".as_slice(), "rtf"),
        ] {
            let result = scan_attachments(&[attachment(bytes)], AttachmentAnalyzers::default());
            assert!(result.attachment_types_scanned.contains(&expected.into()));
            assert_eq!(result.uninspected_attachments.len(), 0);
        }
    }

    #[test]
    fn extracts_docx_and_pptx_text_without_system_libraries() {
        let docx = zip_with(&[(
            "word/document.xml",
            br#"<w:document xmlns:w="w"><w:body><w:p><w:r><w:t>secret: docx-value</w:t></w:r></w:p></w:body></w:document>"#,
        )]);
        let pptx = zip_with(&[
            ("ppt/presentation.xml", br#"<p:presentation xmlns:p="p"/>"#),
            (
                "ppt/slides/slide1.xml",
                br#"<a:t xmlns:a="a">secret: pptx-value</a:t>"#,
            ),
        ]);
        for (bytes, expected_type, needle) in
            [(docx, "docx", "docx-value"), (pptx, "pptx", "pptx-value")]
        {
            let result = scan_attachments(&[attachment(&bytes)], AttachmentAnalyzers::default());
            assert!(
                result
                    .attachment_types_scanned
                    .contains(&expected_type.into()),
                "missing type {expected_type}: {:?}",
                result.attachment_types_scanned
            );
            assert!(
                result
                    .extracted_text
                    .iter()
                    .any(|item| item.text.contains(needle)),
                "missing {needle}"
            );
        }
    }

    #[test]
    fn holds_pdf_and_xlsx_until_parsers_are_process_isolated() {
        let xlsx = zip_with(&[("xl/workbook.xml", b"<workbook/>")]);
        for (bytes, detected_type) in [(text_pdf("secret"), "pdf"), (xlsx, "xlsx")] {
            let result = scan_attachments(&[attachment(&bytes)], AttachmentAnalyzers::default());
            assert!(result.extracted_text.is_empty());
            assert!(result.uninspected_attachments.iter().any(|item| {
                item.detected_type == detected_type
                    && item.reason == UninspectedReason::Unsupported
                    && item.detail.contains("time- and memory-bounded worker")
            }));
        }
    }

    #[test]
    fn rejects_zip_path_traversal_but_scans_safe_entries() {
        let bytes = zip_with(&[
            ("../escape.txt", b"password: no"),
            ("safe.txt", b"secret: yes"),
        ]);
        let result = scan_attachments(&[attachment(&bytes)], AttachmentAnalyzers::default());
        assert!(result
            .uninspected_attachments
            .iter()
            .any(|item| item.reason == UninspectedReason::UnsafeArchiveEntry));
        assert!(result
            .extracted_text
            .iter()
            .any(|item| item.path.ends_with("safe.txt")));
    }

    #[test]
    fn rejects_archive_depth_over_limit() {
        let mut bytes = b"secret: deepest".to_vec();
        for depth in 0..=MAX_ARCHIVE_DEPTH {
            bytes = zip_with(&[(
                Box::leak(format!("level-{depth}.zip").into_boxed_str()),
                &bytes,
            )]);
        }
        let result = scan_attachments(&[attachment(&bytes)], AttachmentAnalyzers::default());
        assert!(result
            .uninspected_attachments
            .iter()
            .any(|item| item.reason == UninspectedReason::LimitExceeded
                && item.detail.contains("depth")));
    }

    #[test]
    fn rejects_expansion_over_hard_size_cap() {
        let oversized = vec![b'a'; MAX_TOTAL_EXPANDED_BYTES + 1];
        let bytes = zip_with(&[("large.txt", &oversized)]);
        let result = scan_attachments(&[attachment(&bytes)], AttachmentAnalyzers::default());
        assert!(result
            .uninspected_attachments
            .iter()
            .any(|item| item.reason == UninspectedReason::LimitExceeded));
        assert!(result.extracted_text.is_empty());
    }

    #[test]
    fn unavailable_media_capabilities_are_never_clean_results() {
        assert_eq!(
            AttachmentAnalyzers::default().capability_flags(),
            LocalMediaCapabilityFlags {
                image_classifier: false,
                video_frame_extractor: false,
                ocr: false,
                speech_to_text: false,
            }
        );
        let image = attachment(b"\x89PNG\r\n\x1a\nminimal");
        let video = attachment(b"\x00\x00\x00\x18ftypisomminimal");
        let image_result = scan_attachments(&[image], AttachmentAnalyzers::default());
        let video_result = scan_attachments(&[video], AttachmentAnalyzers::default());
        assert!(!image_result.images_checked);
        assert!(!video_result.videos_checked);
        assert_eq!(
            image_result.uninspected_attachments[0].reason,
            UninspectedReason::ModelNotInstalled
        );
        assert_eq!(
            video_result.uninspected_attachments[0].reason,
            UninspectedReason::DependencyNotInstalled
        );
    }

    struct TestClassifier;
    impl LocalImageClassifier for TestClassifier {
        fn classify(&self, _image: &[u8]) -> Result<ImageClassification, String> {
            Ok(ImageClassification {
                explicit: true,
                confidence: 81,
            })
        }
    }

    struct TestOcr;
    impl LocalOcrEngine for TestOcr {
        fn extract_text(&self, _image: &[u8]) -> Result<String, String> {
            Ok("password: from-ocr".into())
        }
    }

    struct TestFrames;
    impl LocalVideoFrameExtractor for TestFrames {
        fn keyframes(&self, _video: &[u8]) -> Result<Vec<MediaFrame>, String> {
            Ok(vec![MediaFrame {
                bytes: b"\x89PNG\r\n\x1a\nframe".to_vec(),
            }])
        }
    }

    struct TestTranscriber;
    impl LocalAudioTranscriber for TestTranscriber {
        fn transcribe(&self, _media: &[u8]) -> Result<String, String> {
            Ok("recovery phrase from transcript".into())
        }
    }

    #[test]
    fn available_local_media_interfaces_feed_ocr_and_transcripts_to_text_scan() {
        let analyzers = AttachmentAnalyzers {
            image_classifier: Some(&TestClassifier),
            ocr: Some(&TestOcr),
            video_frames: Some(&TestFrames),
            speech_to_text: Some(&TestTranscriber),
        };
        let result = scan_attachments(
            &[
                attachment(b"\x89PNG\r\n\x1a\nminimal"),
                attachment(b"\x00\x00\x00\x18ftypisomminimal"),
            ],
            analyzers,
        );
        assert!(result.images_checked);
        assert!(result.videos_checked);
        assert!(result.uninspected_attachments.is_empty());
        assert!(result
            .extracted_text
            .iter()
            .any(|item| item.text.contains("from-ocr")));
        assert!(result
            .extracted_text
            .iter()
            .any(|item| item.text.contains("recovery phrase")));
        assert_eq!(result.explicit_media_signals.len(), 2);
    }
}
