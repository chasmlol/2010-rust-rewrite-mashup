//! `make publish-check` — what a push would publish, read before it happens.
//!
//! This is **not** a proof of provenance or of licence cleanliness. It greps
//! for a fixed set of shapes; passing it means those shapes are absent and
//! nothing more. Where the code came from is answered by `README.md` and
//! `NOTICE`, which say plainly that IW4L is built with reverse engineering and
//! name the projects that were read.
//!
//! Two things it does answer, both about accidents rather than about origin:
//!
//! 1. **Leaks.** The working memory under `context/`, a machine-local `.env`, a
//!    private key, a piece of an original game install — none of it is the
//!    product, and a stray `git add -f` is all it takes. This is the half worth
//!    running before every push.
//! 2. **Retail offsets and pinned source.** A standalone runtime resolves
//!    nothing against a retail image, so a hardcoded address from one is dead
//!    weight wherever it appears, a decompiler placeholder name (`FUN_…`,
//!    `DAT_…`) is a half-finished symbol, and a script file pinned by hash and
//!    line range is an index into somebody else's tree. Naming IW4x, KisakCOD,
//!    an original executable or Ghidra is *not* a finding: crediting what was
//!    read is the policy, not a leak.
//!
//! It walks the tracked tree, so it sees exactly what a push would publish —
//! and only that. It cannot see history, and it cannot see a release archive;
//! those are checked separately.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::shell::Res;

/// The IW4 image range. A literal inside it is an address.
const IMAGE_BASE: u64 = 0x0040_0000;
const IMAGE_END: u64 = 0x00a0_0000;

/// Past the code section the module keeps its globals. Sizes and capacities
/// live in that range too, but they are block-aligned; an address almost never
/// is, so only an unaligned literal counts here.
const DATA_END: u64 = 0x0800_0000;
const BLOCK_ALIGN: u64 = 0xfff;

/// Exempt from the scans that read a file's *contents* — offsets, and a pasted
/// key — because a file can legitimately spell what is hunted for. The path
/// rules in `leak` take no exemption at all. Entries earn it one at a time.
const EXEMPT: &[&str] = &[
    // Cargo writes it, and writes build metadata like `0.6.0+11769913` into it.
    "Cargo.lock",
    // This gate has to spell the shapes it hunts for.
    "xtask/src/publish_check.rs",
];

fn stray_test(rel: &str, text: &str) -> Option<(usize, &'static str)> {
    if rel.split('/').rev().skip(1).any(|part| part == "tests") {
        return Some((0, "tests/ directory"));
    }
    if !rel.ends_with(".rs") {
        return None;
    }
    text.lines().enumerate().find_map(|(n, line)| {
        let line = line.trim_start();
        let hit = line.starts_with("#[test]")
            || line.starts_with("#[cfg(test)]")
            || line.starts_with("#[tokio::test")
            || line
                .strip_prefix("mod tests")
                .is_some_and(|rest| rest.starts_with([' ', '{', ';']) || rest.is_empty());
        hit.then_some((n + 1, "cargo test"))
    })
}

/// What must never be in a commit, whatever it is named. These are accidents,
/// not judgement calls, so there is no exemption list beside them.
///
/// `context/` is the working memory: artifacts, third-party clones, agent
/// clones, the research base. `.gitignore` holds `/context/` and the `make mr`
/// pre-commit hook refuses a staged path under it — this is the third fence,
/// the one that survives a fresh clone with no hook installed.
fn leak(rel: &str) -> Option<&'static str> {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    let ext = name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase());
    let ext = ext.as_deref().unwrap_or("");
    if rel == "context" || rel.starts_with("context/") {
        return Some("working memory under context/");
    }
    // `.env` is machine-local: games root, deploy host, the release CA path.
    // `.env.example` is the template and is meant to be here.
    if name == ".env" || (name.starts_with(".env.") && name != ".env.example") {
        return Some("machine-local .env");
    }
    // A private key, or the cert material that sits next to one. The public CA
    // pem a player trusts is generated at release time, never committed.
    if matches!(
        ext,
        "pem" | "key" | "p12" | "pfx" | "jks" | "keystore" | "ppk"
    ) || matches!(name, "id_rsa" | "id_ecdsa" | "id_ed25519")
    {
        return Some("key or certificate material");
    }
    // A piece of somebody's game install. IW4L reads these; it never ships one.
    if matches!(
        ext,
        "ff" | "iwd" | "iwi" | "d3dbsp" | "bik" | "iwuv" | "pak" | "gsc" | "csc"
    ) {
        return Some("original game data");
    }
    // A retail image, or anything else prebuilt. Nothing here is distributed as
    // a binary; the release archive is built from source by `make launcher`.
    if matches!(ext, "exe" | "dll" | "so" | "dylib" | "pdb" | "msi") {
        return Some("executable or library image");
    }
    None
}

