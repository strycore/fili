//! Path classification rules — loaded from rules.json (+ optional user overlay).
//!
//! A rule matches when every predicate it declares holds:
//!   - `path`    (pattern with `{captures}` + optional `**/` or `<home>/` prefix)
//!   - `where`   (constrains a capture to membership in a named group)
//!   - `contains` (marker files/dirs exist as children)
//!   - `majority_ext` (>50% of direct files have one of these extensions)
//!
//! Predicates are evaluated cheapest-first, first matching rule wins.
//!
//! Path prefixes:
//!   - `/absolute/path` — literal, matches exactly.
//!   - `**/suffix` — matches the last N segments of any path.
//!   - `<home>/suffix` — scope-relative. Matches against the user's actual
//!     `$HOME` plus any `home`-tagged ancestor active during the scan, so
//!     the same rule applies to backups of home directories on other drives.

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
    /// Per-extension overrides keyed by the parent collection's base_type.
    /// Shape: {extension: {parent_base_type: override_base_type}}.
    /// Applied on top of the `extensions` map when indexing a file.
    #[serde(default)]
    extension_context: HashMap<String, HashMap<String, String>>,
    /// Tags attached to files by extension when the file indexer creates
    /// them. Lets a ".torrent" file be classified as a document *and*
    /// tagged `kind=torrent` — richer than the base type alone.
    #[serde(default)]
    extension_tags: HashMap<String, Vec<String>>,
    /// Filename-pattern rules for the file indexer. Unlike `match` rules
    /// which operate on directory paths, these match a single filename
    /// with `{captures}` — so `appmanifest_{appid}.acf` can carry the
    /// appid into a tag. First match wins, falls through to the plain
    /// extension-based classification when no pattern matches.
    #[serde(default)]
    file_rules: Vec<FileRule>,
}

#[derive(Debug, Clone, Deserialize)]
struct FileRule {
    name: String,
    base: String,
    #[serde(default)]
    tags: Vec<String>,
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
    /// User's actual home. Included implicitly as a home scope in every
    /// match/should_skip/privacy_for call so ordinary scans still work
    /// without the caller tracking scopes.
    default_home: String,
    skip_patterns: Vec<CompiledSkip>,
    privacy_patterns: Vec<CompiledPrivacy>,
    groups: HashMap<String, Vec<String>>,
    match_rules: Vec<CompiledMatch>,
    /// Lowercased extension → base type lookup for file indexing.
    extension_map: HashMap<String, BaseType>,
    /// Context overrides: ext → (parent_base_type → override_base_type).
    extension_context: HashMap<String, HashMap<BaseType, BaseType>>,
    /// Tags to attach to files by extension during file indexing.
    extension_tags: HashMap<String, Vec<Tag>>,
    /// Filename pattern rules for the file indexer. Checked before the
    /// extension map.
    file_rules: Vec<CompiledFileRule>,
}

#[derive(Debug)]
struct CompiledFileRule {
    name: Segment,
    base: BaseType,
    tag_templates: Vec<String>,
}

#[derive(Debug)]
enum CompiledSkip {
    /// `**/suffix` — matches if any path segment ending matches.
    AnySuffix(String),
    /// Exact absolute-path prefix.
    Prefix(String),
    /// `<home>/suffix` — matches against each active home scope.
    HomeRelative(String),
}

#[derive(Debug)]
struct CompiledPrivacy {
    /// For Absolute, `prefix` is the exact path. For HomeRelative, it's the
    /// suffix to join with each home scope.
    prefix: String,
    scope: PrivacyScope,
    level: PrivacyLevel,
}

#[derive(Debug)]
enum PrivacyScope {
    Absolute,
    HomeRelative,
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
    /// `<home>/...` — match the segments relative to any active home scope
    /// (the user's $HOME plus ancestors classified as `home` during the scan).
    HomeRelative(Vec<Segment>),
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
            .map(|r| compile_skip(&r.pattern))
            .collect();

        let privacy_patterns = raw
            .privacy
            .into_iter()
            .map(|r| {
                let (prefix, scope) = if let Some(rest) = r.pattern.strip_prefix("<home>/") {
                    (rest.to_string(), PrivacyScope::HomeRelative)
                } else {
                    (r.pattern, PrivacyScope::Absolute)
                };
                CompiledPrivacy {
                    prefix,
                    scope,
                    level: PrivacyLevel::from_str(&r.level),
                }
            })
            .collect();

