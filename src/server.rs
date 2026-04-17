use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};

use crate::db::{Database, EntryFilter};
use crate::models::{BaseType, Drive, Entry, Location, PrivacyLevel, Stats, Tag, Unknown};

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum EntryState {
    /// Indexed as a classified entry (collection or item).
    Indexed,
    /// Discovered but unclassified.
    Unknown,
    /// Directory not yet seen by the scanner.
    Unscanned,
    /// Plain file (not indexed yet; file indexing lands later).
    File,
}

#[derive(Serialize)]
struct BrowseItem {
    name: String,
    path: String,
    is_dir: bool,
    state: EntryState,
    size: Option<u64>,
    mtime: Option<i64>,
    /// Populated when state == collection. Kept as `collection` in the JSON
    /// for UI compatibility — the field carries the Entry row now.
    #[serde(skip_serializing_if = "Option::is_none", rename = "collection")]
    entry: Option<Entry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unknown: Option<Unknown>,
}

#[derive(RustEmbed)]
#[folder = "assets/"]
struct Assets;

#[derive(Clone)]
struct AppState {
    db: Arc<Mutex<Database>>,
}

pub fn run(db: Database, addr: SocketAddr) -> Result<()> {
    let state = AppState {
        db: Arc::new(Mutex::new(db)),
    };

    let app = Router::new()
        .route("/api/stats", get(api_stats))
        .route("/api/locations", get(api_locations))
        // URL paths keep the "collections" name for UI compat; internally they
        // operate on the unified `entries` table.
        .route("/api/collections", get(api_entries))
        .route("/api/collections/:id", get(api_entry_detail))
        .route("/api/browse", get(api_browse))
        .route("/api/unknowns", get(api_unknowns))
        .route("/api/unknowns/:id/classify", post(api_classify_unknown))
        .route("/api/unknowns/bulk-classify", post(api_bulk_classify))
        .route("/api/drives", get(api_drives))
        .route("/api/drives/:id/rename", post(api_rename_drive))
        .route("/api/scan", post(api_scan))
        .fallback(static_handler)
        .with_state(state);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(addr).await?;
        println!("Fili serving at http://{}", addr);
        axum::serve(listener, app).await?;
        Ok::<_, anyhow::Error>(())
    })
}

// ---------- Handlers ----------

async fn api_stats(State(state): State<AppState>) -> Result<Json<Stats>, AppError> {
    let stats = tokio::task::spawn_blocking(move || {
        let db = state.db.lock().unwrap();
        db.get_stats()
    })
    .await??;
    Ok(Json(stats))
}

async fn api_locations(State(state): State<AppState>) -> Result<Json<Vec<Location>>, AppError> {
    let locations = tokio::task::spawn_blocking(move || {
        let db = state.db.lock().unwrap();
        db.list_locations()
    })
    .await??;
    Ok(Json(locations))
}