/// The text form of a private key, wherever it is pasted. Extension rules miss
/// a key inside a `.md` walkthrough or a `.json` fixture; this does not.
const KEY_BLOCK: &str = "-----BEGIN";
const KEY_WORDS: &[&str] = &["PRIVATE KEY-----", "OPENSSH PRIVATE KEY"];

fn pasted_key(text: &str) -> bool {
    text.match_indices(KEY_BLOCK).any(|(at, _)| {
        let line_end = text[at..].find('\n').map_or(text.len(), |end| at + end);
        KEY_WORDS
            .iter()
            .any(|word| text[at..line_end].contains(word))
    })
}

/// A bit pattern can land in the module's range by coincidence: `0x0040_2000`
/// is a `pm_flags` bit, `0x00a5_5a5a` an LCG seed, `0x0405_0607` a byte
/// shuffle. A literal is read as one when the text right before it names it —
/// `PRONE_TRACE_MASK: u32 =`, `lcg:`, `MASK_SHOT is`. Naming it *after* the
/// literal does not count, so a citation cannot hide behind a later word.
const NAMES_A_PATTERN: &[&str] = &[
    "MASK", "FLAG", "BITS", "STAND", "CONTENTS", "SEED", "LCG", "MAGIC", "PATTERN", "PRIME",
    "SHUFFLE",
];

/// How far back to read for that name, and where to stop early: a `;` or a `)`
/// closes the expression before it, so a name on its far side is not this
/// literal's. Without that cut `f32::from_bits(...) + read(0x474d40)` clears
/// its second literal for free — `from_bits` spells `BITS`.
const NAME_WINDOW: usize = 64;
const NAME_STOPS: &[char] = &[';', ')'];

/// A markdown table separates its cells with `|`. Those pipes are not operators
/// — without this the gate reads every row of every table as a mask line and
/// clears it whole.
fn table_row(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('|') && trimmed.matches('|').count() >= 2
}

/// The other half of the pattern test: the literal *combined* bitwise, as in
/// `flags & 0x0040_2000` or `bits |= 0x0040_2000`. Only an operator next to the
/// literal counts: an operator anywhere on the line would clear the line whole,
/// which a markdown pipe or an unrelated neighbour could then do for free.
fn bitwise_neighbour(line: &str, at: usize, end: usize) -> bool {
    let ops: &[char] = if table_row(line) {
        &['&', '^']
    } else {
        &['&', '|', '^']
    };
    let before = line[..at].trim_end();
    // `x &= 0x…` reads the same as `x & 0x…`.
    let before = before.strip_suffix('=').unwrap_or(before);
    let lhs = before.ends_with(ops) && !before.ends_with("&&") && !before.ends_with("||");
    let after = line[end.min(line.len())..].trim_start();
    let rhs = after.starts_with(ops) && !after.starts_with("&&") && !after.starts_with("||");
    lhs || rhs
}

fn reads_as_pattern(line: &str, at: usize, end: usize) -> bool {
    if bitwise_neighbour(line, at, end) {
        return true;
    }
    let head = &line[..at.min(line.len())];
    let head = match head.rfind(NAME_STOPS) {
        Some(stop) => &head[stop + 1..],
        None => head,
    };
    let start = head
        .char_indices()
        .rev()
        .take(NAME_WINDOW)
        .last()
        .map_or(0, |(i, _)| i);
    let upper = head[start..].to_ascii_uppercase();
    NAMES_A_PATTERN.iter().any(|word| upper.contains(word))
}

/// `f32::from_bits(0x0040_1000)` is a float written as its bits. Only the
/// literal the call takes is exempt — the rest of the line is still scanned.
fn is_from_bits_arg(line: &str, at: usize) -> bool {
    line[..at].trim_end().ends_with("from_bits(")
}

struct Finding {
    path: PathBuf,
    line: usize,
    what: String,
    text: String,
}

fn tracked(root: &Path) -> Res<Vec<PathBuf>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z"])
        .output()
        .map_err(|error| format!("git ls-files: {error}"))?;
    if !out.status.success() {
        return Err("git ls-files failed".to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .collect())
}

