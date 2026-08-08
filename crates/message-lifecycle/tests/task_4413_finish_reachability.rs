use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const DELIBERATE_MARKER: &str = "OSL-FINISH-ONLY-TESTS-DELIBERATE:";

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct FunctionDef {
    name: String,
    arity: usize,
    path: String,
    line: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CallSite {
    path: String,
    line: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Finding {
    def: FunctionDef,
    test_calls: Vec<CallSite>,
    deliberate: Option<String>,
}

#[test]
fn task_4413_finish_functions_reached_only_by_tests_are_counted_and_capped() {
    let repo = repo_root();
    let report = scan_repo(&repo);
    let before_count = report.len();
    let unmarked = report
        .iter()
        .filter(|finding| finding.deliberate.is_none())
        .collect::<Vec<_>>();
    let after_count = unmarked.len();

    println!("TASK4413_BEFORE_COUNT={before_count}");
    for finding in &report {
        println!(
            "TASK4413_FOUND {}:{} {} arity={} test_calls={}",
            finding.def.path,
            finding.def.line,
            finding.def.name,
            finding.def.arity,
            finding.test_calls.len()
        );
        if let Some(deliberate) = &finding.deliberate {
            println!(
                "TASK4413_DELIBERATE {}:{} {}",
                finding.def.path, finding.def.line, deliberate
            );
        }
    }
    println!("TASK4413_AFTER_COUNT={after_count}");

    assert!(
        before_count >= 4,
        "finish-only-test census must prove the pre-marker count is at least 4, got {before_count}"
    );
    assert!(
        unmarked.is_empty(),
        "new finish-only-test functions need a {DELIBERATE_MARKER} line: {unmarked:#?}"
    );

    let mut synthetic = SyntheticRepo::new();
    synthetic.add_source(
        "src/worker.rs",
        r#"
pub fn finish_orphaned_piece(id: &str) -> String {
    format!("finished {id}")
}
"#,
    );
    synthetic.add_source(
        "tests/worker.rs",
        r#"
#[test]
fn reaches_only_from_test() {
    assert_eq!(finish_orphaned_piece("x"), "finished x");
}
"#,
    );
    let red = scan_sources(synthetic.sources());
    let red_unmarked = red
        .iter()
        .filter(|finding| finding.deliberate.is_none())
        .count();
    println!("TASK4413_SYNTHETIC_NEW_FINISHED_PIECES={red_unmarked}");
    assert_eq!(
        red_unmarked, 1,
        "adding one new finish-only-test function must make the check red"
    );
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("test manifest lives two levels under repo root")
        .to_path_buf()
}

fn scan_repo(repo: &Path) -> Vec<Finding> {
    let mut sources = Vec::new();
    for root in [
        repo.join("apps/osl-hub/src"),
        repo.join("apps/osl-hub/tests"),
        repo.join("crates"),
    ] {
        collect_rs_files(repo, &root, &mut sources);
    }
    scan_sources(sources)
}

fn collect_rs_files(repo: &Path, path: &Path, out: &mut Vec<(String, String)>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(repo, &path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let relative = path
                .strip_prefix(repo)
                .expect("walked path must stay under repo root")
                .to_string_lossy()
                .replace('\\', "/");
            let text = fs::read_to_string(&path).expect("Rust source must be readable");
            out.push((relative, text));
        }
    }
}

fn scan_sources(sources: Vec<(String, String)>) -> Vec<Finding> {
    let mut defs = Vec::new();
    let mut definition_lines = BTreeSet::new();
    let mut test_ranges = BTreeMap::new();

    for (path, text) in &sources {
        let ranges = cfg_test_ranges(text);
        test_ranges.insert(path.clone(), ranges.clone());
        for def in find_defs(path, text) {
            if is_test_line(path, def.line, &ranges) {
                continue;
            }
            definition_lines.insert((def.path.clone(), def.line, def.name.clone()));
            defs.push(def);
        }
    }

    let mut findings = Vec::new();
    for def in defs {
        let mut production_calls = Vec::new();
        let mut test_calls = Vec::new();
        for (path, text) in &sources {
            for call in find_calls(path, text, &def.name, def.arity) {
                if definition_lines.contains(&(call.path.clone(), call.line, def.name.clone())) {
                    continue;
                }
                if is_test_line(path, call.line, test_ranges.get(path).unwrap()) {
                    test_calls.push(call);
                } else {
                    production_calls.push(call);
                }
            }
        }
        if production_calls.is_empty() && !test_calls.is_empty() {
            let deliberate = deliberate_line_for(&def, &sources);
            findings.push(Finding {
                def,
                test_calls,
                deliberate,
            });
        }
    }
    findings.sort_by(|left, right| left.def.cmp(&right.def));
    findings
}

fn find_defs(path: &str, text: &str) -> Vec<FunctionDef> {
    let mut defs = Vec::new();
    let bytes = text.as_bytes();
    for (line_index, line) in text.lines().enumerate() {
        let Some(fn_offset) = line.find("fn ") else {
            continue;
        };
        let before = &line[..fn_offset];
        if before.chars().any(|ch| {
            !(ch.is_whitespace()
                || ch == 'p'
                || ch == 'u'
                || ch == 'b'
                || ch == '('
                || ch == ')'
                || ch == ':'
                || ch == 'c'
                || ch == 'r'
                || ch == 'a'
                || ch == 't'
                || ch == 'e')
        }) {
            continue;
        }
        let name_start = line_start_offset(text, line_index + 1) + fn_offset + 3;
        let Some((name, params_start)) = parse_ident_after(bytes, name_start) else {
            continue;
        };
        if !name.contains("finish") || name == "finish" {
            continue;
        }
        let Some((params, _end)) = balanced_after(bytes, params_start, b'(', b')') else {
            continue;
        };
        defs.push(FunctionDef {
            name,
            arity: parameter_arity(params),
            path: path.to_owned(),
            line: line_index + 1,
        });
    }
    defs
}

fn find_calls(path: &str, text: &str, name: &str, arity: usize) -> Vec<CallSite> {
    let mut calls = Vec::new();
    let bytes = text.as_bytes();
    let mut offset = 0;
    while let Some(relative) = text[offset..].find(name) {
        let start = offset + relative;
        let end = start + name.len();
        if !is_ident_boundary(bytes, start, end) {
            offset = end;
            continue;
        }
        let after = skip_ws(bytes, end);
        if bytes.get(after) != Some(&b'(') {
            offset = end;
            continue;
        }
        let Some((args, _args_end)) = balanced_after(bytes, after, b'(', b')') else {
            offset = end;
            continue;
        };
        if argument_arity(args) == arity {
            calls.push(CallSite {
                path: path.to_owned(),
                line: line_for_offset(text, start),
            });
        }
        offset = end;
    }
    calls
}

fn parse_ident_after(bytes: &[u8], mut offset: usize) -> Option<(String, usize)> {
    offset = skip_ws(bytes, offset);
    let start = offset;
    while bytes
        .get(offset)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        offset += 1;
    }
    if start == offset {
        return None;
    }
    let name = std::str::from_utf8(&bytes[start..offset]).ok()?.to_owned();
    offset = skip_ws(bytes, offset);
    if bytes.get(offset) == Some(&b'<') {
        let (_, after_generics) = balanced_after(bytes, offset, b'<', b'>')?;
        offset = skip_ws(bytes, after_generics);
    }
    Some((name, offset))
}

