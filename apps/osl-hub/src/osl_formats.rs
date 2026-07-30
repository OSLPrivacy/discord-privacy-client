//! Bounded, local-only import adapters for encrypted OSL Notes sources.

use calamine::{open_workbook_auto_from_rs, Data, Range, Reader};
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use zeroize::Zeroize;

const MAX_SOURCE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_ROWS: usize = 200;
const MAX_COLUMNS: usize = 50;
const MAX_CELL_BYTES: usize = 2_000;
const MAX_BODY_CELL_BYTES: usize = 200 * 1024;
const MAX_EXTRACTED_XML_BYTES: u64 = 16 * 1024 * 1024;
const MAX_DOCUMENT_BYTES: usize = 240 * 1024;

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OslOfficeImport {
    pub kind: &'static str,
    pub title: String,
    pub body: String,
    pub folder: &'static str,
    pub tags: Vec<&'static str>,
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SheetModel {
    version: u8,
    rows: usize,
    columns: usize,
    cells: BTreeMap<String, String>,
    formats: BTreeMap<String, String>,
    frozen_rows: usize,
    frozen_columns: usize,
    filters: BTreeMap<String, String>,
    sort: Option<SheetSort>,
}

#[derive(Serialize)]
struct SheetSort {
    column: usize,
    direction: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PresentationModel {
    version: u8,
    selected_id: String,
    slides: Vec<ImportedSlide>,
    master: PresentationMaster,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PresentationMaster {
    footer: String,
    show_numbers: bool,
}

#[derive(Serialize)]
struct ImportedSlide {
    id: String,
    title: String,
    body: String,
    theme: &'static str,
    layout: &'static str,
    notes: String,
    transition: &'static str,
    duration: f32,
}

#[derive(Serialize)]
struct DocumentModel {
    format: &'static str,
    page: DocumentPage,
    blocks: Vec<DocumentBlock>,
}

#[derive(Serialize)]
struct DocumentPage {
    size: &'static str,
    orientation: &'static str,
    margin: f32,
    columns: u8,
    header: String,
    footer: String,
}

#[derive(Serialize)]
struct DocumentBlock {
    id: String,
    #[serde(rename = "type")]
    kind: &'static str,
    text: String,
    checked: bool,
    align: &'static str,
}

pub fn decode_office_asset(asset_id: &str) -> Result<OslOfficeImport, String> {
    let (asset, mut bytes) = crate::osl_assets::read_bounded(asset_id, MAX_SOURCE_BYTES)?;
    let extension = asset
        .name
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let result = match extension.as_str() {
        "xls" | "xlsx" | "xlsb" | "ods" => decode_spreadsheet(&asset.name, &bytes),
        "docx" | "odt" => decode_document(&asset.name, &bytes),
        "pptx" | "odp" => decode_presentation(&asset.name, &bytes),
        _ => Err("That source is not a supported editable Office file".into()),
    };
    bytes.zeroize();
    result
}

fn decode_spreadsheet(name: &str, bytes: &[u8]) -> Result<OslOfficeImport, String> {
    let extension = name
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(extension.as_str(), "xls" | "xlsx" | "xlsb" | "ods") {
        return Err("That source is not a supported spreadsheet file".into());
    }
    let mut workbook = open_workbook_auto_from_rs(Cursor::new(bytes)).map_err(|_| {
        "That spreadsheet is malformed, encrypted with another password, or unsupported".to_owned()
    })?;
    let sheet_names = workbook.sheet_names();
    let sheet_name = sheet_names
        .first()
        .ok_or_else(|| "That spreadsheet does not contain a worksheet".to_owned())?;
    let values = workbook
        .worksheet_range(sheet_name)
        .map_err(|_| "The first worksheet could not be decoded".to_owned())?;
    let formulas = workbook.worksheet_formula(sheet_name).ok();
    let (model, mut warnings) = sheet_model(&values, formulas.as_ref())?;
    if sheet_names.len() > 1 {
        warnings.push(format!(
            "Imported the first worksheet ({sheet_name}); {} additional worksheets remain in the encrypted original.",
            sheet_names.len() - 1
        ));
    }
    warnings.push("Formatting, charts, macros, comments, and advanced formulas remain in the encrypted original and may not be editable yet.".into());
    let body = serde_json::to_string(&model)
        .map_err(|_| "The decoded spreadsheet could not be prepared".to_owned())?;
    let title = name
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(name)
        .trim()
        .chars()
        .take(60)
        .collect::<String>();
    Ok(OslOfficeImport {
        kind: "spreadsheet",
        title: if title.is_empty() {
            "Imported spreadsheet".into()
        } else {
            title
        },
        body,
        folder: "Imports",
        tags: vec!["imported", "spreadsheet"],
        warnings,
    })
}

fn sheet_model(
    values: &Range<Data>,
    formulas: Option<&Range<String>>,
) -> Result<(SheetModel, Vec<String>), String> {
    let end = values.end().unwrap_or((0, 0));
    let used_rows = usize::try_from(end.0)
        .unwrap_or(usize::MAX)
        .saturating_add(1);
    let used_columns = usize::try_from(end.1)
        .unwrap_or(usize::MAX)
        .saturating_add(1);
    let rows = used_rows.clamp(20, MAX_ROWS);
    let columns = used_columns.clamp(8, MAX_COLUMNS);
    let mut cells = BTreeMap::new();
    let mut stored_bytes = 0usize;
    let mut stopped_for_body_limit = false;
    let mut saw_formula = false;
    'rows: for row in 0..used_rows.min(MAX_ROWS) {
        for column in 0..used_columns.min(MAX_COLUMNS) {
            let formula = formulas
                .and_then(|range| range.get_value((row as u32, column as u32)))
                .filter(|value| !value.is_empty());
            let mut text = if let Some(formula) = formula {
                saw_formula = true;
                format!("={}", formula.trim_start_matches('='))
            } else {
                values
                    .get_value((row as u32, column as u32))
                    .map(ToString::to_string)
                    .unwrap_or_default()
            };
            if text.is_empty() {
                continue;
            }
            while text.len() > MAX_CELL_BYTES {
                text.pop();
            }
            if stored_bytes.saturating_add(text.len()) > MAX_BODY_CELL_BYTES {
                stopped_for_body_limit = true;
                break 'rows;
            }
            stored_bytes += text.len();
            cells.insert(format!("{row}:{column}"), text);
        }
    }
    let mut warnings = Vec::new();
    if used_rows > MAX_ROWS || used_columns > MAX_COLUMNS {
        warnings.push(format!(
            "The editable view is limited to {MAX_ROWS} rows by {MAX_COLUMNS} columns; the complete workbook remains in the encrypted original."
        ));
    }
    if stopped_for_body_limit {
        warnings.push("The editable cell content limit was reached; remaining content stays in the encrypted original.".into());
    }
    if saw_formula {
        warnings.push("Formulas were preserved, but OSL currently calculates only arithmetic, cell references, SUM, AVERAGE, MIN, and MAX.".into());
    }
    Ok((
        SheetModel {
            version: 2,
            rows,
            columns,
            cells,
            formats: BTreeMap::new(),
            frozen_rows: 0,
            frozen_columns: 0,
            filters: BTreeMap::new(),
            sort: None,
        },
        warnings,
    ))
}

fn imported_title(name: &str, fallback: &str) -> String {
    let title = name
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(name)
        .trim()
        .chars()
        .take(60)
        .collect::<String>();
    if title.is_empty() {
        fallback.into()
    } else {
        title
    }
}

fn open_zip(bytes: &[u8]) -> Result<zip::ZipArchive<Cursor<&[u8]>>, String> {
    zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| {
        "That Office package is malformed, password-protected, or unsupported".to_owned()
    })
}

fn read_zip_entry<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<Vec<u8>, String> {
    let file = archive
        .by_name(name)
        .map_err(|_| "That Office package is missing required document content".to_owned())?;
    if file.size() > MAX_EXTRACTED_XML_BYTES {
        return Err(
            "That Office document content is too large for the bounded local decoder".into(),
        );
    }
    let mut bytes = Vec::with_capacity(usize::try_from(file.size()).unwrap_or(0));
    file.take(MAX_EXTRACTED_XML_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "That Office document content could not be read".to_owned())?;
    if bytes.len() as u64 > MAX_EXTRACTED_XML_BYTES {
        return Err("That Office document content exceeds the bounded decoder limit".into());
    }
    Ok(bytes)
}

fn xml_reference(value: &str) -> Result<char, String> {
    match value {
        "amp" => Ok('&'),
        "lt" => Ok('<'),
        "gt" => Ok('>'),
        "apos" => Ok('\''),
        "quot" => Ok('"'),
        _ => {
            let number = value
                .strip_prefix("#x")
                .and_then(|digits| u32::from_str_radix(digits, 16).ok())
                .or_else(|| {
                    value
                        .strip_prefix('#')
                        .and_then(|digits| digits.parse().ok())
                });
            number
                .and_then(char::from_u32)
                .ok_or_else(|| "The Office document contains an unsupported XML entity".to_owned())
        }
    }
}

fn xml_text(
    xml: &[u8],
    run_suffix: &[u8],
    paragraph_suffix: &[u8],
) -> Result<(String, bool), String> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut result = String::new();
    let mut in_run = false;
    let mut truncated = false;
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => {
                let name = event.name();
                if name.as_ref().ends_with(run_suffix) {
                    in_run = true;
                }
            }
            Ok(Event::Empty(event)) if event.name().as_ref().ends_with(b":tab") => {
                if result.len() < MAX_DOCUMENT_BYTES {
                    result.push('\t');
                }
            }
            Ok(Event::Text(event)) if in_run => {
                let decoded = event
                    .decode()
                    .map_err(|_| "The Office document contains invalid text".to_owned())?;
                let unescaped = quick_xml::escape::unescape(&decoded)
                    .map_err(|_| "The Office document contains invalid XML entities".to_owned())?;
                for character in unescaped.chars() {
                    if result.len() + character.len_utf8() > MAX_DOCUMENT_BYTES {
                        truncated = true;
                        break;
                    }
                    result.push(character);
                }
            }
            Ok(Event::GeneralRef(event)) if in_run => {
                let decoded = event
                    .decode()
                    .map_err(|_| "The Office document contains an invalid XML entity".to_owned())?;
                let character = xml_reference(&decoded)?;
                if result.len() + character.len_utf8() <= MAX_DOCUMENT_BYTES {
                    result.push(character);
                } else {
                    truncated = true;
                }
            }
            Ok(Event::End(event)) => {
                let name = event.name();
                if name.as_ref().ends_with(run_suffix) {
                    in_run = false;
                }
                if name.as_ref().ends_with(paragraph_suffix)
                    && !result.ends_with('\n')
                    && result.len() < MAX_DOCUMENT_BYTES
                {
                    result.push('\n');
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return Err("The Office document XML is malformed".into()),
            _ => {}
        }
        if truncated {
            break;
        }
        buffer.clear();
    }
    Ok((result.trim().to_owned(), truncated))
}