#[derive(Debug, Deserialize)]
struct EntriesQuery {
    #[serde(rename = "type")]
    base_type: Option<String>,
    privacy: Option<String>,
    parent: Option<String>, // "null" | "<id>" | None
    q: Option<String>,
    tag: Option<String>,
    is_item: Option<bool>,
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn api_entries(
    State(state): State<AppState>,
    Query(q): Query<EntriesQuery>,
) -> Result<Json<Vec<Entry>>, AppError> {
    let parent_id = match q.parent.as_deref() {
        Some("null") | Some("root") => Some(None),
        Some(other) => Some(Some(
            other
                .parse::<i64>()
                .map_err(|_| AppError::bad_request("invalid parent id"))?,
        )),
        None => None,
    };

    let (tag_key, tag_value) = match q.tag.as_deref() {
        Some(raw) if !raw.is_empty() => match raw.split_once('=') {
            Some((k, v)) => (Some(k.trim().to_string()), Some(v.trim().to_string())),
            None => (Some(raw.trim().to_string()), None),
        },
        _ => (None, None),
    };

    let filter = EntryFilter {
        base_type: q.base_type,
        privacy: q.privacy,
        parent_id,
        query: q.q,
        tag_key,
        tag_value,
        is_item: q.is_item,
        limit: q.limit,
        offset: q.offset,
    };

    let entries = tokio::task::spawn_blocking(move || {
        let db = state.db.lock().unwrap();
        db.list_entries(&filter)
    })
    .await??;
    Ok(Json(entries))
}

#[derive(Serialize)]
struct EntryDetail {
    entry: Entry,
    children: Vec<Entry>,
}

async fn api_entry_detail(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<EntryDetail>, AppError> {
    let detail = tokio::task::spawn_blocking(move || -> Result<EntryDetail> {
        let db = state.db.lock().unwrap();
        let entry = db
            .find_entry_by_id(id)?
            .ok_or_else(|| anyhow::anyhow!("not_found"))?;
        let children = db.list_children(id)?;
        Ok(EntryDetail { entry, children })
    })
    .await??;
    Ok(Json(detail))
}

#[derive(Debug, Deserialize)]
struct BrowseQuery {
    path: Option<String>,
    #[serde(default = "default_show_hidden")]
    hidden: bool,
}

fn default_show_hidden() -> bool {
    true
}

#[derive(Serialize)]
struct BrowseResponse {
    path: String,
    current: Option<Entry>,
    ancestors: Vec<Entry>,
    entries: Vec<BrowseItem>,
}

async fn api_browse(
    State(state): State<AppState>,
    Query(q): Query<BrowseQuery>,
) -> Result<Json<BrowseResponse>, AppError> {
    let raw = q.path.unwrap_or_default();
    let trimmed = raw.trim_end_matches('/');
    let path = if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    };
    let show_hidden = q.hidden;

    let resp = tokio::task::spawn_blocking(move || -> Result<BrowseResponse> {
        let db = state.db.lock().unwrap();

        let current = db.find_entry_by_path(std::path::Path::new(&path))?;
        let ancestors = db.list_path_ancestors(&path)?;
        let entries = read_fs_entries(&db, &path, show_hidden)?;

        Ok(BrowseResponse {
            path,
            current,
            ancestors,
            entries,
        })
    })
    .await??;
    Ok(Json(resp))
}

/// Walk the real directory and attach DB state to each child.
fn read_fs_entries(
    db: &Database,
    path_str: &str,
    show_hidden: bool,
) -> anyhow::Result<Vec<BrowseItem>> {
    let path = std::path::Path::new(path_str);
    let Ok(iter) = std::fs::read_dir(path) else {
        return Ok(Vec::new());
    };

    let mut items = Vec::new();
    for entry in iter.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !show_hidden && name.starts_with('.') {
            continue;
        }

        let full_path = entry.path();
        let full_path_str = full_path.to_string_lossy().to_string();

        let meta = std::fs::metadata(&full_path)
            .or_else(|_| entry.metadata())
            .ok();
        let Some(meta) = meta else { continue };
        let is_dir = meta.is_dir();
        let size = if meta.is_file() {
            Some(meta.len())
        } else {
            None
        };
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);

        let (state, entry_opt, unknown) = if is_dir {
            if let Some(e) = db.find_entry_by_path(&full_path)? {
                (EntryState::Indexed, Some(e), None)
            } else if let Some(u) = db.find_unknown_by_path(&full_path_str)? {
                (EntryState::Unknown, None, Some(u))
            } else {
                (EntryState::Unscanned, None, None)
            }
        } else if let Some(e) = db.find_entry_by_path(&full_path)? {
            // Indexed file item — show with its base_type + tags.
            (EntryState::Indexed, Some(e), None)
        } else {
            (EntryState::File, None, None)
        };

        items.push(BrowseItem {
            name,
            path: full_path_str,
            is_dir,
            state,
            size,
            mtime,
            entry: entry_opt,
            unknown,
        });
    }

    items.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.starts_with('.').cmp(&b.name.starts_with('.')))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(items)
}

// ---------- Unknowns ----------

async fn api_unknowns(State(state): State<AppState>) -> Result<Json<Vec<Unknown>>, AppError> {
    let unknowns = tokio::task::spawn_blocking(move || {
        let db = state.db.lock().unwrap();
        db.list_unknowns()
    })
    .await??;
    Ok(Json(unknowns))
}

#[derive(Debug, Deserialize)]
struct ClassifyBody {
    base_type: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    privacy: Option<String>,
    /// Optionally mark the classified entry as an item rather than a collection.
    #[serde(default)]
    is_item: bool,
}

async fn api_classify_unknown(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<ClassifyBody>,
) -> Result<Json<Entry>, AppError> {
    let entry = tokio::task::spawn_blocking(move || -> anyhow::Result<Entry> {
        let mut db = state.db.lock().unwrap();
        let unknown = db
            .find_unknown_by_id(id)?
            .ok_or_else(|| anyhow::anyhow!("not_found"))?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let path_buf = std::path::PathBuf::from(&unknown.path);
        let name = path_buf
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| unknown.path.clone());

        let privacy = body
            .privacy
            .as_deref()
            .map(PrivacyLevel::from_str)
            .unwrap_or_default();

        let entry = Entry {
            id: 0,
            parent_id: None,
            location_id: unknown.location_id,
            path: unknown.path.clone(),
            name,
            base_type: BaseType::from_str(&body.base_type),
            is_item: body.is_item,
            is_dir: true,
            tags: body.tags.iter().map(|t| Tag::parse(t)).collect(),
            privacy,
            identifier: None,
            total_size: unknown.total_size,
            file_count: unknown.file_count,
            child_count: 0,
            manifest_hash: None,
            indexed_at: now,
        };
        let new_id = db.upsert_entry(&entry)?;
        db.remove_unknown_by_id(unknown.id)?;

        // If the user said this is a collection, follow up with a scan of
        // the path so its children get indexed. Items are atomic so a scan
        // wouldn't produce anything new.
        if !body.is_item {
            let _ = crate::scanner::scan_with(
                &mut db,
                &path_buf,
                false,
                crate::scanner::ScanOptions::default(),
            );
        }

        db.find_entry_by_id(new_id)?
            .ok_or_else(|| anyhow::anyhow!("post-insert entry missing"))
    })
    .await??;

    Ok(Json(entry))
}

