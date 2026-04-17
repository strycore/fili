//! Path classification rules — loaded from rules.json (+ optional user overlay).
//!
//! A rule matches when every predicate it declares holds:
//!   - `path`    (pattern with `{captures}` + optional `**/` prefix)
//!   - `where`   (constrains a capture to membership in a named group)
//!   - `contains` (marker files/dirs exist as children)
//!   - `majority_ext` (>50% of direct files have one of these extensions)
//!
//! Predicates are evaluated cheapest-first, first matching rule wins.

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

use crate::models::{BaseType, PrivacyLevel, Tag};

// ---------- Raw (JSON) shapes ----------

#[derive(Debug, Default, Deserialize)]
struct RulesFile {
    #[allow(dead_code)]
    version: u32,
    #[serde(default)]
    skip: Vec<SkipRule>,
    #[serde(default)]
    privacy: Vec<PrivacyRule>,
    #[serde(default)]
    groups: HashMap<String, Vec<String>>,
    #[serde(default)]
    r#match: Vec<MatchRule>,
    /// Extension → base_type map. Populated from the `extensions` section
    /// of rules.json; inverted into a flat ext → base_type table when
    /// compiling the engine.
    #[serde(default)]
    extensions: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
struct SkipRule {
    pattern: String,
}

#[derive(Debug, Clone, Deserialize)]
struct PrivacyRule {
    pattern: String,
    level: String,
}

#[derive(Debug, Clone, Deserialize)]
struct MatchRule {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    contains: Option<OneOrMany>,
    #[serde(default)]
    majority_ext: Option<Vec<String>>,
    base: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    stop: bool,
    /// If true, the matched entry is an **item** (atomic unit — no children
    /// indexed). Orthogonal to `stop`: an entry can be a collection with
    /// stop=true (/bin — we don't auto-walk but it's still a collection).
    #[serde(default)]
    item: bool,
    #[serde(default, rename = "where")]
    where_: HashMap<String, String>,
}

/// Accepts either a single string or an array of strings in JSON.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    fn into_vec(self) -> Vec<String> {
        match self {
            OneOrMany::One(s) => vec![s],
            OneOrMany::Many(v) => v,
        }
    }
}

// ---------- Compiled engine ----------

pub struct RulesEngine {
    skip_patterns: Vec<CompiledSkip>,
    privacy_patterns: Vec<CompiledPrivacy>,
    groups: HashMap<String, Vec<String>>,
    match_rules: Vec<CompiledMatch>,
    /// Lowercased extension → base type lookup for file indexing.
    extension_map: HashMap<String, BaseType>,
}

#[derive(Debug)]
enum CompiledSkip {
    /// `**/suffix` — matches if any path segment ending matches.
    AnySuffix(String),
    /// Exact prefix match after home expansion.
    Prefix(String),
}

#[derive(Debug)]
struct CompiledPrivacy {
    prefix: String,
    level: PrivacyLevel,
}

#[derive(Debug)]
struct CompiledMatch {
    path: Option<CompiledPath>,
    contains: Vec<String>,
    majority_ext: Vec<String>,
    base: BaseType,
    tag_templates: Vec<String>,
    stop: bool,
    item: bool,
    where_: HashMap<String, String>,
}

#[derive(Debug)]
enum CompiledPath {
    /// Match the whole path (segment count must equal).
    Exact(Vec<Segment>),
    /// Match the last N segments of the path (zero or more leading segments allowed).
    Suffix(Vec<Segment>),
}

/// One path segment (between slashes). A segment is a sequence of parts:
/// literal text, a `*` wildcard, or a `{name}` capture. Captures/wildcards
/// are anchored by adjacent literals; two consecutive placeholders are
/// rejected at parse time as ambiguous.
#[derive(Debug)]
struct Segment {
    parts: Vec<SegmentPart>,
}