fn xml_pages(xml: &[u8]) -> Result<(Vec<Vec<String>>, bool), String> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut pages = Vec::new();
    let mut page = Vec::new();
    let mut paragraph = String::new();
    let mut in_page = false;
    let mut in_paragraph = false;
    let mut stored = 0usize;
    let mut truncated = false;
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => {
                let name = event.name();
                if name.as_ref().ends_with(b":page") {
                    in_page = true;
                    page.clear();
                }
                if in_page && name.as_ref().ends_with(b":p") {
                    in_paragraph = true;
                    paragraph.clear();
                }
            }
            Ok(Event::Text(event)) if in_paragraph => {
                let decoded = event
                    .decode()
                    .map_err(|_| "The presentation contains invalid text".to_owned())?;
                let unescaped = quick_xml::escape::unescape(&decoded)
                    .map_err(|_| "The presentation contains invalid XML entities".to_owned())?;
                for character in unescaped.chars() {
                    if stored + character.len_utf8() > MAX_DOCUMENT_BYTES {
                        truncated = true;
                        break;
                    }
                    paragraph.push(character);
                    stored += character.len_utf8();
                }
            }
            Ok(Event::GeneralRef(event)) if in_paragraph => {
                let decoded = event
                    .decode()
                    .map_err(|_| "The presentation contains an invalid XML entity".to_owned())?;
                let character = xml_reference(&decoded)?;
                if stored + character.len_utf8() <= MAX_DOCUMENT_BYTES {
                    paragraph.push(character);
                    stored += character.len_utf8();
                } else {
                    truncated = true;
                }
            }
            Ok(Event::End(event)) => {
                let name = event.name();
                if name.as_ref().ends_with(b":p") && in_paragraph {
                    let text = paragraph.trim();
                    if !text.is_empty() {
                        page.push(text.to_owned());
                    }
                    in_paragraph = false;
                }
                if name.as_ref().ends_with(b":page") && in_page {
                    pages.push(std::mem::take(&mut page));
                    in_page = false;
                    if pages.len() >= 200 {
                        truncated = true;
                        break;
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => return Err("The presentation XML is malformed".into()),
            _ => {}
        }
        if truncated {
            break;
        }
        buffer.clear();
    }
    Ok((pages, truncated))
}

