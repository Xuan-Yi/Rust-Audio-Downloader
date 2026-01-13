use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, Semaphore};

#[derive(Clone)]
pub struct AppState {
    pub queue: Arc<Mutex<Vec<QueueItem>>>,
    pub preview_dir: PathBuf,
    pub temp_dir: PathBuf,
    pub download_semaphore: Arc<Semaphore>,
    pub client: reqwest::Client,
    pub project_root: PathBuf,
}

#[derive(Clone, Serialize)]
pub struct QueueItem {
    pub id: String,
    pub youtube_url: String,
    pub title: String,
    pub artist: String,
    pub thumbnail_url: Option<String>,
    pub duration: Option<u64>,
    pub state: DownloadState,
    pub progress: Option<f32>,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DownloadState {
    Waiting,
    Working,
    Complete,
    Failed,
}

#[derive(Deserialize)]
pub struct AddRequest {
    pub url: String,
}

#[derive(Deserialize)]
pub struct UpdateRequest {
    pub id: String,
    pub title: Option<String>,
    pub artist: Option<String>,
}

#[derive(Deserialize)]
pub struct ClearRequest {
    pub mode: String,
}

#[derive(Deserialize)]
pub struct DownloadRequest {
    pub format: String,
}

#[derive(Deserialize)]
pub struct ExportRequest {
    pub format: String,
}

#[derive(Serialize)]
pub struct DownloadResponse {
    pub started: usize,
}

#[derive(Serialize)]
pub struct DefaultDirResponse {
    pub path: String,
}

#[derive(Serialize)]
pub struct PreviewResponse {
    pub url: String,
}

#[derive(Serialize)]
pub struct VersionResponse {
    pub current: String,
    pub latest: Option<String>,
    pub is_latest: Option<bool>,
    pub consistency: Option<String>,
    pub release_url: Option<String>,
}

#[derive(Deserialize)]
pub struct YtDlpInfo {
    pub id: String,
    pub title: Option<String>,
    pub uploader: Option<String>,
    pub channel: Option<String>,
    pub thumbnail: Option<String>,
    pub thumbnails: Option<Vec<YtDlpThumb>>,
    pub duration: Option<f64>,
}

#[derive(Deserialize)]
pub struct YtDlpThumb {
    pub url: Option<String>,
}

#[derive(Clone)]
pub struct VideoInfo {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub thumbnail_url: Option<String>,
    pub duration: Option<u64>,
}
