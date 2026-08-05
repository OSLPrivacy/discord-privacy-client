//! The **seam contract** a live carry receipt is bound to.
//!
//! D-206 built the carry receipt and bound it, by content hash, to **both** the
//! provider's adapter source **and** the whole of the shared substrate
//! (`src/native_a11y.rs`). That binding is what stops a proof from being a thing
//! you register once — but it does not scale. The owner has ruled the surface
//! list at Discord, Signal, WhatsApp, Telegram and ~10 email surfaces, so
//! roughly 12-14 published providers; with a whole-file binding, **one edit to
//! `native_a11y.rs` invalidates every receipt at once** and demands 12-14 live
//! signed-in Windows runs before any merge. That makes the shared substrate
//! effectively unrefactorable -- and the substrate is exactly the file the
//! placement-primitive work still has to change (D-204, D-211, D-220).
//!
//! This module replaces "the substrate's bytes" with "the substrate's **seam**",
//! defined as:
//!
//! > every item of the substrate that **this provider's own source refers to**,
//! > rendered as a *declaration* -- signatures, field and variant lists, constant
//! > values, and the signatures of every `impl` on a referenced type -- with
//! > function **bodies dropped**, comments stripped and whitespace canonical.
//!
//! Two consequences, and they are the two the owner asked to be provable:
//!
//! 1. A behaviour-preserving substrate refactor -- a comment, a reflow, a renamed
//!    local -- changes no declaration, so the contract hash does not move and no
//!    receipt goes stale.
//! 2. A change that alters the seam -- a signature, a return type, the name (and
//!    so the meaning) of a parameter a provider depends on, a new or removed enum
//!    variant, a struct field, a measured constant -- changes a declaration, so
//!    the hash moves and every receipt bound to it goes `Stale`.
//!
//! **The set is derived, not declared.** It comes from the adapter's own `use
//! crate::native_a11y::...` trees and `native_a11y::...` paths, so a provider
//! that starts depending on a new substrate item widens its own contract without
//! anybody remembering to. A reference that cannot be resolved to a real
//! declaration -- including a glob import -- is a **refusal**, not a skip: the
//! contract is never quietly computed over less than the provider uses.
//!
//! **What this deliberately does NOT catch, stated rather than papered over:** a
//! substrate edit that changes *behaviour* while leaving every declaration
//! identical -- `value_of` growing a cache that hands back what `set_value`
//! stored would be exactly D-206's founding failure -- does not move this hash.
//! Behaviour-preserving is not decidable from text, and requirement (1) forbids
//! binding to bodies. The whole-file hash is therefore **kept** in the receipt
//! and still computed; it is reported as substrate *drift* rather than `Stale`,
//! is printed by the fleet report and by every gate failure, and is what an
//! operator reads when deciding whether a substrate change needs re-proving that
//! the declarations alone cannot show.

use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

/// The substrate module every `Uia2Substrate` seam is cut from.
pub const SUBSTRATE_MODULE: &str = "native_a11y";
/// Path of that module's source, relative to `apps/osl-hub`.
pub const SUBSTRATE_SOURCE: &str = "src/native_a11y.rs";

/// A contract with fewer items than this is not a contract, it is an extractor
/// that has stopped working. Telegram resolves far more than this today.
/// Checked when the contract is computed **and** re-checked against the number a
/// receipt records, so a contract that silently collapses cannot be presented as
/// a proof.
pub const SEAM_CONTRACT_MIN_ITEMS: usize = 12;

/// Version tag mixed into the rendered contract, so a change to the *extraction
/// rules* also invalidates receipts. Without it a rule change would silently
/// reinterpret every hash already recorded.
pub const SEAM_CONTRACT_RENDERING: &str = "osl-seam-contract-v1";

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// One declaration the seam is made of.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SeamItem {
    /// `Uia2Syscalls`, `win32::Uia2Win32Host`, `impl Uia2WindowPlan`, ...
    pub path: String,
    pub kind: &'static str,
    /// Normalised declaration text: no comments, canonical whitespace, no
    /// function bodies.
    pub declaration: String,
}

/// The seam contract for one provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeamContract {
    pub items: Vec<SeamItem>,
    pub rendered: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeamContractError {
    /// The adapter refers to nothing in the substrate. Either the adapter does
    /// not use this seam or the reference scanner has stopped matching; both are
    /// refusals, because a contract over nothing would accept anything.
    NoReferences,
    /// Names the adapter refers to that resolve to no declaration in the
    /// substrate -- including `*` for a glob import, which cannot be resolved by
    /// name and would silently under-cover the seam.
    Unresolved(Vec<String>),
    /// The extractor produced a contract too small to be believable.
    TooFewItems {
        found: usize,
        minimum: usize,
    },
    Unreadable(String),
}

