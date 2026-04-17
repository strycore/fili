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

use crate::db::{BrowseEntry, CollectionFilter, Database};
use crate::models::{BaseType, Collection, File, Location, PrivacyLevel, Stats, Tag, Unknown};

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
        .route("/api/collections", get(api_collections))
        .route("/api/collections/:id", get(api_collection_detail))
        .route("/api/browse", get(api_browse))
        .route("/api/unknowns", get(api_unknowns))
        .route("/api/unknowns/:id/classify", post(api_classify_unknown))
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

// ---------- API handlers ----------

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
struct CollectionsQuery {
    #[serde(rename = "type")]
    base_type: Option<String>,
    privacy: Option<String>,
    parent: Option<String>, // "null" | "<id>" | None
    q: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn api_collections(
    State(state): State<AppState>,
    Query(q): Query<CollectionsQuery>,
) -> Result<Json<Vec<Collection>>, AppError> {
    let parent_id = match q.parent.as_deref() {
        Some("null") | Some("root") => Some(None),
        Some(other) => Some(Some(other.parse::<i64>().map_err(|_| {
            AppError::bad_request("invalid parent id")
        })?)),
        None => None,
    };

    let filter = CollectionFilter {
        base_type: q.base_type,
        privacy: q.privacy,
        parent_id,
        query: q.q,
        limit: q.limit,
        offset: q.offset,
    };

    let collections = tokio::task::spawn_blocking(move || {
        let db = state.db.lock().unwrap();
        db.list_collections(&filter)
    })
    .await??;
    Ok(Json(collections))
}

#[derive(Serialize)]
struct CollectionDetail {
    collection: Collection,
    children: Vec<Collection>,
    files: Vec<File>,
}

async fn api_collection_detail(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<CollectionDetail>, AppError> {
    let detail = tokio::task::spawn_blocking(move || -> Result<CollectionDetail> {
        let db = state.db.lock().unwrap();
        let collection = db
            .find_collection_by_id(id)?
            .ok_or_else(|| anyhow::anyhow!("not_found"))?;
        let children = db.list_children(id)?;
        let files = db.list_files_in_collection(id, 500)?;
        Ok(CollectionDetail {
            collection,
            children,
            files,
        })
    })
    .await??;
    Ok(Json(detail))
}

#[derive(Debug, Deserialize)]
struct BrowseQuery {
    path: Option<String>,
}

#[derive(Serialize)]
struct BrowseResponse {
    path: Option<String>,
    current: Option<Collection>,
    ancestors: Vec<Collection>,
    children: Vec<BrowseEntry>,
    files: Vec<File>,
}

async fn api_browse(
    State(state): State<AppState>,
    Query(q): Query<BrowseQuery>,
) -> Result<Json<BrowseResponse>, AppError> {
    let path = q.path.map(|p| p.trim_end_matches('/').to_string());
    let path_for_task = path.clone();

    let resp = tokio::task::spawn_blocking(move || -> Result<BrowseResponse> {
        let db = state.db.lock().unwrap();

        match path_for_task.as_deref() {
            None | Some("") => Ok(BrowseResponse {
                path: None,
                current: None,
                ancestors: Vec::new(),
                children: db.list_root_entries()?,
                files: Vec::new(),
            }),
            Some(p) => {
                let current = db.find_collection_by_path(std::path::Path::new(p))?;
                let ancestors = db.list_path_ancestors(p)?;
                let children = db.list_direct_path_children(p)?;
                let files = match &current {
                    Some(c) => db.list_files_in_collection(c.id, 500)?,
                    None => Vec::new(),
                };
                Ok(BrowseResponse {
                    path: Some(p.to_string()),
                    current,
                    ancestors,
                    children,
                    files,
                })
            }
        }
    })
    .await??;
    Ok(Json(resp))
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
}

async fn api_classify_unknown(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<ClassifyBody>,
) -> Result<Json<Collection>, AppError> {
    let collection = tokio::task::spawn_blocking(move || -> anyhow::Result<Collection> {
        let db = state.db.lock().unwrap();
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

        let collection = Collection {
            id: 0,
            parent_id: None,
            location_id: unknown.location_id,
            path: unknown.path.clone(),
            name,
            base_type: BaseType::from_str(&body.base_type),
            tags: body.tags.iter().map(|t| Tag::parse(t)).collect(),
            privacy,
            identifier: None,
            total_size: unknown.total_size,
            file_count: unknown.file_count,
            child_count: 0,
            manifest_hash: None,
            indexed_at: now,
        };
        let new_id = db.upsert_collection(&collection)?;
        db.remove_unknown_by_id(unknown.id)?;

        db.find_collection_by_id(new_id)?
            .ok_or_else(|| anyhow::anyhow!("post-insert collection missing"))
    })
    .await??;

    Ok(Json(collection))
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