fn is_hex(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

fn word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// A literal can carry a Rust integer suffix; `0x0047_4d40u32` is the same
/// address as `0x0047_4d40`.
const INT_SUFFIX: &[&str] = &[
    "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize",
];

/// Reads the word that follows the digits: nothing, or an integer suffix, and
/// the literal ends there. Anything else means the digits are part of a name.
fn ends_literal(bytes: &[u8], k: usize) -> Option<usize> {
    let mut end = k;
    while end < bytes.len() && word_char(bytes[end]) {
        end += 1;
    }
    if end == k {
        return Some(k);
    }
    let word = std::str::from_utf8(&bytes[k..end]).ok()?;
    INT_SUFFIX.contains(&word).then_some(end)
}

/// `0xffffff`, `0xff00ff`, `0x777777`, `0xff_efff` — built out of one digit, or
/// out of `0` and `f` with at most one nibble punched out of the run. A mask is
/// typed that way; an address is not.
fn mask_shaped(digits: &str) -> bool {
    let odd = digits.bytes().filter(|b| *b != b'0' && *b != b'f').count();
    odd <= 1 || digits.bytes().all(|b| b == digits.as_bytes()[0])
}

/// A hex literal in the module's range, not preceded by a word character or a
/// dot (so `0x1.0p0` stays put).
///
/// `0x0047_4d40` is the canonical form — eight digits behind a leading zero —
/// and is taken at face value. The same address is just as often typed short,
/// `0x474d40` or `0x4f_ade0`, and six or seven digits are ambiguous with an
/// ordinary constant, so the short form has to *look* like an address: not
/// block-aligned, and not built from the digits a mask is.
fn address_at(bytes: &[u8], i: usize) -> Option<usize> {
    if i > 0 && (word_char(bytes[i - 1]) || bytes[i - 1] == b'.') {
        return None;
    }
    if !(bytes[i..].starts_with(b"0x") || bytes[i..].starts_with(b"0X")) {
        return None;
    }
    let mut digits = String::new();
    let mut k = i + 2;
    while k < bytes.len() && (is_hex(bytes[k]) || bytes[k] == b'_') {
        if bytes[k] != b'_' {
            digits.push(bytes[k] as char);
        }
        k += 1;
    }
    let end = ends_literal(bytes, k)?;
    if !(6..=8).contains(&digits.len()) {
        return None;
    }
    let value = u64::from_str_radix(&digits, 16).ok()?;
    let code = (IMAGE_BASE..IMAGE_END).contains(&value);
    let data = (IMAGE_END..DATA_END).contains(&value) && value & BLOCK_ALIGN != 0;
    if !(code || data) {
        return None;
    }
    let canonical = digits.len() == 8 && digits.starts_with('0');
    if !canonical && (value & BLOCK_ALIGN == 0 || mask_shaped(&digits)) {
        return None;
    }
    Some(end)
}

/// The same address typed in decimal — `4738368` is `0x484bc0`. Seven and eight
/// digits only: fewer cannot reach the image base. Sizes and counts land in
/// that range too, but like the globals above they are block-aligned.
fn decimal_at(bytes: &[u8], i: usize) -> Option<usize> {
    if i > 0 && (word_char(bytes[i - 1]) || bytes[i - 1] == b'.') {
        return None;
    }
    if !bytes[i].is_ascii_digit() {
        return None;
    }
    let mut digits = String::new();
    let mut k = i;
    while k < bytes.len() && (bytes[k].is_ascii_digit() || bytes[k] == b'_') {
        if bytes[k] != b'_' {
            digits.push(bytes[k] as char);
        }
        k += 1;
    }
    // A decimal point makes it a float, not an address.
    if bytes.get(k) == Some(&b'.') {
        return None;
    }
    let end = ends_literal(bytes, k)?;
    if !(7..=8).contains(&digits.len()) {
        return None;
    }
    let value: u64 = digits.parse().ok()?;
    ((IMAGE_BASE..DATA_END).contains(&value) && value & BLOCK_ALIGN != 0).then_some(end)
}

/// An address can also ride inside a name — `fun_00440c70_consume_buckets`,
/// `SCENE_MODEL_OFF_UNREAD_0063BF30`. Any `_`-separated run of 6-8 hex digits
/// that lands in the image range is one, whatever follows it.
fn address_in_name(lower: &str) -> bool {
    lower.split(|c: char| !word_char(c as u8)).any(|word| {
        word.split('_').filter(|part| !part.is_empty()).any(|part| {
            (6..=8).contains(&part.len())
                && part.bytes().all(is_hex)
                && part.bytes().any(|b| b.is_ascii_alphabetic())
                && u64::from_str_radix(part, 16)
                    .is_ok_and(|value| (IMAGE_BASE..IMAGE_END).contains(&value))
        })
    })
}

/// A script file pinned by content hash and line range. Nothing here reads one,
/// and a `ScriptGap` id, which names a hole in our own runtime, is not this
/// shape.
fn gsc_citation(lower: &str) -> bool {
    let Some(at) = lower.find(".gsc@sha256:") else {
        return lower.find(".gsc#l").is_some_and(|at| {
            lower[at + ".gsc#l".len()..]
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_digit)
        });
    };
    lower[at + ".gsc@sha256:".len()..]
        .bytes()
        .take_while(|b| is_hex(*b))
        .count()
        >= 16
}