fn balanced_after(bytes: &[u8], start: usize, open: u8, close: u8) -> Option<(&str, usize)> {
    if bytes.get(start) != Some(&open) {
        return None;
    }
    let mut depth = 0usize;
    for index in start..bytes.len() {
        match bytes[index] {
            byte if byte == open => depth += 1,
            byte if byte == close => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let inner = std::str::from_utf8(&bytes[start + 1..index]).ok()?;
                    return Some((inner, index + 1));
                }
            }
            _ => {}
        }
    }
    None
}

fn parameter_arity(params: &str) -> usize {
    split_top_level(params)
        .into_iter()
        .filter(|param| {
            let trimmed = param.trim();
            !trimmed.is_empty()
                && trimmed != "self"
                && trimmed != "&self"
                && trimmed != "&mut self"
                && !trimmed.starts_with("mut self")
        })
        .count()
}

fn argument_arity(args: &str) -> usize {
    split_top_level(args)
        .into_iter()
        .filter(|arg| !arg.trim().is_empty())
        .count()
}

fn split_top_level(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut paren = 0i32;
    let mut angle = 0i32;
    let mut brace = 0i32;
    let mut bracket = 0i32;
    for (index, ch) in text.char_indices() {
        match ch {
            '(' => paren += 1,
            ')' => paren -= 1,
            '<' => angle += 1,
            '>' => angle -= 1,
            '{' => brace += 1,
            '}' => brace -= 1,
            '[' => bracket += 1,
            ']' => bracket -= 1,
            ',' if paren == 0 && angle == 0 && brace == 0 && bracket == 0 => {
                parts.push(&text[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

fn cfg_test_ranges(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut ranges = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        if !line.contains("mod ") {
            continue;
        }
        let previous = text
            .lines()
            .take(line_index + 1)
            .skip(line_index.saturating_sub(6))
            .collect::<Vec<_>>()
            .join("\n");
        if !previous.contains("cfg(test)") {
            continue;
        }
        let line_start = line_start_offset(text, line_index + 1);
        let Some(open_relative) = text[line_start..].find('{') else {
            continue;
        };
        let open = line_start + open_relative;
        let Some((_body, end)) = balanced_after(bytes, open, b'{', b'}') else {
            continue;
        };
        ranges.push((line_index + 1, line_for_offset(text, end)));
    }
    ranges
}

fn is_test_line(path: &str, line: usize, ranges: &[(usize, usize)]) -> bool {
    path.starts_with("tests/")
        || path.contains("/tests/")
        || ranges
            .iter()
            .any(|(start, end)| *start <= line && line <= *end)
}

fn deliberate_line_for(def: &FunctionDef, sources: &[(String, String)]) -> Option<String> {
    let (_, text) = sources.iter().find(|(path, _)| path == &def.path)?;
    let lines = text.lines().collect::<Vec<_>>();
    let start = def.line.saturating_sub(7);
    let needle = format!("{}/{}", def.name, def.arity);
    lines[start..def.line]
        .iter()
        .find(|line| line.contains(DELIBERATE_MARKER) && line.contains(&needle))
        .map(|line| line.trim().to_owned())
}

fn line_start_offset(text: &str, line: usize) -> usize {
    let mut offset = 0;
    for current in 1..line {
        if let Some(next) = text[offset..].find('\n') {
            offset += next + 1;
        } else if current < line {
            return text.len();
        }
    }
    offset
}

fn line_for_offset(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

fn skip_ws(bytes: &[u8], mut offset: usize) -> usize {
    while bytes
        .get(offset)
        .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        offset += 1;
    }
    offset
}

fn is_ident_boundary(bytes: &[u8], start: usize, end: usize) -> bool {
    let before = start
        .checked_sub(1)
        .and_then(|index| bytes.get(index))
        .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_');
    let after = bytes
        .get(end)
        .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_');
    before && after
}

struct SyntheticRepo {
    sources: Vec<(String, String)>,
}

impl SyntheticRepo {
    fn new() -> Self {
        Self {
            sources: Vec::new(),
        }
    }

    fn add_source(&mut self, path: &str, text: &str) {
        self.sources.push((path.to_owned(), text.to_owned()));
    }

    fn sources(self) -> Vec<(String, String)> {
        self.sources
    }
}