#[derive(Debug)]
enum SegmentPart {
    Literal(String),
    /// `*` — matches any substring (including empty), no capture.
    Wildcard,
    /// `{name}` — matches any non-empty substring, captures as `name`.
    Capture(String),
}

#[derive(Debug, Clone)]
pub struct MatchResult {
    pub base_type: BaseType,
    pub tags: Vec<Tag>,
    pub stop: bool,
    pub item: bool,
}

// ---------- Public API ----------

impl RulesEngine {
    /// Load embedded default, optionally merged with `~/.config/fili/rules.local.json`.
    pub fn load() -> Self {
        let raw = load_raw();
        let home = home_dir();
        Self::compile(raw, home)
    }

    fn compile(raw: RulesFile, home: String) -> Self {
        let skip_patterns = raw
            .skip
            .into_iter()
            .map(|r| compile_skip(&r.pattern, &home))
            .collect();

        let privacy_patterns = raw
            .privacy
            .into_iter()
            .map(|r| CompiledPrivacy {
                prefix: expand_home(&r.pattern, &home),
                level: PrivacyLevel::from_str(&r.level),
            })
            .collect();

        let match_rules = raw
            .r#match
            .into_iter()
            .map(|r| CompiledMatch {
                path: r.path.map(|p| compile_path(&expand_home(&p, &home))),
                contains: r.contains.map(|c| c.into_vec()).unwrap_or_default(),
                majority_ext: r
                    .majority_ext
                    .unwrap_or_default()
                    .into_iter()
                    .map(|e| e.to_lowercase())
                    .collect(),
                base: BaseType::from_str(&r.base),
                tag_templates: r.tags,
                stop: r.stop,
                item: r.item,
                where_: r.where_,
            })
            .collect();

        let mut extension_map: HashMap<String, BaseType> = HashMap::new();
        for (type_str, exts) in raw.extensions {
            let base = BaseType::from_str(&type_str);
            for ext in exts {
                // Later entries win on duplicates — rare, not worth a conflict
                // check until the file grows.
                extension_map.insert(ext.to_lowercase(), base);
            }
        }

        RulesEngine {
            skip_patterns,
            privacy_patterns,
            groups: raw.groups,
            match_rules,
            extension_map,
        }
    }

    /// Resolve a filename's extension to a base type, if known.
    /// Returns None for extension-less files or unregistered extensions.
    pub fn lookup_extension(&self, filename: &str) -> Option<BaseType> {
        let dot = filename.rfind('.')?;
        if dot == 0 {
            return None; // dotfile with no extension (".bashrc")
        }
        let ext = filename[dot + 1..].to_lowercase();
        if ext.is_empty() {
            return None;
        }
        self.extension_map.get(&ext).copied()
    }