fn scan_line(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    for i in 0..bytes.len() {
        for tag in [
            b"dat_".as_slice(),
            b"fun_",
            b"lab_",
            b"ptr_",
            b"sub_",
            b"loc_",
            b"unk_",
        ] {
            if bytes[i..].starts_with(tag) {
                let rest = &bytes[i + tag.len()..];
                let n = rest.iter().take_while(|b| is_hex(**b)).count();
                // A suffix after the digits does not redeem the name:
                // `fun_00436990_sift` is still a decompiler symbol.
                if (6..=8).contains(&n) && !rest.get(n).is_some_and(|b| is_hex(*b)) {
                    let tag = std::str::from_utf8(tag).expect("ascii tag");
                    return Some(format!("decompiler symbol {tag}"));
                }
            }
        }
        if let Some(end) = address_at(bytes, i)
            && !reads_as_pattern(line, i, end)
            && !is_from_bits_arg(line, i)
        {
            return Some("retail address".to_string());
        }
        if let Some(end) = decimal_at(bytes, i)
            && !reads_as_pattern(line, i, end)
            && !is_from_bits_arg(line, i)
        {
            return Some("retail address in decimal".to_string());
        }
    }
    if address_in_name(&lower) {
        return Some("address inside an identifier".to_string());
    }
    if gsc_citation(&lower) {
        return Some("script citation (path, hash and line range)".to_string());
    }
    None
}

/// Git's own rule: a NUL in the head of the file means binary. Such a file has
/// no lines to scan — the offset scan skips it, and what a binary could be
/// hiding is caught by `leak` on its path instead.
fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8000).any(|b| *b == 0)
}

pub fn run_cli(root: &Path) -> Res<()> {
    let mut leaks = Vec::new();
    let mut offsets = Vec::new();
    let mut tests = Vec::new();
    let mut scanned = 0usize;
    for path in tracked(root)? {
        let rel = path.to_string_lossy().replace('\\', "/");
        // The leak scan runs on every tracked path, exemptions included: a file
        // is exempt from being read for offsets, never from being here at all.
        if let Some(what) = leak(&rel) {
            leaks.push(Finding {
                path: path.clone(),
                line: 0,
                what: what.to_string(),
                text: "(path)".to_string(),
            });
            continue;
        }
        // Anything unreadable is a hole, not a pass: say so instead of skipping.
        let bytes = std::fs::read(root.join(&path))
            .map_err(|error| format!("{}: unreadable, so unscanned: {error}", path.display()))?;
        if looks_binary(&bytes) {
            continue;
        }
        // Not every text file is valid UTF-8; what is in one still counts.
        let text = String::from_utf8_lossy(&bytes);
        if let Some((line, what)) = stray_test(&rel, &text) {
            tests.push(Finding {
                path: path.clone(),
                line,
                what: what.to_string(),
                text: "(unapproved test)".to_string(),
            });
        }
        // Past this line the file is read for shapes it *spells*, and this gate
        // has to spell every one of them. The path rules above have no such
        // problem and take no exemption.
        if EXEMPT.iter().any(|prefix| rel.starts_with(prefix)) {
            continue;
        }
        if pasted_key(&text) {
            leaks.push(Finding {
                path: path.clone(),
                line: text[..text.find(KEY_BLOCK).unwrap_or(0)].lines().count() + 1,
                what: "pasted private key".to_string(),
                text: "(redacted)".to_string(),
            });
            continue;
        }
        scanned += 1;
        for (n, line) in text.lines().enumerate() {
            if let Some(what) = scan_line(line) {
                offsets.push(Finding {
                    path: path.clone(),
                    line: n + 1,
                    what,
                    text: line.trim().chars().take(120).collect(),
                });
            }
        }
    }
    for finding in leaks.iter().chain(&offsets).chain(&tests) {
        println!(
            "{}:{}: {} — {}",
            finding.path.display(),
            finding.line,
            finding.what,
            finding.text
        );
    }
    if !leaks.is_empty() {
        return Err(format!(
            "{} file(s) that are not the product are staged for publication; \
             none of it belongs in a commit",
            leaks.len()
        ));
    }
    if !offsets.is_empty() {
        return Err(format!(
            "{} retail offset(s) in the tracked tree; a standalone runtime \
             resolves nothing against a retail image, so each one is dead weight",
            offsets.len()
        ));
    }
    if !tests.is_empty() {
        return Err(format!(
            "{} cargo test(s) in the tracked tree; the only permanent tests are \
             the owner-approved scenarios in crates/approved_tests",
            tests.len()
        ));
    }
    println!(
        "publish-check: clean — nothing but the product is tracked, no cargo test, \
         and no retail offsets in the {scanned} text file(s) read"
    );
    Ok(())
}
