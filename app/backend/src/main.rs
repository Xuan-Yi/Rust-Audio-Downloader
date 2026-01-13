use std::path::PathBuf;

use anyhow::Result;
use axum::routing::{delete, get, post};
use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing::info;

mod errors;
mod handlers;
mod media;
mod port;
mod types;

use types::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let project_root = resolve_project_root();
    let preview_dir = project_root.join("app").join("preview_cache");
    let temp_dir = project_root.join("app").join("tmp");
    tokio::fs::create_dir_all(&preview_dir).await?;
    tokio::fs::create_dir_all(&temp_dir).await?;

    let state = AppState {
        queue: std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new())),
        preview_dir: preview_dir.clone(),
        temp_dir: temp_dir.clone(),
        download_semaphore: std::sync::Arc::new(tokio::sync::Semaphore::new(6)),
        client: reqwest::Client::new(),
        project_root,
    };

    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any);

    let app = Router::new()
        .route("/api/version", get(handlers::version_info))
        .route("/api/default-dir", get(handlers::default_dir))
        .route("/api/select-dir", get(handlers::select_dir))
        .route("/api/queue", get(handlers::list_queue))
        .route("/api/queue/add", post(handlers::add_queue))
        .route("/api/queue/update", post(handlers::update_queue))
        .route("/api/queue/clear", post(handlers::clear_queue))
        .route("/api/queue/:id", delete(handlers::delete_queue))
        .route("/api/download", post(handlers::download_all))
        .route("/api/import", post(handlers::import_list))
        .route("/api/export", post(handlers::export_list))
        .route("/api/sample", get(handlers::sample_file))
        .route("/api/preview/:id", get(handlers::ensure_preview))
        .nest_service("/preview", ServeDir::new(preview_dir))
        .layer(cors)
        .with_state(state);

    let address = "127.0.0.1:47815";
    info!("listening on http://{address}");
    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn resolve_project_root() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if cwd.ends_with("backend") {
        return cwd.join("..").join("..");
    }
    cwd
}