        let match_rules = raw
            .r#match
            .into_iter()
            .map(|r| CompiledMatch {
                path: r.path.map(|p| compile_path(&p)),
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

        // Extension -> base_type lookup. Some extensions appear in multiple
        // category lists (md = Markdown or Mega Drive ROM; iso = archive or
        // game disc; bin = executable or ROM). Iterating rules.json's
        // categories in a fixed precedence and keeping the first insert
        // makes the default deterministic and biased toward the more common
        // meaning. Context overrides (see `extension_context`) still let a
        // book library reclassify .pdf as book, etc.
        const EXTENSION_PRIORITY: &[&str] = &[
            "document",
            "code",
            "book",
            "image",
            "audio",
            "video",
            "application",
            "archive",
            "game",
        ];
        let mut extension_map: HashMap<String, BaseType> = HashMap::new();
        let mut ordered: Vec<(&str, &Vec<String>)> = EXTENSION_PRIORITY
            .iter()
            .filter_map(|k| {
                raw.extensions
                    .get_key_value(*k)
                    .map(|(k, v)| (k.as_str(), v))
            })
            .collect();
        for (type_str, exts) in &raw.extensions {
            if !EXTENSION_PRIORITY.contains(&type_str.as_str()) {
                ordered.push((type_str.as_str(), exts));
            }
        }
        for (type_str, exts) in ordered {
            let base = BaseType::from_str(type_str);
            for ext in exts {
                extension_map.entry(ext.to_lowercase()).or_insert(base);
            }
        }

        let mut extension_context: HashMap<String, HashMap<BaseType, BaseType>> = HashMap::new();
        for (ext, overrides) in raw.extension_context {
            let key = ext.to_lowercase();
            let mut parent_map = HashMap::new();
            for (parent_type, override_type) in overrides {
                parent_map.insert(
                    BaseType::from_str(&parent_type),
                    BaseType::from_str(&override_type),
                );
            }
            extension_context.insert(key, parent_map);
        }

        let mut extension_tags: HashMap<String, Vec<Tag>> = HashMap::new();
        for (ext, tag_templates) in raw.extension_tags {
            let tags: Vec<Tag> = tag_templates.iter().map(|t| Tag::parse(t)).collect();
            extension_tags.insert(ext.to_lowercase(), tags);
        }

        let file_rules: Vec<CompiledFileRule> = raw
            .file_rules
            .into_iter()
            .map(|r| CompiledFileRule {
                name: parse_segment(&r.name),
                base: BaseType::from_str(&r.base),
                tag_templates: r.tags,
            })
            .collect();

        RulesEngine {
            default_home: home,
            skip_patterns,
            privacy_patterns,
            groups: raw.groups,
            match_rules,
            extension_map,
            extension_context,
            extension_tags,
            file_rules,
        }
    }

    /// Match a filename against the file-name pattern rules. First match
    /// wins. Returns the declared base_type and tag set (with captures
    /// from the pattern expanded). None when no rule matches.
    pub fn match_filename(&self, filename: &str) -> Option<(BaseType, Vec<Tag>)> {
        for rule in &self.file_rules {
            let mut captures = HashMap::new();
            if match_one_segment(&rule.name, filename, &mut captures) {
                let tags: Vec<Tag> = rule
                    .tag_templates
                    .iter()
                    .map(|t| expand_tag(t, &captures))
                    .collect();
                return Some((rule.base, tags));
            }
        }
        None
    }

    /// Tags to attach to an indexed file by extension (e.g. .torrent →
    /// `kind=torrent`). Empty vec when no tags are configured.
    pub fn tags_for_extension(&self, filename: &str) -> Vec<Tag> {
        let Some(dot) = filename.rfind('.') else {
            return Vec::new();
        };
        if dot == 0 {
            return Vec::new();
        }
        let ext = filename[dot + 1..].to_lowercase();
        self.extension_tags.get(&ext).cloned().unwrap_or_default()
    }

    /// Resolve a filename's extension to a base type, if known.
    /// Returns None for extension-less files or unregistered extensions.
    ///
    /// `parent_context` is the base_type of the collection holding the file.
    /// If a matching `extension_context` entry exists, it overrides the
    /// default (e.g. .pdf defaults to document but becomes book when the
    /// parent collection is a book library).
    pub fn lookup_extension(
        &self,
        filename: &str,
        parent_context: Option<BaseType>,
    ) -> Option<BaseType> {
        let dot = filename.rfind('.')?;
        if dot == 0 {
            return None;
        }
        let ext = filename[dot + 1..].to_lowercase();
        if ext.is_empty() {
            return None;
        }
        if let Some(parent) = parent_context {
            if let Some(overrides) = self.extension_context.get(&ext) {
                if let Some(&resolved) = overrides.get(&parent) {
                    return Some(resolved);
                }
            }
        }
        self.extension_map.get(&ext).copied()
    }

    pub fn should_skip(&self, path: &Path) -> bool {
        self.should_skip_scoped(path, &[])
    }

    pub fn should_skip_scoped(&self, path: &Path, extra_scopes: &[std::path::PathBuf]) -> bool {
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
                CompiledSkip::HomeRelative(suffix) => {
                    for scope in self.iter_scopes(extra_scopes) {
                        let full = format!("{}/{}", scope, suffix);
                        if path_str == full.as_str() || path_str.starts_with(&format!("{}/", full))
                        {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    pub fn privacy_for(&self, path: &Path) -> Option<PrivacyLevel> {
        self.privacy_for_scoped(path, &[])
    }

    pub fn privacy_for_scoped(
        &self,
        path: &Path,
        extra_scopes: &[std::path::PathBuf],
    ) -> Option<PrivacyLevel> {
        let path_str = path.to_string_lossy();
        for rule in &self.privacy_patterns {
            match rule.scope {
                PrivacyScope::Absolute => {
                    if path_str == rule.prefix.as_str()
                        || path_str.starts_with(&format!("{}/", rule.prefix))
                    {
                        return Some(rule.level);
                    }
                }
                PrivacyScope::HomeRelative => {
                    for scope in self.iter_scopes(extra_scopes) {
                        let full = format!("{}/{}", scope, rule.prefix);
                        if path_str == full.as_str() || path_str.starts_with(&format!("{}/", full))
                        {
                            return Some(rule.level);
                        }
                    }
                }
            }
        }
        None
    }

    /// Iterate home scopes: the engine's default_home first, then any extras
    /// provided by the caller (typically home-tagged ancestors during a scan).
    fn iter_scopes<'a>(
        &'a self,
        extra: &'a [std::path::PathBuf],
    ) -> impl Iterator<Item = std::borrow::Cow<'a, str>> {
        std::iter::once(std::borrow::Cow::Borrowed(self.default_home.as_str()))
            .chain(extra.iter().map(|p| p.to_string_lossy()))
    }

    /// Match a path against the rules. First match wins. `path` must exist on disk
    /// for `contains`/`majority_ext` predicates to succeed.
    pub fn match_path(&self, path: &Path) -> Option<MatchResult> {
        self.match_path_scoped(path, &[], None)
    }

    /// Like `match_path` but also considers the caller-provided home scopes
    /// (in addition to the user's $HOME) and an optional enclosing library
    /// scope. When inside a library of type X, library-declaring rules for
    /// any other type are skipped — so `**/Music` won't promote a folder
    /// to an audio library when it's nested under an image library.
    pub fn match_path_scoped(
        &self,
        path: &Path,
        extra_scopes: &[std::path::PathBuf],
        library_scope: Option<BaseType>,
    ) -> Option<MatchResult> {
        let path_str = path.to_string_lossy();
        let parts: Vec<&str> = split_path(&path_str);

        // majority_ext is expensive (read_dir); cache the result once per match_path call.
        let mut contents_cache: Option<Option<DirContents>> = None;

        // The library-scope gate is strict only for media scopes — inside
        // an image/audio/video/book library, a different-kind library
        // rule would almost always be wrong (a "Music" subfolder of an
        // Images library holds pictures *about* music, not audio files).
        // Non-media scopes (home, mount, inbox, code, archive, …) freely
        // nest libraries of any type.
        let strict_scope = match library_scope {
            Some(bt @ (BaseType::Image | BaseType::Audio | BaseType::Video | BaseType::Book)) => {
                Some(bt)
            }
            _ => None,
        };

        for rule in &self.match_rules {
            // Predicate 0: library-scope gate.
            if let Some(scope) = strict_scope {
                if rule.base != scope && rule.tag_templates.iter().any(|t| t == "library") {
                    continue;
                }
            }

            // Predicate 1: path pattern (cheapest — string ops)
            let captures = match &rule.path {
                Some(cp) => match self.match_path_pattern(cp, &parts, extra_scopes) {
                    Some(c) => c,
                    None => continue,
                },
                None => HashMap::new(),
            };

            // Predicate 2: where constraints (capture-to-group)
            if !self.check_where(&rule.where_, &captures) {
                continue;
            }

            // Predicate 3: contains. Literal markers must exist as child
            // files/dirs. A marker of the form `*.ext` matches if any
            // direct-child file has that extension — so a rule can say
            // "folder holds weights alongside configs" without listing
            // every possible filename.
            if !rule.contains.is_empty() {
                let mut all_present = true;
                for marker in &rule.contains {
                    let present = if let Some(ext) = marker.strip_prefix("*.") {
                        let contents =
                            contents_cache.get_or_insert_with(|| read_direct_contents(path));
                        contents
                            .as_ref()
                            .is_some_and(|c| c.file_exts.iter().any(|e| e == ext))
                    } else {
                        path.join(marker).exists()
                    };
                    if !present {
                        all_present = false;
                        break;
                    }
                }
                if !all_present {
                    continue;
                }
            }

            // Predicate 4: majority_ext (read_dir, cached)
            if !rule.majority_ext.is_empty() {
                let contents = contents_cache.get_or_insert_with(|| read_direct_contents(path));
                match contents {
                    Some(c) if matches_majority(c, &rule.majority_ext) => {}
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

fn compile_skip(pattern: &str) -> CompiledSkip {
    if let Some(suffix) = pattern.strip_prefix("**/") {
        CompiledSkip::AnySuffix(suffix.to_string())
    } else if let Some(rest) = pattern.strip_prefix("<home>/") {
        CompiledSkip::HomeRelative(rest.to_string())
    } else {
        CompiledSkip::Prefix(pattern.to_string())
    }
}

fn compile_path(pattern: &str) -> CompiledPath {
    if let Some(rest) = pattern.strip_prefix("**/") {
        CompiledPath::Suffix(parse_segments(rest))
    } else if let Some(rest) = pattern.strip_prefix("<home>/") {
        CompiledPath::HomeRelative(parse_segments(rest))
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

impl RulesEngine {
    fn match_path_pattern(
        &self,
        cp: &CompiledPath,
        parts: &[&str],
        extra_scopes: &[std::path::PathBuf],
    ) -> Option<HashMap<String, String>> {
        match cp {
            CompiledPath::Exact(segs) => match_segments(segs, parts),
            CompiledPath::Suffix(segs) => {
                if segs.len() > parts.len() {
                    return None;
                }
                let start = parts.len() - segs.len();
                match_segments(segs, &parts[start..])
            }
            CompiledPath::HomeRelative(segs) => {
                for scope in self.iter_scopes(extra_scopes) {
                    let scope_parts = split_path(scope.as_ref());
                    if parts.len() != scope_parts.len() + segs.len() {
                        continue;
                    }
                    if parts[..scope_parts.len()] != scope_parts[..] {
                        continue;
                    }
                    if let Some(caps) = match_segments(segs, &parts[scope_parts.len()..]) {
                        return Some(caps);
                    }
                }
                None
            }
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

struct DirContents {
    /// Extensions of direct-child files (lowercased).
    file_exts: Vec<String>,
    /// Count of direct-child subdirectories.
    subdir_count: usize,
}

/// Read direct-child files and subdirectory count. Returns None if read_dir
/// fails or the directory is empty of files (majority_ext only applies to
/// folders that contain files).
fn read_direct_contents(path: &Path) -> Option<DirContents> {
    const SAMPLE_CAP: usize = 500;

    let entries = std::fs::read_dir(path).ok()?;
    let mut file_exts = Vec::new();
    let mut subdir_count = 0usize;
    for entry in entries.flatten().take(SAMPLE_CAP) {
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            subdir_count += 1;
            continue;
        }
        if !ft.is_file() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(dot) = name.rfind('.') {
            file_exts.push(name[dot + 1..].to_lowercase());
        }
    }
    if file_exts.is_empty() {
        None
    } else {
        Some(DirContents {
            file_exts,
            subdir_count,
        })
    }
}

/// A folder is treated as a content leaf (album / rom-collection / scans…)
/// only when (a) more than half of direct files have a wanted extension AND
/// (b) it has fewer than `SUBDIR_LEAF_LIMIT` subdirectories. The absolute
/// cap is what stops a mixed folder like ~/Images/Music (dozens of direct
/// cover images + dozens of artist subfolders) from being flattened into a
/// single leaf album — past a handful of organized subfolders the user
/// clearly intended structure, not a leaf.
const SUBDIR_LEAF_LIMIT: usize = 5;

fn matches_majority(contents: &DirContents, wanted: &[String]) -> bool {
    if contents.file_exts.is_empty() {
        return false;
    }
    if contents.subdir_count >= SUBDIR_LEAF_LIMIT {
        return false;
    }
    let matches = contents
        .file_exts
        .iter()
        .filter(|e| wanted.iter().any(|w| w == *e))
        .count();
    matches * 2 > contents.file_exts.len()
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
    fn music_library_root_matches() {
        // The Music folder itself gets tagged as a library. Inner artist/album
        // folders have no positional rule — they're classified by content
        // (majority_ext) or synthesized as groupings by the scanner when
        // inside a library scope. This test just confirms the library root.
        let engine = test_engine();
        let r = engine
            .match_path(&PathBuf::from("/home/user/Music"))
            .expect("library root should match");
        assert_eq!(r.base_type, BaseType::Audio);
        assert!(r.tags.iter().any(|t| t.key == "library"));
        assert!(!r.item);
    }

    #[test]
    fn audio_folder_detected_by_majority_ext() -> Result<(), Box<dyn std::error::Error>> {
        // A folder whose direct files are mostly audio gets tagged as an
        // album regardless of its position in the tree.
        let dir = tempfile::tempdir()?;
        for i in 0..8 {
            std::fs::write(dir.path().join(format!("t{i:02}.mp3")), b"")?;
        }
        std::fs::write(dir.path().join("cover.jpg"), b"")?;

        let engine = test_engine();
        let r = engine.match_path(dir.path()).expect("should match album");
        assert_eq!(r.base_type, BaseType::Audio);
        assert!(r.item);
        assert!(r
            .tags
            .iter()
            .any(|t| t.key == "kind" && t.value.as_deref() == Some("album")));
        Ok(())
    }

    #[test]
    fn lookup_extension_defaults() {
        let engine = test_engine();
        assert_eq!(
            engine.lookup_extension("track.flac", None),
            Some(BaseType::Audio)
        );
        assert_eq!(
            engine.lookup_extension("TRACK.FLAC", None),
            Some(BaseType::Audio)
        );
        assert_eq!(
            engine.lookup_extension("image.jpg", None),
            Some(BaseType::Image)
        );
        assert_eq!(engine.lookup_extension("README", None), None);
        assert_eq!(engine.lookup_extension(".bashrc", None), None);
        assert_eq!(engine.lookup_extension("random.xyz", None), None);
    }

    #[test]
    fn lookup_extension_context_override() {
        let engine = test_engine();
        // PDF defaults to document everywhere.
        assert_eq!(
            engine.lookup_extension("story.pdf", None),
            Some(BaseType::Document)
        );
        assert_eq!(
            engine.lookup_extension("story.pdf", Some(BaseType::Audio)),
            Some(BaseType::Document)
        );
        // In a Book library it becomes a Book.
        assert_eq!(
            engine.lookup_extension("story.pdf", Some(BaseType::Book)),
            Some(BaseType::Book)
        );
        // README.md inside a code project is code, not a document.
        assert_eq!(
            engine.lookup_extension("README.md", None),
            Some(BaseType::Document)
        );
        assert_eq!(
            engine.lookup_extension("README.md", Some(BaseType::Code)),
            Some(BaseType::Code)
        );
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

    #[test]
    fn home_scope_applies_to_extra_scope() {
        let engine = test_engine();
        let backup = PathBuf::from("/mnt/backup/oldhome");
        let scopes = vec![backup.clone()];

        // Without the scope, the backup's .config is just an unknown folder.
        assert!(engine.match_path(&backup.join(".config/firefox")).is_none());

        // With the scope active, the same rules as the real home apply.
        let r = engine
            .match_path_scoped(&backup.join(".config/firefox"), &scopes, None)
            .expect("should match <home>/.config/{app}");
        assert_eq!(r.base_type, BaseType::Config);
        assert!(r
            .tags
            .iter()
            .any(|t| t.key == "app" && t.value.as_deref() == Some("firefox")));

        // Privacy too.
        assert_eq!(
            engine.privacy_for_scoped(&backup.join(".ssh/id_rsa"), &scopes),
            Some(PrivacyLevel::Confidential)
        );
    }
}