    pub fn should_skip(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        for rule in &self.skip_patterns {
            match rule {
                CompiledSkip::AnySuffix(suffix) => {
                    if path_str.ends_with(suffix) || path_str.contains(&format!("/{}/", suffix)) {
                        return true;
                    }
                }
                CompiledSkip::Prefix(prefix) => {
                    if path_str == prefix.as_str() || path_str.starts_with(&format!("{}/", prefix))
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn privacy_for(&self, path: &Path) -> Option<PrivacyLevel> {
        let path_str = path.to_string_lossy();
        for rule in &self.privacy_patterns {
            if path_str == rule.prefix.as_str()
                || path_str.starts_with(&format!("{}/", rule.prefix))
            {
                return Some(rule.level);
            }
        }
        None
    }

    /// Match a path against the rules. First match wins. `path` must exist on disk
    /// for `contains`/`majority_ext` predicates to succeed.
    pub fn match_path(&self, path: &Path) -> Option<MatchResult> {
        let path_str = path.to_string_lossy();
        let parts: Vec<&str> = split_path(&path_str);

        // majority_ext is expensive (read_dir); cache the result once per match_path call.
        let mut ext_cache: Option<Option<Vec<String>>> = None;

        for rule in &self.match_rules {
            // Predicate 1: path pattern (cheapest — string ops)
            let captures = match &rule.path {
                Some(cp) => match match_path_pattern(cp, &parts) {
                    Some(c) => c,
                    None => continue,
                },
                None => HashMap::new(),
            };

            // Predicate 2: where constraints (capture-to-group)
            if !self.check_where(&rule.where_, &captures) {
                continue;
            }

            // Predicate 3: contains (one exists() per marker)
            if !rule.contains.is_empty() && !rule.contains.iter().all(|m| path.join(m).exists()) {
                continue;
            }

            // Predicate 4: majority_ext (read_dir, cached)
            if !rule.majority_ext.is_empty() {
                let exts = match &ext_cache {
                    Some(c) => c.clone(),
                    None => {
                        let e = read_direct_extensions(path);
                        ext_cache = Some(e.clone());
                        e
                    }
                };
                match exts {
                    Some(list) if has_majority(&list, &rule.majority_ext) => {}
                    _ => continue,
                }
            }

            let tags = rule
                .tag_templates
                .iter()
                .map(|t| expand_tag(t, &captures))
                .collect();

            return Some(MatchResult {
                base_type: rule.base,
                tags,
                // Items are atomic — stop is always implied. Collections can
                // still be stop=true independently (e.g. /bin is a collection
                // we don't auto-walk).
                stop: rule.stop || rule.item,
                item: rule.item,
            });
        }

        None
    }

    fn check_where(
        &self,
        constraints: &HashMap<String, String>,
        captures: &HashMap<String, String>,
    ) -> bool {
        for (capture_name, group_name) in constraints {
            let Some(value) = captures.get(capture_name) else {
                return false;
            };
            let Some(group) = self.groups.get(group_name) else {
                return false;
            };
            if !group.iter().any(|g| g == value) {
                return false;
            }
        }
        true
    }
}

// ---------- Loading & merging ----------

fn load_raw() -> RulesFile {
    let mut base = load_embedded();
    if let Some(user) = load_user_overlay() {
        merge_user(&mut base, user);
    }
    base
}

fn load_embedded() -> RulesFile {
    let default_json = include_str!("../rules.json");
    serde_json::from_str(default_json).expect("invalid embedded rules.json")
}

fn load_user_overlay() -> Option<RulesFile> {
    let path = directories::BaseDirs::new()?
        .config_dir()
        .join("fili/rules.local.json");
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(&path).ok()?;
    match serde_json::from_str(&content) {
        Ok(r) => Some(r),
        Err(e) => {
            eprintln!("warning: failed to parse {}: {}", path.display(), e);
            None
        }
    }
}

/// User entries win: prepended for ordered lists, merged for groups.
fn merge_user(base: &mut RulesFile, user: RulesFile) {
    let mut skip = user.skip;
    skip.extend(std::mem::take(&mut base.skip));
    base.skip = skip;

    let mut privacy = user.privacy;
    privacy.extend(std::mem::take(&mut base.privacy));
    base.privacy = privacy;

    let mut rules = user.r#match;
    rules.extend(std::mem::take(&mut base.r#match));
    base.r#match = rules;

    for (k, v) in user.groups {
        base.groups.entry(k).or_default().extend(v);
    }
}

fn home_dir() -> String {
    directories::BaseDirs::new()
        .map(|d| d.home_dir().to_string_lossy().to_string())
        .unwrap_or_default()
}

fn expand_home(pattern: &str, home: &str) -> String {
    if let Some(rest) = pattern.strip_prefix("~/") {
        format!("{}/{}", home, rest)
    } else if pattern == "~" {
        home.to_string()
    } else {
        pattern.to_string()
    }
}

fn compile_skip(pattern: &str, home: &str) -> CompiledSkip {
    if let Some(suffix) = pattern.strip_prefix("**/") {
        CompiledSkip::AnySuffix(suffix.to_string())
    } else {
        CompiledSkip::Prefix(expand_home(pattern, home))
    }
}

fn compile_path(pattern: &str) -> CompiledPath {
    if let Some(rest) = pattern.strip_prefix("**/") {
        CompiledPath::Suffix(parse_segments(rest))
    } else {
        CompiledPath::Exact(parse_segments(pattern))
    }
}

fn parse_segments(pattern: &str) -> Vec<Segment> {
    split_path(pattern).into_iter().map(parse_segment).collect()
}

/// Parse one path segment (no slashes) into an alternating list of literal
/// runs and placeholders. Two consecutive placeholders are rejected since
/// they can't be delimited.
fn parse_segment(seg: &str) -> Segment {
    let mut parts: Vec<SegmentPart> = Vec::new();
    let mut literal = String::new();
    let mut chars = seg.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '{' => {
                if !literal.is_empty() {
                    parts.push(SegmentPart::Literal(std::mem::take(&mut literal)));
                }
                let mut name = String::new();
                for c in chars.by_ref() {
                    if c == '}' {
                        break;
                    }
                    name.push(c);
                }
                if let Some(SegmentPart::Capture(_) | SegmentPart::Wildcard) = parts.last() {
                    panic!(
                        "invalid pattern segment {:?}: two placeholders must be separated by literal text",
                        seg
                    );
                }
                parts.push(SegmentPart::Capture(name));
            }
            '*' => {
                if !literal.is_empty() {
                    parts.push(SegmentPart::Literal(std::mem::take(&mut literal)));
                }
                if let Some(SegmentPart::Capture(_) | SegmentPart::Wildcard) = parts.last() {
                    panic!(
                        "invalid pattern segment {:?}: two placeholders must be separated by literal text",
                        seg
                    );
                }
                parts.push(SegmentPart::Wildcard);
            }
            _ => literal.push(c),
        }
    }
    if !literal.is_empty() {
        parts.push(SegmentPart::Literal(literal));
    }
    if parts.is_empty() {
        parts.push(SegmentPart::Literal(String::new()));
    }
    Segment { parts }
}

fn split_path(s: &str) -> Vec<&str> {
    s.split('/').filter(|p| !p.is_empty()).collect()
}

// ---------- Matching primitives ----------

fn match_path_pattern(cp: &CompiledPath, parts: &[&str]) -> Option<HashMap<String, String>> {
    match cp {
        CompiledPath::Exact(segs) => match_segments(segs, parts),
        CompiledPath::Suffix(segs) => {
            if segs.len() > parts.len() {
                return None;
            }
            let start = parts.len() - segs.len();
            match_segments(segs, &parts[start..])
        }
    }
}

fn match_segments(pattern: &[Segment], parts: &[&str]) -> Option<HashMap<String, String>> {
    if pattern.len() != parts.len() {
        return None;
    }
    let mut captures = HashMap::new();
    for (seg, part) in pattern.iter().zip(parts.iter()) {
        if !match_one_segment(seg, part, &mut captures) {
            return None;
        }
    }
    Some(captures)
}

/// Match a single segment's parts against a concrete path part. Parser
/// guarantees placeholders are always separated by literals, so matching
/// is unambiguous: advance through the part anchoring literals and letting
/// each placeholder consume what's between them.
fn match_one_segment(seg: &Segment, input: &str, captures: &mut HashMap<String, String>) -> bool {
    // Fast path: single literal — most rules use this.
    if let [SegmentPart::Literal(lit)] = seg.parts.as_slice() {
        return lit == input;
    }
    // Single placeholder covering the whole segment.
    if seg.parts.len() == 1 {
        match &seg.parts[0] {
            SegmentPart::Capture(name) => {
                if input.is_empty() {
                    return false;
                }
                captures.insert(name.clone(), input.to_string());
                return true;
            }
            SegmentPart::Wildcard => return true,
            SegmentPart::Literal(lit) => return lit == input,
        }
    }

    let mut cursor = 0usize;
    let mut pending: Option<&SegmentPart> = None;

    for part in &seg.parts {
        match part {
            SegmentPart::Literal(lit) => match pending.take() {
                None => {
                    if !input[cursor..].starts_with(lit.as_str()) {
                        return false;
                    }
                    cursor += lit.len();
                }
                Some(placeholder) => {
                    // Find the literal somewhere after cursor; content between
                    // cursor and the match is what the placeholder consumes.
                    let Some(rel) = input[cursor..].find(lit.as_str()) else {
                        return false;
                    };
                    let taken = &input[cursor..cursor + rel];
                    match placeholder {
                        SegmentPart::Capture(name) => {
                            if taken.is_empty() {
                                return false;
                            }
                            captures.insert(name.clone(), taken.to_string());
                        }
                        SegmentPart::Wildcard => {}
                        SegmentPart::Literal(_) => unreachable!(),
                    }
                    cursor += rel + lit.len();
                }
            },
            SegmentPart::Capture(_) | SegmentPart::Wildcard => {
                pending = Some(part);
            }
        }
    }

    // Anything left after the last literal goes to a trailing placeholder
    // (or must be empty if the segment ended on a literal).
    match pending {
        None => cursor == input.len(),
        Some(SegmentPart::Capture(name)) => {
            let rest = &input[cursor..];
            if rest.is_empty() {
                return false;
            }
            captures.insert(name.clone(), rest.to_string());
            true
        }
        Some(SegmentPart::Wildcard) => true,
        Some(SegmentPart::Literal(_)) => unreachable!(),
    }
}

fn expand_tag(template: &str, captures: &HashMap<String, String>) -> Tag {
    let expanded = expand_placeholders(template, captures);
    Tag::parse(&expanded)
}

fn expand_placeholders(template: &str, captures: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let Some(end_rel) = rest[start..].find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let end = start + end_rel;
        let name = &rest[start + 1..end];
        if let Some(value) = captures.get(name) {
            out.push_str(value);
        } else {
            out.push_str(&rest[start..=end]);
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

// ---------- Content predicate helpers ----------

/// Read extensions of direct-child files (lowercased). Returns None if read_dir fails
/// or the directory is empty of files (no meaningful majority).
fn read_direct_extensions(path: &Path) -> Option<Vec<String>> {
    const SAMPLE_CAP: usize = 200;

    let entries = std::fs::read_dir(path).ok()?;
    let mut exts = Vec::new();
    for entry in entries.flatten().take(SAMPLE_CAP) {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_file() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(dot) = name.rfind('.') {
            exts.push(name[dot + 1..].to_lowercase());
        }
    }
    if exts.is_empty() {
        None
    } else {
        Some(exts)
    }
}

fn has_majority(actual: &[String], wanted: &[String]) -> bool {
    if actual.is_empty() {
        return false;
    }
    let matches = actual
        .iter()
        .filter(|e| wanted.iter().any(|w| w == *e))
        .count();
    matches * 2 > actual.len()
}

// ---------- Tests ----------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_engine() -> RulesEngine {
        let raw = load_embedded();
        RulesEngine::compile(raw, "/home/user".to_string())
    }

    #[test]
    fn matches_music_artist_album() {
        let engine = test_engine();
        let r = engine
            .match_path(&PathBuf::from("/home/user/Music/Pink Floyd/The Wall"))
            .expect("should match");
        assert_eq!(r.base_type, BaseType::Audio);
        // Albums recurse now so tracks can be indexed as file items.
        assert!(!r.stop);
        assert!(!r.item);
        assert!(r.tags.iter().any(|t| t.key == "album"));
    }

    #[test]
    fn lookup_extension_audio() {
        let engine = test_engine();
        assert_eq!(engine.lookup_extension("track.flac"), Some(BaseType::Audio));
        assert_eq!(engine.lookup_extension("TRACK.FLAC"), Some(BaseType::Audio));
        assert_eq!(engine.lookup_extension("image.jpg"), Some(BaseType::Image));
        assert_eq!(engine.lookup_extension("README"), None);
        assert_eq!(engine.lookup_extension(".bashrc"), None);
        assert_eq!(engine.lookup_extension("random.xyz"), None);
    }

    #[test]
    fn matches_game_store() {
        let engine = test_engine();
        let r = engine
            .match_path(&PathBuf::from("/home/user/Games/gog/alan-wake"))
            .expect("should match");
        assert_eq!(r.base_type, BaseType::Game);
        assert!(r.stop);
        assert!(r
            .tags
            .iter()
            .any(|t| t.key == "store" && t.value.as_deref() == Some("gog")));
    }

    #[test]
    fn matches_emulator() {
        let engine = test_engine();
        let r = engine
            .match_path(&PathBuf::from("/home/user/Games/cemu"))
            .expect("should match");
        assert_eq!(r.base_type, BaseType::Emulator);
        assert!(r
            .tags
            .iter()
            .any(|t| t.key == "name" && t.value.as_deref() == Some("cemu")));
    }

    #[test]
    fn glob_suffix_matches_node_modules() {
        let engine = test_engine();
        let r = engine
            .match_path(&PathBuf::from("/home/user/Projects/fili/node_modules"))
            .expect("should match");
        assert_eq!(r.base_type, BaseType::Dependencies);
        assert!(r.stop);
        assert!(r.item);
    }

    #[test]
    fn glob_suffix_matches_nested_target_debug() {
        let engine = test_engine();
        let r = engine
            .match_path(&PathBuf::from("/any/where/target/debug"))
            .expect("should match");
        assert_eq!(r.base_type, BaseType::BuildArtifact);
        assert!(r.item);
    }

    #[test]
    fn contains_cargo_toml_classifies_as_rust() {
        // Build a rule set with only the Cargo.toml contains rule, match against our own repo root.
        let raw: RulesFile = serde_json::from_str(
            r#"{"version":1,"match":[
                {"contains":"Cargo.toml","base":"code","tags":["lang=rust"],"stop":true}
            ]}"#,
        )
        .unwrap();
        let engine = RulesEngine::compile(raw, "/unused".to_string());
        let repo_root = std::env::current_dir().unwrap();
        let r = engine.match_path(&repo_root).expect("should match");
        assert_eq!(r.base_type, BaseType::Code);
        assert!(r
            .tags
            .iter()
            .any(|t| t.key == "lang" && t.value.as_deref() == Some("rust")));
    }

    #[test]
    fn contains_without_marker_does_not_match() {
        let raw: RulesFile = serde_json::from_str(
            r#"{"version":1,"match":[
                {"contains":"DEFINITELY_NOT_A_REAL_FILE","base":"code","stop":true}
            ]}"#,
        )
        .unwrap();
        let engine = RulesEngine::compile(raw, "/unused".to_string());
        let r = engine.match_path(&std::env::current_dir().unwrap());
        assert!(r.is_none());
    }