fn decode_document(name: &str, bytes: &[u8]) -> Result<OslOfficeImport, String> {
    let extension = name
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mut archive = open_zip(bytes)?;
    let xml = read_zip_entry(
        &mut archive,
        if extension == "docx" {
            "word/document.xml"
        } else {
            "content.xml"
        },
    )?;
    let (body, truncated) = if extension == "docx" {
        xml_text(&xml, b":t", b":p")?
    } else {
        xml_text(&xml, b":p", b":p")?
    };
    if body.is_empty() {
        return Err("That document does not contain editable text".into());
    }
    let mut warnings = vec!["The editable import preserves text. Page layout, tracked changes, comments, fields, embedded media, and advanced formatting remain in the encrypted original.".into()];
    if truncated {
        warnings.insert(0, "The editable text limit was reached; remaining content stays in the encrypted original.".into());
    }
    let blocks = body
        .split('\n')
        .enumerate()
        .map(|(index, text)| DocumentBlock {
            id: format!("block{index:04}"),
            kind: "paragraph",
            text: text.to_owned(),
            checked: false,
            align: "left",
        })
        .collect();
    let body = serde_json::to_string(&DocumentModel {
        format: "osl-document-v2",
        page: DocumentPage {
            size: "letter",
            orientation: "portrait",
            margin: 1.0,
            columns: 1,
            header: String::new(),
            footer: String::new(),
        },
        blocks,
    })
    .map_err(|_| "The decoded document could not be prepared".to_owned())?;
    Ok(OslOfficeImport {
        kind: "document",
        title: imported_title(name, "Imported document"),
        body,
        folder: "Imports",
        tags: vec!["imported", "document"],
        warnings,
    })
}