impl std::fmt::Display for SeamContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoReferences => write!(
                f,
                "the adapter refers to nothing in {SUBSTRATE_MODULE}, so there is no seam to bind"
            ),
            Self::Unresolved(names) => write!(
                f,
                "these {SUBSTRATE_MODULE} references resolve to no declaration: {names:?}. A seam \
                 contract is never computed over less than the provider uses"
            ),
            Self::TooFewItems { found, minimum } => write!(
                f,
                "the seam contract holds {found} items, fewer than the {minimum} floor; the \
                 extractor is measuring nothing"
            ),
            Self::Unreadable(why) => write!(f, "{why}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Lexical scanning
// ---------------------------------------------------------------------------

/// Byte spans of one top-level item. `body` is the index of the `{` that opens
/// its block, when it has one.
#[derive(Debug, Clone, Copy)]
struct RawItem {
    start: usize,
    end: usize,
    body: Option<usize>,
}

/// Walk `src` as Rust source, calling `visit(index, char, in_literal)` for every
/// character that is not inside a comment.
///
/// `in_literal` is true for every character of a string, raw string or character
/// literal, **including its delimiters**. Structural consumers ignore those;
/// text consumers keep them verbatim. This is the one place lexical trivia is
/// understood, so a string containing `//` or `{` cannot desynchronise item
/// splitting, normalisation or reference scanning.
fn walk_code(src: &str, mut visit: impl FnMut(usize, char, bool)) {
    let chars: Vec<(usize, char)> = src.char_indices().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let (idx, ch) = chars[i];
        let next = chars.get(i + 1).map(|(_, c)| *c);
        match ch {
            '/' if next == Some('/') => {
                while i < chars.len() && chars[i].1 != '\n' {
                    i += 1;
                }
            }
            '/' if next == Some('*') => {
                let mut depth = 1usize;
                i += 2;
                while i < chars.len() && depth > 0 {
                    let c = chars[i].1;
                    let n = chars.get(i + 1).map(|(_, c)| *c);
                    if c == '/' && n == Some('*') {
                        depth += 1;
                        i += 2;
                    } else if c == '*' && n == Some('/') {
                        depth -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
            }
            'r' | 'b' if raw_string_hashes(&chars, i).is_some() => {
                let hashes = raw_string_hashes(&chars, i).expect("checked");
                let lead = raw_string_lead(&chars, i);
                for step in 0..lead {
                    let (j, c) = chars[i + step];
                    visit(j, c, true);
                }
                i += lead;
                loop {
                    let Some((j, c)) = chars.get(i).copied() else {
                        break;
                    };
                    visit(j, c, true);
                    i += 1;
                    if c == '"'
                        && (1..=hashes).all(|h| chars.get(i + h - 1).map(|(_, c)| *c) == Some('#'))
                    {
                        for h in 0..hashes {
                            let (j, c) = chars[i + h];
                            visit(j, c, true);
                        }
                        i += hashes;
                        break;
                    }
                }
            }
            '"' => {
                visit(idx, ch, true);
                i += 1;
                while i < chars.len() {
                    let (j, c) = chars[i];
                    visit(j, c, true);
                    match c {
                        '\\' => {
                            if let Some((k, e)) = chars.get(i + 1).copied() {
                                visit(k, e, true);
                            }
                            i += 2;
                        }
                        '"' => {
                            i += 1;
                            break;
                        }
                        _ => i += 1,
                    }
                }
            }
            '\'' => {
                // A character literal, or a lifetime. `'a` is a lifetime; `'a'`
                // and `'\n'` are literals.
                let after = chars.get(i + 1).map(|(_, c)| *c);
                let after2 = chars.get(i + 2).map(|(_, c)| *c);
                let is_literal = match after {
                    Some('\\') => true,
                    Some(_) => after2 == Some('\''),
                    None => false,
                };
                if is_literal {
                    visit(idx, ch, true);
                    i += 1;
                    while i < chars.len() {
                        let (j, c) = chars[i];
                        visit(j, c, true);
                        match c {
                            '\\' => {
                                if let Some((k, e)) = chars.get(i + 1).copied() {
                                    visit(k, e, true);
                                }
                                i += 2;
                            }
                            '\'' => {
                                i += 1;
                                break;
                            }
                            _ => i += 1,
                        }
                    }
                } else {
                    visit(idx, ch, false);
                    i += 1;
                }
            }
            _ => {
                visit(idx, ch, false);
                i += 1;
            }
        }
    }
}

/// Characters of a raw-string opener (`r`, `br`, and their `#`s and the quote).
fn raw_string_lead(chars: &[(usize, char)], i: usize) -> usize {
    let mut j = i;
    if chars.get(j).map(|(_, c)| *c) == Some('b') {
        j += 1;
    }
    j += 1; // the `r`
    while chars.get(j).map(|(_, c)| *c) == Some('#') {
        j += 1;
    }
    j + 1 - i // include the opening quote
}

/// Number of `#` in a raw-string opener starting at `i`, if this really is one.
fn raw_string_hashes(chars: &[(usize, char)], i: usize) -> Option<usize> {
    let mut j = i;
    if chars.get(j).map(|(_, c)| *c) == Some('b') {
        j += 1;
    }
    if chars.get(j).map(|(_, c)| *c) != Some('r') {
        return None;
    }
    // The opener must not be the tail of a longer identifier.
    if i > 0 {
        let prev = chars[i - 1].1;
        if prev.is_alphanumeric() || prev == '_' {
            return None;
        }
    }
    j += 1;
    let mut hashes = 0usize;
    while chars.get(j).map(|(_, c)| *c) == Some('#') {
        hashes += 1;
        j += 1;
    }
    if chars.get(j).map(|(_, c)| *c) == Some('"') {
        Some(hashes)
    } else {
        None
    }
}

/// Strip comments and canonicalise whitespace, leaving code identical up to
/// formatting.
///
/// One rule beyond "collapse whitespace", aimed squarely at rustfmt: a comma
/// immediately before a closer is dropped, because wrapping a signature across
/// lines *adds* a trailing comma and unwrapping removes it. String and character
/// literals are exempt from both rules -- their bytes are the contract.
pub fn normalise(src: &str) -> String {
    let mut chars: Vec<(char, bool)> = Vec::with_capacity(src.len());
    walk_code(src, |_, ch, literal| {
        if literal {
            chars.push((ch, true));
        } else if ch.is_whitespace() {
            if !matches!(chars.last(), Some((' ', false)) | None) {
                chars.push((' ', false));
            }
        } else {
            chars.push((ch, false));
        }
    });

    // Pass A: drop a comma that only exists because the item was wrapped.
    let mut without_trailing_commas: Vec<(char, bool)> = Vec::with_capacity(chars.len());
    for (i, (ch, literal)) in chars.iter().enumerate() {
        if *ch == ',' && !*literal {
            let mut j = i + 1;
            while matches!(chars.get(j), Some((' ', false))) {
                j += 1;
            }
            if matches!(
                chars.get(j),
                Some((')', false)) | Some((']', false)) | Some(('}', false)) | Some(('>', false))
            ) {
                continue;
            }
        }
        without_trailing_commas.push((*ch, *literal));
    }

    // Pass B: a space next to a delimiter is wrapping, not meaning. `fn f(\n a:
    // u8,\n)` and `fn f(a: u8)` are the same declaration.
    let mut out = String::with_capacity(without_trailing_commas.len());
    let mut last_kept: Option<(char, bool)> = None;
    for (i, (ch, literal)) in without_trailing_commas.iter().enumerate() {
        if *ch == ' ' && !*literal {
            let opens = matches!(
                last_kept,
                Some(('(', false)) | Some(('[', false)) | Some(('<', false))
            );
            let closes = matches!(
                without_trailing_commas.get(i + 1),
                Some((')', false))
                    | Some((']', false))
                    | Some(('>', false))
                    | Some((',', false))
                    | Some((';', false))
            );
            if opens || closes {
                continue;
            }
        }
        out.push(*ch);
        last_kept = Some((*ch, *literal));
    }
    out.trim().to_owned()
}

/// Remove `#[...]` / `#![...]` groups from an already-normalised header.
fn strip_attributes(header: &str) -> String {
    let chars: Vec<char> = header.chars().collect();
    let mut out = String::with_capacity(header.len());
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '#'
            && matches!(chars.get(i + 1), Some('[') | Some('!'))
            && (chars.get(i + 1) == Some(&'[') || chars.get(i + 2) == Some(&'['))
        {
            let mut depth = 0i32;
            while i < chars.len() {
                match chars[i] {
                    '[' => depth += 1,
                    ']' => {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Split one scope's source into its top-level items.
fn split_items(src: &str) -> Vec<RawItem> {
    let mut items: Vec<RawItem> = Vec::new();
    let mut brace = 0i32;
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut start: Option<usize> = None;
    let mut body: Option<usize> = None;
    let mut closed_at: Option<usize> = None;

    walk_code(src, |idx, ch, literal| {
        if literal {
            return;
        }
        if closed_at.is_some() && ch.is_whitespace() {
            return;
        }
        if let Some(close) = closed_at.take() {
            // The item's block has closed. It ends there -- unless the next code
            // character is `;`, as in `const X: T = T { .. };`.
            let began = start.take().expect("an item was open");
            if ch == ';' {
                items.push(RawItem {
                    start: began,
                    end: idx + 1,
                    body,
                });
                body = None;
                return;
            }
            items.push(RawItem {
                start: began,
                end: close,
                body,
            });
            body = None;
            start = Some(idx);
        }
        if start.is_none() {
            if ch.is_whitespace() {
                return;
            }
            start = Some(idx);
        }
        match ch {
            '{' => {
                if brace == 0 && paren == 0 && bracket == 0 && body.is_none() {
                    body = Some(idx);
                }
                brace += 1;
            }
            '}' => {
                brace -= 1;
                if brace == 0 {
                    closed_at = Some(idx + 1);
                }
            }
            '(' => paren += 1,
            ')' => paren -= 1,
            '[' => bracket += 1,
            ']' => bracket -= 1,
            ';' if brace == 0 && paren == 0 && bracket == 0 => {
                items.push(RawItem {
                    start: start.take().expect("an item was open"),
                    end: idx + 1,
                    body,
                });
                body = None;
            }
            _ => {}
        }
    });
    if let (Some(s), Some(close)) = (start, closed_at) {
        items.push(RawItem {
            start: s,
            end: close,
            body,
        });
    }
    items
}

/// The kind and name of an item, read from its header.
fn classify(header: &str) -> Option<(&'static str, String)> {
    let cleaned = strip_attributes(&normalise(header));
    let mut words = cleaned.split_whitespace().peekable();
    while let Some(raw_word) = words.next() {
        let word = raw_word.trim_end_matches('!');
        let kind: &'static str = match word {
            "pub" | "unsafe" | "async" | "extern" | "default" | "auto" | "move" => continue,
            w if w.starts_with("pub(") => continue,
            "const" => {
                if words.peek().copied() == Some("fn") {
                    continue;
                }
                "const"
            }
            "static" => "static",
            "fn" => "fn",
            "struct" => "struct",
            "enum" => "enum",
            "union" => "union",
            "trait" => "trait",
            "type" => "type",
            "mod" => "mod",
            "impl" => return Some(("impl", impl_target(&cleaned)?)),
            "use" => return Some(("use", String::new())),
            "macro_rules" => "macro",
            _ => continue,
        };
        let mut name = words.next()?.to_owned();
        if kind == "static" && name == "mut" {
            name = words.next()?.to_owned();
        }
        let name: String = name
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        return if name.is_empty() {
            None
        } else {
            Some((kind, name))
        };
    }
    None
}

/// The type an `impl` block is *for*: the text after a top-level `for` when
/// there is one, otherwise the type after the `impl` generics.
fn impl_target(header: &str) -> Option<String> {
    let after_impl = header.split_once("impl")?.1;
    let after_generics = skip_generics(after_impl.trim_start());
    let target = split_top_level_for(after_generics).unwrap_or(after_generics);
    let ident: String = target
        .trim_start()
        .trim_start_matches('&')
        .trim_start()
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if ident.is_empty() {
        None
    } else {
        Some(ident)
    }
}

fn skip_generics(text: &str) -> &str {
    if !text.starts_with('<') {
        return text;
    }
    let mut depth = 0i32;
    for (idx, ch) in text.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return &text[idx + 1..];
                }
            }
            _ => {}
        }
    }
    text
}

/// The text after a ` for ` that is not nested inside generics.
fn split_top_level_for(text: &str) -> Option<&str> {
    let chars: Vec<char> = text.chars().collect();
    let mut depth = 0i32;
    let mut byte = 0usize;
    for idx in 0..chars.len() {
        match chars[idx] {
            '<' => depth += 1,
            '>' => depth -= 1,
            'f' if depth == 0
                && chars.get(idx + 1) == Some(&'o')
                && chars.get(idx + 2) == Some(&'r')
                && (idx == 0 || chars[idx - 1].is_whitespace())
                && chars.get(idx + 3).is_some_and(|c| c.is_whitespace()) =>
            {
                return Some(&text[byte + 3..]);
            }
            _ => {}
        }
        byte += chars[idx].len_utf8();
    }
    None
}

/// Render one item as a **declaration**.
///
/// Bodies are dropped for everything that has one, because a body is where a
/// renamed local lives and requirement (1) says a renamed local must not
/// invalidate a receipt. Field lists, variant lists and constant values are
/// kept, because those are what a caller can observe.
fn declaration_of(src: &str, item: RawItem, kind: &'static str) -> String {
    let text = &src[item.start..item.end];
    let header = || item.body.map(|b| &src[item.start..b]).unwrap_or(text);
    match kind {
        "struct" | "enum" | "union" | "type" | "const" | "static" | "macro" => normalise(text),
        "mod" => format!("{} {{ ... }}", normalise(header())),
        "fn" => format!("{};", normalise(header()).trim_end_matches(';')),
        "trait" | "impl" => {
            let Some(body_start) = item.body else {
                return normalise(text);
            };
            let head = normalise(&src[item.start..body_start]);
            let inner_src = &src[body_start + 1..item.end.saturating_sub(1)];
            let mut inner = Vec::new();
            for raw in split_items(inner_src) {
                let inner_header = raw
                    .body
                    .map(|b| &inner_src[raw.start..b])
                    .unwrap_or(&inner_src[raw.start..raw.end]);
                let Some((inner_kind, _)) = classify(inner_header) else {
                    continue;
                };
                if inner_kind == "use" {
                    continue;
                }
                inner.push(declaration_of(inner_src, raw, inner_kind));
            }
            inner.sort();
            format!("{head} {{ {} }}", inner.join(" "))
        }
        _ => normalise(text),
    }
}

// ---------------------------------------------------------------------------
// Reference scanning
// ---------------------------------------------------------------------------

/// Every `native_a11y::...` path an adapter refers to, as 1- or 2-segment paths.
///
/// `use` trees (`use crate::native_a11y::{a, m::{b, C}};`) and bare paths
/// (`crate::native_a11y::win32::Host::desktop()`) are the same grammar and go
/// through the same parser. A glob is reported as the literal `*`, which
/// resolves to nothing and therefore refuses.
pub fn referenced_substrate_paths(adapter_src: &str) -> Vec<Vec<String>> {
    // Comments and string literals are not references.
    let mut code = String::with_capacity(adapter_src.len());
    walk_code(adapter_src, |_, ch, literal| {
        code.push(if literal { ' ' } else { ch });
    });

    let needle: Vec<char> = format!("{SUBSTRATE_MODULE}::").chars().collect();
    let chars: Vec<char> = code.chars().collect();
    let mut found: BTreeSet<Vec<String>> = BTreeSet::new();
    let mut from = 0usize;
    while let Some(hit) = find_from(&chars, &needle, from) {
        from = hit + needle.len();
        if hit > 0 {
            let prev = chars[hit - 1];
            if prev.is_alphanumeric() || prev == '_' {
                continue;
            }
        }
        let mut cursor = from;
        parse_tree(&chars, &mut cursor, &mut Vec::new(), &mut found);
    }
    found.into_iter().collect()
}

fn find_from(haystack: &[char], needle: &[char], from: usize) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() || from > haystack.len() - needle.len() {
        return None;
    }
    (from..=haystack.len() - needle.len()).find(|&i| haystack[i..i + needle.len()] == needle[..])
}

fn parse_tree(
    chars: &[char],
    cursor: &mut usize,
    prefix: &mut Vec<String>,
    out: &mut BTreeSet<Vec<String>>,
) {
    skip_ws(chars, cursor);
    match chars.get(*cursor).copied() {
        Some('{') => {
            *cursor += 1;
            loop {
                skip_ws(chars, cursor);
                match chars.get(*cursor).copied() {
                    None => return,
                    Some('}') => {
                        *cursor += 1;
                        return;
                    }
                    Some(',') => {
                        *cursor += 1;
                        continue;
                    }
                    _ => {}
                }
                let mut branch = prefix.clone();
                let before = *cursor;
                parse_tree(chars, cursor, &mut branch, out);
                if *cursor == before {
                    return; // no progress: something unparsed, stop rather than spin
                }
                skip_ws(chars, cursor);
                match chars.get(*cursor).copied() {
                    Some(',') => *cursor += 1,
                    Some('}') => {
                        *cursor += 1;
                        return;
                    }
                    _ => return,
                }
            }
        }
        Some('*') => {
            *cursor += 1;
            out.insert(vec!["*".to_owned()]);
        }
        Some(c) if c.is_alphabetic() || c == '_' => {
            let mut ident = String::new();
            while let Some(c) = chars.get(*cursor).copied() {
                if c.is_alphanumeric() || c == '_' {
                    ident.push(c);
                    *cursor += 1;
                } else {
                    break;
                }
            }
            prefix.push(ident);
            let mark = *cursor;
            skip_ws(chars, cursor);
            if chars.get(*cursor) == Some(&':') && chars.get(*cursor + 1) == Some(&':') {
                *cursor += 2;
                if prefix.len() >= 2 {
                    // Two segments is the depth the substrate has; anything
                    // deeper is an associated item, covered by the type's impls.
                    out.insert(prefix.clone());
                    return;
                }
                parse_tree(chars, cursor, prefix, out);
                return;
            }
            *cursor = mark;
            out.insert(prefix.clone());
        }
        _ => {}
    }
}

fn skip_ws(chars: &[char], cursor: &mut usize) {
    while chars.get(*cursor).is_some_and(|c| c.is_whitespace()) {
        *cursor += 1;
    }
}

// ---------------------------------------------------------------------------
// The contract
// ---------------------------------------------------------------------------

/// Compute the seam contract for `adapter_src` against `substrate_src`.
///
/// Pure over two strings, so every property is testable without touching the
/// filesystem and every mutant can be driven from a literal.
pub fn seam_contract_from_sources(
    adapter_src: &str,
    substrate_src: &str,
) -> Result<SeamContract, SeamContractError> {
    seam_contract_with_floor(adapter_src, substrate_src, SEAM_CONTRACT_MIN_ITEMS)
}

/// As [`seam_contract_from_sources`], with the item floor supplied. Only the
/// module's own fixtures lower it; production always uses
/// [`SEAM_CONTRACT_MIN_ITEMS`].
pub fn seam_contract_with_floor(
    adapter_src: &str,
    substrate_src: &str,
    floor: usize,
) -> Result<SeamContract, SeamContractError> {
    let paths = referenced_substrate_paths(adapter_src);
    if paths.is_empty() {
        return Err(SeamContractError::NoReferences);
    }

    let top = index_scope(substrate_src);
    let mut items: BTreeSet<SeamItem> = BTreeSet::new();
    let mut unresolved: Vec<String> = Vec::new();

    for path in &paths {
        let joined = path.join("::");
        if path.len() == 1 {
            if !collect_named(substrate_src, &top, &path[0], "", &mut items) {
                unresolved.push(joined);
            }
            continue;
        }
        let (module, name) = (&path[0], &path[1]);
        match top
            .iter()
            .find(|(kind, ident, _)| *kind == "mod" && ident == module)
        {
            Some((kind, ident, raw)) => {
                items.insert(SeamItem {
                    path: ident.clone(),
                    kind,
                    declaration: declaration_of(substrate_src, *raw, kind),
                });
                let Some(body) = raw.body else {
                    unresolved.push(joined);
                    continue;
                };
                let inner_src = &substrate_src[body + 1..raw.end.saturating_sub(1)];
                let inner = index_scope(inner_src);
                let mut inner_items: BTreeSet<SeamItem> = BTreeSet::new();
                if collect_named(
                    inner_src,
                    &inner,
                    name,
                    &format!("{module}::"),
                    &mut inner_items,
                ) {
                    items.extend(inner_items);
                } else {
                    unresolved.push(joined);
                }
            }
            None => {
                // Not a module: `Type::ASSOCIATED`. The type itself, with its
                // impls, covers the associated item.
                if !collect_named(substrate_src, &top, module, "", &mut items) {
                    unresolved.push(joined);
                }
            }
        }
    }

    if !unresolved.is_empty() {
        unresolved.sort();
        unresolved.dedup();
        return Err(SeamContractError::Unresolved(unresolved));
    }

    let items: Vec<SeamItem> = items.into_iter().collect();
    if items.len() < floor {
        return Err(SeamContractError::TooFewItems {
            found: items.len(),
            minimum: floor,
        });
    }

    let mut rendered = String::from(SEAM_CONTRACT_RENDERING);
    for item in &items {
        rendered.push('\n');
        rendered.push_str(item.kind);
        rendered.push(' ');
        rendered.push_str(&item.path);
        rendered.push('\n');
        rendered.push_str(&item.declaration);
    }
    rendered.push('\n');
    let sha256 = sha256_hex(rendered.as_bytes());
    Ok(SeamContract {
        items,
        rendered,
        sha256,
    })
}

type ScopeIndex = Vec<(&'static str, String, RawItem)>;

fn index_scope(src: &str) -> ScopeIndex {
    split_items(src)
        .into_iter()
        .filter_map(|raw| {
            let header = raw
                .body
                .map(|b| &src[raw.start..b])
                .unwrap_or(&src[raw.start..raw.end]);
            let (kind, name) = classify(header)?;
            if kind == "use" {
                return None;
            }
            Some((kind, name, raw))
        })
        .collect()
}

/// Collect the declaration named `name`, plus every `impl` block targeting it.
/// Returns false when nothing of that name exists -- which is a refusal, never a
/// silent skip.
fn collect_named(
    src: &str,
    scope: &ScopeIndex,
    name: &str,
    prefix: &str,
    out: &mut BTreeSet<SeamItem>,
) -> bool {
    let mut hit = false;
    for (kind, ident, raw) in scope {
        if ident != name {
            continue;
        }
        hit = true;
        let path = if *kind == "impl" {
            format!("{prefix}impl {ident}")
        } else {
            format!("{prefix}{ident}")
        };
        out.insert(SeamItem {
            path,
            kind,
            declaration: declaration_of(src, *raw, kind),
        });
    }
    hit
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUBSTRATE: &str = r##"
/// A doc comment that must not be part of the contract.
pub const LADDER: &[u64] = &[150, 300];

pub const CLASS: &str = "Qt51519QWindowIcon";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Editable {
    pub name: String,
    pub writable: bool,
}

impl Editable {
    /// Inherent methods are part of the seam.
    pub fn is_composer(&self) -> bool {
        let mine = self.writable;
        mine
    }
}

pub enum Route {
    Window,
    Renderer,
}

pub trait Syscalls {
    fn set_value(&self, element: &Editable, value: &str) -> Result<bool, Timeout>;
    fn value_of(&self, element: &Editable) -> Result<Option<String>, Timeout>;
}

pub struct Timeout {
    pub millis: u64,
}

pub fn place(host: &dyn Syscalls, element: &Editable, carrier: &str) -> bool {
    // A brace in a string must not desynchronise the splitter: "{"
    let local = carrier.len();
    host.set_value(element, carrier).is_ok() && local > 0
}

pub(crate) mod win32 {
    pub struct Host;
    impl Host {
        pub fn desktop() -> Self {
            Self
        }
    }
}
"##;

    const ADAPTER: &str = r#"
use crate::native_a11y::{place, Editable, Route, Syscalls, Timeout, CLASS, LADDER};

pub fn drive() {
    let host = crate::native_a11y::win32::Host::desktop();
    let _ = place(&host, &Editable { name: String::new(), writable: true }, "x");
}
"#;

    fn contract(substrate: &str) -> SeamContract {
        seam_contract_with_floor(ADAPTER, substrate, 1).expect("the fixture contract computes")
    }

    #[test]
    fn the_contract_covers_everything_the_adapter_names() {
        let c = contract(SUBSTRATE);
        let paths: Vec<&str> = c.items.iter().map(|i| i.path.as_str()).collect();
        for expected in [
            "LADDER",
            "CLASS",
            "Editable",
            "impl Editable",
            "Route",
            "Syscalls",
            "Timeout",
            "place",
            "win32",
            "win32::Host",
            "win32::impl Host",
        ] {
            assert!(
                paths.contains(&expected),
                "{expected} missing from {paths:?}"
            );
        }
        let syscalls = c
            .items
            .iter()
            .find(|i| i.path == "Syscalls")
            .expect("the trait is in the contract");
        assert!(
            syscalls.declaration.contains("fn set_value")
                && syscalls.declaration.contains("fn value_of"),
            "{}",
            syscalls.declaration
        );
        let place = c
            .items
            .iter()
            .find(|i| i.path == "place")
            .expect("the free function is in the contract");
        assert!(
            !place.declaration.contains("let local"),
            "a function body reached the contract: {}",
            place.declaration
        );
    }

    #[test]
    fn a_body_only_change_does_not_move_the_hash() {
        let before = contract(SUBSTRATE);
        // Rename a local, reflow a signature, add a comment. Nothing a caller
        // can observe changes.
        let after_src = SUBSTRATE
            .replace(
                "let mine = self.writable;\n        mine",
                "let flag = self.writable;\n        flag",
            )
            .replace(
                "let local = carrier.len();",
                "// an added comment\n    let length = carrier.len();",
            )
            .replace("local > 0", "length > 0")
            .replace(
                "pub fn place(host: &dyn Syscalls, element: &Editable, carrier: &str) -> bool {",
                "pub fn place(\n    host: &dyn Syscalls,\n    element: &Editable,\n    carrier: &str,\n) -> bool {",
            );
        assert_ne!(
            after_src, SUBSTRATE,
            "the refactor mutant must actually edit the source"
        );
        let after = contract(&after_src);
        assert_eq!(
            before.sha256, after.sha256,
            "a behaviour-preserving refactor moved the seam contract:\n{}\n---\n{}",
            before.rendered, after.rendered
        );
    }

    #[test]
    fn a_seam_change_moves_the_hash() {
        let before = contract(SUBSTRATE);
        for (label, mutated) in [
            (
                "parameter renamed",
                SUBSTRATE.replace(
                    "fn set_value(&self, element: &Editable, value: &str)",
                    "fn set_value(&self, element: &Editable, carrier: &str)",
                ),
            ),
            (
                "return type changed",
                SUBSTRATE.replace(
                    "fn value_of(&self, element: &Editable) -> Result<Option<String>, Timeout>",
                    "fn value_of(&self, element: &Editable) -> Result<Option<Vec<u8>>, Timeout>",
                ),
            ),
            (
                "enum variant added",
                SUBSTRATE.replace(
                    "    Window,\n    Renderer,",
                    "    Window,\n    Renderer,\n    Overlay,",
                ),
            ),
            (
                "struct field removed",
                SUBSTRATE.replace("    pub writable: bool,\n", ""),
            ),
            (
                "measured constant changed",
                SUBSTRATE.replace("&[150, 300]", "&[150, 301]"),
            ),
            (
                "measured string changed",
                SUBSTRATE.replace("Qt51519QWindowIcon", "Qt51520QWindowIcon"),
            ),
            (
                "inherent method signature changed",
                SUBSTRATE.replace(
                    "pub fn is_composer(&self) -> bool",
                    "pub fn is_composer(&self) -> Option<bool>",
                ),
            ),
        ] {
            assert_ne!(
                mutated, SUBSTRATE,
                "{label}: the mutant must edit the source"
            );
            let after = contract(&mutated);
            assert_ne!(
                before.sha256, after.sha256,
                "{label} did not move the seam contract hash"
            );
        }
    }

    #[test]
    fn an_unresolvable_reference_refuses_rather_than_under_covering() {
        match seam_contract_with_floor("use crate::native_a11y::{place, NoSuchItem};", SUBSTRATE, 1)
        {
            Err(SeamContractError::Unresolved(names)) => {
                assert!(names.iter().any(|n| n == "NoSuchItem"), "{names:?}")
            }
            other => panic!("an unresolvable reference was tolerated: {other:?}"),
        }
        match seam_contract_with_floor("use crate::native_a11y::*;", SUBSTRATE, 1) {
            Err(SeamContractError::Unresolved(names)) => {
                assert!(names.iter().any(|n| n == "*"), "{names:?}")
            }
            other => panic!("a glob import was tolerated: {other:?}"),
        }
    }

    #[test]
    fn an_adapter_that_names_nothing_refuses() {
        assert_eq!(
            seam_contract_with_floor("fn main() {}", SUBSTRATE, 1),
            Err(SeamContractError::NoReferences)
        );
    }

    #[test]
    fn the_substrate_losing_an_item_refuses_instead_of_hashing_less() {
        let renamed = SUBSTRATE.replace("pub trait Syscalls {", "pub trait Renamed {");
        match seam_contract_with_floor(ADAPTER, &renamed, 1) {
            Err(SeamContractError::Unresolved(names)) => {
                assert!(names.iter().any(|n| n == "Syscalls"), "{names:?}")
            }
            other => panic!("the contract was computed without the trait it binds: {other:?}"),
        }
    }

    #[test]
    fn a_collapsed_contract_refuses_at_the_floor() {
        match seam_contract_from_sources(ADAPTER, SUBSTRATE) {
            Err(SeamContractError::TooFewItems { minimum, .. }) => {
                assert_eq!(minimum, SEAM_CONTRACT_MIN_ITEMS)
            }
            other => panic!("the production floor did not apply: {other:?}"),
        }
    }

    #[test]
    fn strings_and_comments_cannot_desynchronise_the_splitter() {
        let scope = index_scope(SUBSTRATE);
        let names: Vec<&str> = scope.iter().map(|(_, n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "LADDER", "CLASS", "Editable", "Editable", "Route", "Syscalls", "Timeout", "place",
                "win32"
            ],
            "item splitting drifted"
        );
    }

    #[test]
    fn normalisation_is_insensitive_to_wrapping_but_not_to_tokens() {
        assert_eq!(
            normalise("fn f(\n    a: u8,\n    b: u8,\n) -> u8"),
            normalise("fn f(a: u8, b: u8) -> u8")
        );
        assert_ne!(normalise("fn f(a: u8)"), normalise("fn f(a: u16)"));
        assert_eq!(normalise("let x = 1; // note"), normalise("let x = 1;"));
        assert_eq!(normalise("let x = 1; /* note */"), normalise("let x = 1;"));
        // A literal's own bytes survive both rules.
        assert_ne!(
            normalise(r#"const A: &str = "a  b";"#),
            normalise(r#"const A: &str = "a b";"#)
        );
        assert_ne!(
            normalise(r#"const A: &str = "x,)";"#),
            normalise(r#"const A: &str = "x)";"#)
        );
    }
}
