use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{anyhow, Context, Result};
use axum::extract::{Multipart, Path as AxumPath, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use dirs::download_dir;
use mime_guess::MimeGuess;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio_util::io::ReaderStream;
use tracing::error;

use crate::errors::AppError;
use crate::media::{
    apply_yt_dlp_common_args, download_preview, fetch_thumbnail, fetch_video_info,
    find_downloaded_file, find_preview_file, parse_yt_dlp_progress, sanitize_text, tag_audio,
};
use crate::port::{create_sample_xlsx, export_music_list, get_version_info, import_music_list, MusicRow};
use crate::types::{
    AddRequest, AppState, ClearRequest, DefaultDirResponse, DownloadRequest, DownloadResponse,
    DownloadState, ExportRequest, PreviewResponse, QueueItem, UpdateRequest, VersionResponse,
};

pub async fn version_info(State(state): State<AppState>) -> Result<Json<VersionResponse>, AppError> {
    let project_root = state.project_root.clone();
    let info = tokio::task::spawn_blocking(move || {
        let client = reqwest::blocking::Client::new();
        get_version_info(&client, &project_root)
    })
    .await
    .map_err(|err| AppError::internal(err.to_string()))?
    .map_err(|err| AppError::internal(err.to_string()))?;

    Ok(Json(VersionResponse {
        current: info.current,
        latest: info.latest,
        is_latest: info.is_latest,
        consistency: info.consistency,
        release_url: info.release_url,
    }))
}

pub async fn default_dir() -> Json<DefaultDirResponse> {
    let path = download_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .to_string_lossy()
        .to_string();
    Json(DefaultDirResponse { path })
}

pub async fn select_dir() -> Result<Json<DefaultDirResponse>, AppError> {
    let picked = tokio::task::spawn_blocking(|| rfd::FileDialog::new().pick_folder())
        .await
        .map_err(|err| AppError::internal(err.to_string()))?;
    let path = picked
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_default();
    Ok(Json(DefaultDirResponse { path }))
}

pub async fn list_queue(State(state): State<AppState>) -> Json<Vec<QueueItem>> {
    let queue = state.queue.lock().await;
    Json(queue.clone())
}

pub async fn add_queue(
    State(state): State<AppState>,
    Json(req): Json<AddRequest>,
) -> Result<Json<QueueItem>, AppError> {
    let info = fetch_video_info(&req.url).await?;
    let title = sanitize_text(&info.title);
    let artist = sanitize_text(&info.artist);

    let item = QueueItem {
        id: info.id.clone(),
        youtube_url: req.url,
        title: if title.is_empty() { "Unknown".to_string() } else { title },
        artist: if artist.is_empty() { "Unknown".to_string() } else { artist },
        thumbnail_url: info.thumbnail_url,
        duration: info.duration,
        state: DownloadState::Waiting,
        progress: None,
        error: None,
    };

    let mut queue = state.queue.lock().await;
    if queue.iter().any(|existing| existing.id == item.id) {
        return Err(AppError::conflict("queue already contains this video"));
    }
    queue.push(item.clone());
    Ok(Json(item))
}

pub async fn update_queue(
    State(state): State<AppState>,
    Json(req): Json<UpdateRequest>,
) -> Result<Json<QueueItem>, AppError> {
    let mut queue = state.queue.lock().await;
    let Some(item) = queue.iter_mut().find(|item| item.id == req.id) else {
        return Err(AppError::not_found("queue item not found"));
    };

    if let Some(title) = req.title {
        let clean = sanitize_text(&title);
        if !clean.is_empty() {
            item.title = clean;
        }
    }
    if let Some(artist) = req.artist {
        let clean = sanitize_text(&artist);
        if !clean.is_empty() {
            item.artist = clean;
        }
    }

    Ok(Json(item.clone()))
}

pub async fn delete_queue(
    AxumPath(id): AxumPath<String>,
    State(state): State<AppState>,
) -> Result<StatusCode, AppError> {
    let mut queue = state.queue.lock().await;
    let before = queue.len();
    queue.retain(|item| item.id != id);
    if queue.len() == before {
        return Err(AppError::not_found("queue item not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn clear_queue(
    State(state): State<AppState>,
    Json(req): Json<ClearRequest>,
) -> Result<Json<Vec<QueueItem>>, AppError> {
    let mut queue = state.queue.lock().await;
    match req.mode.as_str() {
        "complete" => queue.retain(|item| item.state != DownloadState::Complete),
        "failed" => queue.retain(|item| item.state != DownloadState::Failed),
        "all" => queue.retain(|item| item.state == DownloadState::Working),
        "non_working" => queue.retain(|item| item.state == DownloadState::Working),
        _ => return Err(AppError::bad_request("unknown clear mode")),
    }
    Ok(Json(queue.clone()))
}

pub async fn download_all(
    State(state): State<AppState>,
    Json(req): Json<DownloadRequest>,
) -> Result<Json<DownloadResponse>, AppError> {
    let format = normalize_format(&req.format)?;
    let dir = download_dir().unwrap_or_else(|| PathBuf::from("."));
    tokio::fs::create_dir_all(&dir).await.map_err(|err| {
        AppError::bad_request(format!("failed to create output directory: {err}"))
    })?;

    let ids: Vec<String> = {
        let queue = state.queue.lock().await;
        queue
            .iter()
            .filter(|item| {
                matches!(
                    item.state,
                    DownloadState::Waiting | DownloadState::Complete | DownloadState::Failed
                )
            })
            .map(|item| item.id.clone())
            .collect()
    };

    let started = ids.len();
    let state_clone = state.clone();

    tokio::spawn(async move {
        for id in ids {
            let permit = state_clone.download_semaphore.clone().acquire_owned().await;
            if permit.is_err() {
                break;
            }
            let state = state_clone.clone();
            let dir = dir.clone();
            let format = format.to_string();
            tokio::spawn(async move {
                let _permit = permit;
                if let Err(err) = handle_download_item(state, &id, &dir, &format).await {
                    error!("download failed for {id}: {err}");
                }
            });
        }
    });

    Ok(Json(DownloadResponse { started }))
}

async fn handle_download_item(
    state: AppState,
    id: &str,
    dir: &Path,
    format: &str,
) -> Result<()> {
    let item = {
        let mut queue = state.queue.lock().await;
        let Some(item) = queue.iter_mut().find(|item| item.id == id) else {
            return Ok(());
        };
        item.state = DownloadState::Working;
        item.error = None;
        item.progress = Some(0.0);
        item.clone()
    };

    let thumbnail_data = if let Some(url) = item.thumbnail_url.as_deref() {
        match fetch_thumbnail(&state.client, url).await {
            Ok(bytes) => Some(bytes),
            Err(err) => {
                error!("thumbnail fetch failed: {err}");
                None
            }
        }
    } else {
        None
    };

    let result = download_audio(&state, id, &item.youtube_url, &item.title, format, dir).await;
    match result {
        Ok(path) => {
            if let Err(err) = tag_audio(&path, &item.artist, thumbnail_data) {
                error!("tagging failed for {id}: {err}");
            }
            update_item_state(&state, id, DownloadState::Complete, None).await;
        }
        Err(err) => {
            update_item_state(&state, id, DownloadState::Failed, Some(err.to_string())).await;
        }
    }
    Ok(())
}

async fn update_item_state(
    state: &AppState,
    id: &str,
    new_state: DownloadState,
    error: Option<String>,
) {
    let mut queue = state.queue.lock().await;
    if let Some(item) = queue.iter_mut().find(|item| item.id == id) {
        item.state = new_state;
        item.error = error;
        item.progress = match new_state {
            DownloadState::Complete => Some(100.0),
            DownloadState::Working => item.progress.or(Some(0.0)),
            _ => None,
        };
    }
}

async fn update_item_progress(state: &AppState, id: &str, progress: f32) {
    let mut queue = state.queue.lock().await;
    if let Some(item) = queue.iter_mut().find(|item| item.id == id) {
        item.progress = Some(progress.clamp(0.0, 100.0));
    }
}

pub async fn import_list(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<Vec<QueueItem>>, AppError> {
    let mut saved_path = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| AppError::bad_request(err.to_string()))?
    {
        let file_name = field
            .file_name()
            .map(|name| name.to_string())
            .unwrap_or_else(|| "upload.bin".to_string());
        let data = field
            .bytes()
            .await
            .map_err(|err| AppError::bad_request(err.to_string()))?;
        let file_path = state
            .temp_dir
            .join(format!("{}-{}", uuid::Uuid::new_v4(), file_name));
        tokio::fs::write(&file_path, data)
            .await
            .map_err(|err| AppError::bad_request(err.to_string()))?;
        saved_path = Some(file_path);
        break;
    }

    let Some(file_path) = saved_path else {
        return Err(AppError::bad_request("no file uploaded"));
    };

    let rows = tokio::task::spawn_blocking({
        let file_path = file_path.clone();
        move || import_music_list(&file_path)
    })
    .await
    .map_err(|err| AppError::internal(err.to_string()))?
    .map_err(|err| AppError::bad_request(err.to_string()))?;

    let mut new_items = Vec::new();
    for row in rows {
        match build_queue_item_from_row(&row).await {
            Ok(item) => new_items.push(item),
            Err(err) => error!("failed to import row: {err:?}"),
        }
    }

    let mut queue = state.queue.lock().await;
    for item in &new_items {
        if !queue.iter().any(|existing| existing.id == item.id) {
            queue.push(item.clone());
        }
    }

    Ok(Json(new_items))
}

pub async fn export_list(
    State(state): State<AppState>,
    Json(req): Json<ExportRequest>,
) -> Result<Response, AppError> {
    let format = normalize_export_format(&req.format)?;
    let rows = {
        let queue = state.queue.lock().await;
        queue
            .iter()
            .map(|item| MusicRow {
                title: Some(item.title.clone()),
                artist: Some(item.artist.clone()),
                youtube_url: item.youtube_url.clone(),
            })
            .collect::<Vec<_>>()
    };

    let file_name = format!("AudioDownloader_export.{format}");
    let file_path = state.temp_dir.join(format!("{}-{file_name}", uuid::Uuid::new_v4()));

    tokio::task::spawn_blocking({
        let file_path = file_path.clone();
        let rows = rows.clone();
        move || export_music_list(&file_path, &rows)
    })
    .await
    .map_err(|err| AppError::internal(err.to_string()))?
    .map_err(|err| AppError::internal(err.to_string()))?;

    stream_file(&file_path, &file_name).await
}

pub async fn sample_file(State(state): State<AppState>) -> Result<Response, AppError> {
    let created_path = tokio::task::spawn_blocking({
        let dir = state.temp_dir.clone();
        move || create_sample_xlsx(&dir)
    })
    .await
    .map_err(|err| AppError::internal(err.to_string()))?
    .map_err(|err| AppError::internal(err.to_string()))?;

    stream_file(&created_path, "Sample.xlsx").await
}

pub async fn ensure_preview(
    AxumPath(id): AxumPath<String>,
    State(state): State<AppState>,
) -> Result<Json<PreviewResponse>, AppError> {
    let item = {
        let queue = state.queue.lock().await;
        queue.iter().find(|item| item.id == id).cloned()
    };
    let Some(item) = item else {
        return Err(AppError::not_found("queue item not found"));
    };

    let existing = find_preview_file(&state.preview_dir, &item.id);
    let path = if let Some(path) = existing {
        path
    } else {
        download_preview(&item.youtube_url, &item.id, &state.preview_dir).await?
    };

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::internal("invalid preview file name"))?;

    Ok(Json(PreviewResponse {
        url: format!("/preview/{file_name}"),
    }))
}

async fn build_queue_item_from_row(row: &MusicRow) -> Result<QueueItem, AppError> {
    let info = fetch_video_info(&row.youtube_url).await?;
    let title = row.title.clone().unwrap_or_else(|| info.title.clone());
    let artist = row.artist.clone().unwrap_or_else(|| info.artist.clone());

    Ok(QueueItem {
        id: info.id,
        youtube_url: row.youtube_url.clone(),
        title: sanitize_text(&title),
        artist: sanitize_text(&artist),
        thumbnail_url: info.thumbnail_url,
        duration: info.duration,
        state: DownloadState::Waiting,
        progress: None,
        error: None,
    })
}

async fn download_audio(
    state: &AppState,
    id: &str,
    url: &str,
    title: &str,
    format: &str,
    dir: &Path,
) -> Result<PathBuf> {
    let clean_title = sanitize_text(title);
    let output_template = dir.join(format!("{clean_title}.%(ext)s"));
    let output_template = output_template
        .to_str()
        .ok_or_else(|| anyhow!("invalid output path"))?
        .to_string();

    let mut cmd = Command::new("yt-dlp");
    cmd.arg("-x")
        .arg("--audio-format")
        .arg(format)
        .arg("--audio-quality")
        .arg("0")
        .arg("--no-playlist")
        .arg("--progress")
        .arg("--newline")
        .arg("-o")
        .arg(output_template)
        .arg(url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    apply_yt_dlp_common_args(&mut cmd);
    let mut child = cmd.spawn().context("yt-dlp execution failed")?;

    let mut progress_tasks = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        let state = state.clone();
        let id = id.to_string();
        progress_tasks.push(tokio::spawn(async move {
            consume_progress(stdout, state, id).await;
        }));
    }
    if let Some(stderr) = child.stderr.take() {
        let state = state.clone();
        let id = id.to_string();
        progress_tasks.push(tokio::spawn(async move {
            consume_progress(stderr, state, id).await;
        }));
    }

    let status = child.wait().await.context("yt-dlp execution failed")?;
    for task in progress_tasks {
        let _ = task.await;
    }
    if !status.success() {
        return Err(anyhow!("yt-dlp download failed"));
    }

    let path = dir.join(format!("{clean_title}.{format}"));
    if path.exists() {
        return Ok(path);
    }

    find_downloaded_file(dir, &clean_title).ok_or_else(|| anyhow!("downloaded file not found"))
}

async fn consume_progress<R: AsyncRead + Unpin>(reader: R, state: AppState, id: String) {
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if let Some(progress) = parse_yt_dlp_progress(&line) {
            update_item_progress(&state, &id, progress).await;
        }
    }
}

fn normalize_format(format: &str) -> Result<&'static str, AppError> {
    match format.to_lowercase().as_str() {
        "flac" => Ok("flac"),
        "mp3" => Ok("mp3"),
        "m4a" => Ok("m4a"),
        "wav" => Ok("wav"),
        _ => Err(AppError::bad_request("unsupported format")),
    }
}

fn normalize_export_format(format: &str) -> Result<&'static str, AppError> {
    match format.to_lowercase().as_str() {
        "xlsx" => Ok("xlsx"),
        "csv" => Ok("csv"),
        _ => Err(AppError::bad_request("unsupported export format")),
    }
}

async fn stream_file(path: &Path, download_name: &str) -> Result<Response, AppError> {
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|err| AppError::internal(format!("failed to open export file: {err}")))?;
    let stream = ReaderStream::new(file);
    let body = axum::body::Body::from_stream(stream);

    let mut headers = HeaderMap::new();
    let mime = MimeGuess::from_path(path).first_or_octet_stream();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(mime.as_ref()).map_err(|err| AppError::internal(err.to_string()))?,
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{download_name}\""))
            .map_err(|err| AppError::internal(err.to_string()))?,
    );

    Ok((headers, body).into_response())
}