fn slide_number(name: &str) -> usize {
    name.strip_prefix("ppt/slides/slide")
        .and_then(|value| value.strip_suffix(".xml"))
        .and_then(|value| value.parse().ok())
        .unwrap_or(usize::MAX)
}

fn decode_presentation(name: &str, bytes: &[u8]) -> Result<OslOfficeImport, String> {
    let extension = name
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mut archive = open_zip(bytes)?;
    let mut slides = Vec::new();
    let mut truncated = false;
    if extension == "pptx" {
        let mut names = (0..archive.len())
            .filter_map(|index| {
                archive
                    .by_index(index)
                    .ok()
                    .map(|file| file.name().to_owned())
            })
            .filter(|entry| {
                entry.starts_with("ppt/slides/slide")
                    && entry.ends_with(".xml")
                    && slide_number(entry) != usize::MAX
            })
            .collect::<Vec<_>>();
        names.sort_by_key(|entry| slide_number(entry));
        for (index, entry) in names.into_iter().take(200).enumerate() {
            let xml = read_zip_entry(&mut archive, &entry)?;
            let (text, cut) = xml_text(&xml, b":t", b":p")?;
            truncated |= cut;
            let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
            let title = lines
                .next()
                .unwrap_or("Untitled slide")
                .chars()
                .take(100)
                .collect();
            let body = lines
                .collect::<Vec<_>>()
                .join("\n")
                .chars()
                .take(5_000)
                .collect();
            slides.push(ImportedSlide {
                id: format!("slide{index:04}"),
                title,
                body,
                theme: "cyan",
                layout: "content",
                notes: String::new(),
                transition: "none",
                duration: 5.0,
            });
        }
    } else {
        let xml = read_zip_entry(&mut archive, "content.xml")?;
        let (pages, cut) = xml_pages(&xml)?;
        truncated = cut;
        for (index, page) in pages
            .into_iter()
            .filter(|page| !page.is_empty())
            .enumerate()
        {
            let mut lines = page.into_iter();
            let title = lines
                .next()
                .unwrap_or_else(|| "Untitled slide".into())
                .chars()
                .take(100)
                .collect();
            let body = lines
                .collect::<Vec<_>>()
                .join("\n")
                .chars()
                .take(5_000)
                .collect();
            slides.push(ImportedSlide {
                id: format!("slide{index:04}"),
                title,
                body,
                theme: "cyan",
                layout: "content",
                notes: String::new(),
                transition: "none",
                duration: 5.0,
            });
        }
    }
    if slides.is_empty() {
        return Err("That presentation does not contain editable slide text".into());
    }
    let selected_id = slides[0].id.clone();
    let body = serde_json::to_string(&PresentationModel {
        version: 2,
        selected_id,
        slides,
        master: PresentationMaster {
            footer: String::new(),
            show_numbers: true,
        },
    })
    .map_err(|_| "The decoded presentation could not be prepared".to_owned())?;
    let mut warnings = vec!["The editable import preserves slide text. Layouts, fonts, transitions, animations, charts, media, and speaker notes remain in the encrypted original.".into()];
    if truncated {
        warnings.insert(0, "The editable slide-text limit was reached; remaining content stays in the encrypted original.".into());
    }
    Ok(OslOfficeImport {
        kind: "presentation",
        title: imported_title(name, "Imported presentation"),
        body,
        folder: "Imports",
        tags: vec!["imported", "presentation"],
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_values_and_formulas_into_the_bounded_editor_model() {
        let mut values = Range::new((0, 0), (1, 1));
        values.set_value((0, 0), Data::String("Revenue".into()));
        values.set_value((1, 0), Data::Int(20));
        let mut formulas = Range::new((0, 0), (1, 1));
        formulas.set_value((1, 1), "SUM(A2:A2)".into());
        let (model, warnings) = sheet_model(&values, Some(&formulas)).unwrap();
        assert_eq!(model.cells.get("0:0").map(String::as_str), Some("Revenue"));
        assert_eq!(model.cells.get("1:0").map(String::as_str), Some("20"));
        assert_eq!(
            model.cells.get("1:1").map(String::as_str),
            Some("=SUM(A2:A2)")
        );
        assert!(warnings.iter().any(|warning| warning.contains("Formulas")));
    }

    #[test]
    fn rejects_non_spreadsheet_extensions_before_decoding() {
        assert_eq!(
            decode_spreadsheet("notes.txt", b"nope").unwrap_err(),
            "That source is not a supported spreadsheet file"
        );
    }

    #[test]
    fn extracts_docx_runs_without_executing_xml_content() {
        let xml = br#"<w:document xmlns:w="safe"><w:p><w:r><w:t>Hello &amp; goodbye</w:t></w:r></w:p><w:p><w:r><w:t>Second line</w:t></w:r></w:p></w:document>"#;
        let (text, truncated) = xml_text(xml, b":t", b":p").unwrap();
        assert_eq!(text, "Hello & goodbye\nSecond line");
        assert!(!truncated);
    }

    #[test]
    fn keeps_open_document_pages_separate() {
        let xml = br#"<office:presentation xmlns:office="safe" xmlns:draw="safe" xmlns:text="safe"><draw:page><text:p>Title one</text:p><text:p>Body one</text:p></draw:page><draw:page><text:p>Title two</text:p></draw:page></office:presentation>"#;
        let (pages, truncated) = xml_pages(xml).unwrap();
        assert_eq!(
            pages,
            vec![vec!["Title one", "Body one"], vec!["Title two"]]
        );
        assert!(!truncated);
    }
}