    #[test]
    fn majority_ext_photo_album() -> Result<(), Box<dyn std::error::Error>> {
        // Build a tempdir with mostly .jpg files.
        let dir = tempfile::tempdir()?;
        for i in 0..10 {
            std::fs::write(dir.path().join(format!("p{i}.jpg")), b"")?;
        }
        std::fs::write(dir.path().join("notes.txt"), b"")?;

        let raw: RulesFile = serde_json::from_str(
            r#"{"version":1,"match":[
                {"majority_ext":["jpg","png"],"base":"image","tags":["album"],"stop":true}
            ]}"#,
        )
        .unwrap();
        let engine = RulesEngine::compile(raw, "/unused".to_string());
        let r = engine.match_path(dir.path()).expect("should match");
        assert_eq!(r.base_type, BaseType::Image);
        Ok(())
    }

    #[test]
    fn majority_ext_fails_without_majority() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("a.jpg"), b"")?;
        for i in 0..5 {
            std::fs::write(dir.path().join(format!("n{i}.txt")), b"")?;
        }
        let raw: RulesFile = serde_json::from_str(
            r#"{"version":1,"match":[
                {"majority_ext":["jpg"],"base":"image","stop":true}
            ]}"#,
        )
        .unwrap();
        let engine = RulesEngine::compile(raw, "/unused".to_string());
        assert!(engine.match_path(dir.path()).is_none());
        Ok(())
    }

    #[test]
    fn skip_matches_prefix_and_glob_suffix() {
        let raw: RulesFile = serde_json::from_str(
            r#"{"version":1,"skip":[
                {"pattern":"/proc"},
                {"pattern":"**/node_modules"}
            ]}"#,
        )
        .unwrap();
        let engine = RulesEngine::compile(raw, "/home/user".to_string());
        assert!(engine.should_skip(&PathBuf::from("/proc/self")));
        assert!(engine.should_skip(&PathBuf::from("/proc")));
        assert!(engine.should_skip(&PathBuf::from("/home/user/proj/node_modules")));
        assert!(!engine.should_skip(&PathBuf::from("/home/user/.cache")));
    }

    #[test]
    fn cache_path_classifies_and_stops() {
        let engine = test_engine();
        let r = engine
            .match_path(&PathBuf::from("/home/user/.cache"))
            .expect("should match");
        assert_eq!(r.base_type, BaseType::Cache);
        assert!(r.stop);
    }

    #[test]
    fn wildcard_prefix_captures_version() {
        let raw: RulesFile = serde_json::from_str(
            r#"{"version":1,"match":[
                {"path":"/opt/rocm-{version}","base":"application",
                 "tags":["name=rocm","version={version}"],"stop":true}
            ]}"#,
        )
        .unwrap();
        let engine = RulesEngine::compile(raw, "/unused".to_string());
        let r = engine
            .match_path(&PathBuf::from("/opt/rocm-6.2.4"))
            .expect("should match");
        assert!(r
            .tags
            .iter()
            .any(|t| t.key == "name" && t.value.as_deref() == Some("rocm")));
        assert!(r
            .tags
            .iter()
            .any(|t| t.key == "version" && t.value.as_deref() == Some("6.2.4")));
    }

    #[test]
    fn wildcard_star_matches_any_suffix() {
        let raw: RulesFile = serde_json::from_str(
            r#"{"version":1,"match":[
                {"path":"/opt/signal-*","base":"application","stop":true}
            ]}"#,
        )
        .unwrap();
        let engine = RulesEngine::compile(raw, "/unused".to_string());
        assert!(engine
            .match_path(&PathBuf::from("/opt/signal-cli-0.14.1"))
            .is_some());
        assert!(engine.match_path(&PathBuf::from("/opt/signal")).is_none());
        assert!(engine
            .match_path(&PathBuf::from("/opt/other-cli"))
            .is_none());
    }

    #[test]
    fn wildcard_prefix_and_suffix_captures_middle() {
        let raw: RulesFile = serde_json::from_str(
            r#"{"version":1,"match":[
                {"path":"/opt/{app}-linux-x86_64","base":"application",
                 "tags":["name={app}"],"stop":true}
            ]}"#,
        )
        .unwrap();
        let engine = RulesEngine::compile(raw, "/unused".to_string());
        let r = engine
            .match_path(&PathBuf::from("/opt/firefox-linux-x86_64"))
            .expect("should match");
        assert!(r
            .tags
            .iter()
            .any(|t| t.key == "name" && t.value.as_deref() == Some("firefox")));
    }

    #[test]
    fn privacy_lookup() {
        let engine = test_engine();
        assert_eq!(
            engine.privacy_for(&PathBuf::from("/home/user/.ssh/id_rsa")),
            Some(PrivacyLevel::Confidential)
        );
    }
}