#[derive(Debug, Deserialize)]
struct BulkClassifyBody {
    parent_path: String,
    base_type: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    privacy: Option<String>,
    #[serde(default)]
    is_item: bool,
}

#[derive(Debug, Serialize)]
struct BulkClassifyResult {
    classified: u64,
}

async fn api_bulk_classify(
    State(state): State<AppState>,
    Json(body): Json<BulkClassifyBody>,
) -> Result<Json<BulkClassifyResult>, AppError> {
    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<BulkClassifyResult> {
        let db = state.db.lock().unwrap();
        let unknowns = db.list_unknowns_with_parent(&body.parent_path)?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let base = BaseType::from_str(&body.base_type);
        let privacy = body
            .privacy
            .as_deref()
            .map(PrivacyLevel::from_str)
            .unwrap_or_default();
        let tags: Vec<Tag> = body.tags.iter().map(|t| Tag::parse(t)).collect();

        let mut classified = 0u64;
        for u in unknowns {
            let path_buf = std::path::PathBuf::from(&u.path);
            let name = path_buf
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| u.path.clone());

            let entry = Entry {
                id: 0,
                parent_id: None,
                location_id: u.location_id,
                path: u.path.clone(),
                name,
                base_type: base,
                is_item: body.is_item,
                is_dir: true,
                tags: tags.clone(),
                privacy,
                identifier: None,
                total_size: u.total_size,
                file_count: u.file_count,
                child_count: 0,
                manifest_hash: None,
                indexed_at: now,
            };
            db.upsert_entry(&entry)?;
            db.remove_unknown_by_id(u.id)?;
            classified += 1;
        }

        Ok(BulkClassifyResult { classified })
    })
    .await??;

    Ok(Json(result))
}

// ---------- Drives ----------

async fn api_drives(State(state): State<AppState>) -> Result<Json<Vec<Drive>>, AppError> {
    let drives = tokio::task::spawn_blocking(move || {
        let db = state.db.lock().unwrap();
        db.list_drives()
    })
    .await??;
    Ok(Json(drives))
}

#[derive(Debug, Deserialize)]
struct RenameDriveBody {
    friendly_name: Option<String>,
}

async fn api_rename_drive(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<RenameDriveBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    tokio::task::spawn_blocking(move || {
        let db = state.db.lock().unwrap();
        let name = body
            .friendly_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        db.rename_drive(id, name)
    })
    .await??;
    Ok(Json(serde_json::json!({"ok": true})))
}

// ---------- Scan ----------

#[derive(Debug, Deserialize)]
struct ScanBody {
    path: String,
    #[serde(default)]
    max_depth: Option<u32>,
    #[serde(default)]
    index_files: bool,
}

async fn api_scan(
    State(state): State<AppState>,
    Json(body): Json<ScanBody>,
) -> Result<Json<crate::scanner::ScanSummary>, AppError> {
    let summary =
        tokio::task::spawn_blocking(move || -> anyhow::Result<crate::scanner::ScanSummary> {
            let mut db = state.db.lock().unwrap();
            let path = std::path::PathBuf::from(&body.path);
            let opts = crate::scanner::ScanOptions {
                max_depth: body.max_depth,
                index_files: body.index_files,
            };
            crate::scanner::scan_with(&mut db, &path, false, opts)
        })
        .await??;
    Ok(Json(summary))
}

// ---------- Static assets ----------

async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data.into_owned()))
                .unwrap()
        }
        None => match Assets::get("index.html") {
            Some(index) => Response::builder()
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .body(Body::from(index.data.into_owned()))
                .unwrap(),
            None => (StatusCode::NOT_FOUND, "Not found").into_response(),
        },
    }
}

// ---------- Errors ----------

struct AppError {
    status: StatusCode,
    message: String,
}

impl AppError {
    fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: msg.into(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let body = Json(serde_json::json!({ "error": self.message }));
        (self.status, body).into_response()
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        let msg = err.to_string();
        let status = if msg == "not_found" {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        Self {
            status,
            message: msg,
        }
    }
}

impl From<tokio::task::JoinError> for AppError {
    fn from(err: tokio::task::JoinError) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: err.to_string(),
        }
    }
}
