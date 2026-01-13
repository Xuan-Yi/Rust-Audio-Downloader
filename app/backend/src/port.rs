use std::path::{Path, PathBuf};
use std::{fs, str};

use anyhow::{anyhow, Context, Result};
use calamine::{open_workbook_auto, Data, Reader};
use rust_xlsxwriter::{Workbook, XlsxError};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct MusicRow {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub youtube_url: String,
}

#[derive(Clone, Debug)]
pub struct VersionInfo {
    pub current: String,
    pub latest: Option<String>,
    pub is_latest: Option<bool>,
    pub consistency: Option<String>,
    pub release_url: Option<String>,
}

pub fn get_version_info(
    client: &reqwest::blocking::Client,
    project_root: &Path,
) -> Result<VersionInfo> {
    let backend_path = project_root.join("app").join("backend").join("Cargo.toml");
    let root_path = project_root.join("package.json");
    let frontend_path = project_root
        .join("app")
        .join("frontend")
        .join("package.json");

    let backend_raw = read_cargo_version(&backend_path)?;
    let root_raw = read_package_version(&root_path)?;
    let frontend_raw = read_package_version(&frontend_path)?;

    let backend_version = backend_raw.as_deref().map(normalize_version);
    let root_version = root_raw.as_deref().map(normalize_version);
    let frontend_version = frontend_raw.as_deref().map(normalize_version);

    let current = backend_version
        .as_deref()
        .map(|value| format!("v{value}"))
        .unwrap_or_else(|| "v0.0.0".to_string());

    let all_match = match (&backend_version, &root_version, &frontend_version) {
        (Some(a), Some(b), Some(c)) => a == b && b == c,
        _ => false,
    };

    let (remote_backend, remote_root, remote_frontend, remote_present) =
        read_remote_versions(client);
    let remote_backend = remote_backend.as_deref().map(normalize_version);
    let remote_root = remote_root.as_deref().map(normalize_version);
    let remote_frontend = remote_frontend.as_deref().map(normalize_version);
    let remote_all_match = match (&remote_backend, &remote_root, &remote_frontend) {
        (Some(a), Some(b), Some(c)) => a == b && b == c,
        _ => false,
    };

    let latest = if remote_all_match {
        remote_backend
            .as_deref()
            .map(|value| format!("v{value}"))
            .or_else(|| Some("v0.0.0".to_string()))
    } else {
        Some("v0.0.0".to_string())
    };

    let consistency = build_consistency_message(
        &backend_version,
        &root_version,
        &frontend_version,
        &remote_backend,
        &remote_root,
        &remote_frontend,
        remote_present,
    );

    Ok(VersionInfo {
        current,
        latest,
        is_latest: Some(all_match),
        consistency,
        release_url: None,
    })
}

fn normalize_version(raw: &str) -> String {
    raw.trim().trim_start_matches('v').to_string()
}

fn build_consistency_message(
    backend: &Option<String>,
    root: &Option<String>,
    frontend: &Option<String>,
    remote_backend: &Option<String>,
    remote_root: &Option<String>,
    remote_frontend: &Option<String>,
    remote_present: bool,
) -> Option<String> {
    let local_match = match (backend, root, frontend) {
        (Some(a), Some(b), Some(c)) => a == b && b == c,
        _ => false,
    };
    if !local_match {
        return Some(format!(
            "Local mismatch: backend {}, root {}, frontend {}",
            version_label(backend),
            version_label(root),
            version_label(frontend)
        ));
    }

    if !remote_present {
        return None;
    }

    let remote_match = match (remote_backend, remote_root, remote_frontend) {
        (Some(a), Some(b), Some(c)) => a == b && b == c,
        _ => false,
    };
    if !remote_match {
        return Some(format!(
            "Remote mismatch: backend {}, root {}, frontend {}",
            version_label(remote_backend),
            version_label(remote_root),
            version_label(remote_frontend)
        ));
    }

    match (backend, remote_backend) {
        (Some(local), Some(remote)) if local != remote => {
            Some(format!("Local v{} differs from remote v{}", local, remote))
        }
        _ => None,
    }
}

fn version_label(value: &Option<String>) -> String {
    value
        .as_deref()
        .map(|version| format!("v{version}"))
        .unwrap_or_else(|| "missing".to_string())
}

fn read_cargo_version(path: &Path) -> Result<Option<String>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let mut in_package = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_package = trimmed == "[package]";
            continue;
        }
        if in_package && trimmed.starts_with("version") {
            let value = trimmed
                .splitn(2, '=')
                .nth(1)
                .map(str::trim)
                .and_then(|value| value.strip_prefix('"').and_then(|v| v.strip_suffix('"')))
                .map(|value| value.to_string());
            if value.is_some() {
                return Ok(value);
            }
        }
    }
    Ok(None)
}

fn read_package_version(path: &Path) -> Result<Option<String>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let json: serde_json::Value = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(json
        .get("version")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string()))
}

fn read_remote_versions(
    client: &reqwest::blocking::Client,
) -> (Option<String>, Option<String>, Option<String>, bool) {
    let refs = ["main", "master"];
    for reference in refs {
        let base = format!(
            "https://raw.githubusercontent.com/Xuan-Yi/Rust-Audio-Downloader/{reference}"
        );
        let backend = fetch_remote_version(
            client,
            &format!("{base}/app/backend/Cargo.toml"),
            true,
        );
        let root = fetch_remote_version(client, &format!("{base}/package.json"), false);
        let frontend = fetch_remote_version(
            client,
            &format!("{base}/app/frontend/package.json"),
            false,
        );
        if backend.is_some() || root.is_some() || frontend.is_some() {
            let all_present = backend.is_some() && root.is_some() && frontend.is_some();
            return (backend, root, frontend, all_present);
        }
    }
    (None, None, None, false)
}

fn fetch_remote_version(
    client: &reqwest::blocking::Client,
    url: &str,
    is_cargo: bool,
) -> Option<String> {
    let response = client.get(url).send().ok()?;
    if !response.status().is_success() {
        return None;
    }
    let content = response.text().ok()?;
    if is_cargo {
        read_cargo_version_from_str(&content)
    } else {
        read_package_version_from_str(&content)
    }
}

fn read_cargo_version_from_str(content: &str) -> Option<String> {
    let mut in_package = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_package = trimmed == "[package]";
            continue;
        }
        if in_package && trimmed.starts_with("version") {
            let value = trimmed
                .splitn(2, '=')
                .nth(1)
                .map(str::trim)
                .and_then(|value| value.strip_prefix('"').and_then(|v| v.strip_suffix('"')))
                .map(|value| value.to_string());
            if value.is_some() {
                return value;
            }
        }
    }
    None
}

fn read_package_version_from_str(content: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(content).ok()?;
    json.get("version")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}


pub fn import_music_list(path: &Path) -> Result<Vec<MusicRow>> {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "csv" => import_csv(path),
        "xlsx" => import_xlsx(path),
        other => Err(anyhow!("unsupported import format: {other}")),
    }
}

pub fn export_music_list(path: &Path, rows: &[MusicRow]) -> Result<()> {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "csv" => export_csv(path, rows),
        "xlsx" => export_xlsx(path, rows),
        other => Err(anyhow!("unsupported export format: {other}")),
    }
}

pub fn create_sample_xlsx(dir: &Path) -> Result<PathBuf> {
    let file_path = dir.join(format!("Sample-{}.xlsx", Uuid::new_v4()));
    let rows = vec![MusicRow {
        title: Some("Example Title".to_string()),
        artist: Some("Example Artist".to_string()),
        youtube_url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
    }];
    export_xlsx(&file_path, &rows)?;
    Ok(file_path)
}

fn import_csv(path: &Path) -> Result<Vec<MusicRow>> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_path(path)
        .with_context(|| format!("failed to open csv: {}", path.display()))?;

    let mut records = reader.records();
    let Some(first) = records.next().transpose()? else {
        return Ok(Vec::new());
    };

    let (header_map, mut rows) = if looks_like_header(&first) {
        (HeaderMap::from_header(&first), Vec::new())
    } else {
        (HeaderMap::default(), Vec::new())
    };

    if !header_map.has_header {
        if let Some(row) = row_from_record(&first, &header_map) {
            rows.push(row);
        }
    }

    for record in records {
        let record = record?;
        if let Some(row) = row_from_record(&record, &header_map) {
            rows.push(row);
        }
    }

    Ok(rows)
}

fn import_xlsx(path: &Path) -> Result<Vec<MusicRow>> {
    let mut workbook = open_workbook_auto(path)
        .with_context(|| format!("failed to open xlsx: {}", path.display()))?;
    let sheet_name = workbook
        .sheet_names()
        .get(0)
        .cloned()
        .ok_or_else(|| anyhow!("xlsx contains no sheets"))?;

    let range = workbook
        .worksheet_range(&sheet_name)
        .with_context(|| format!("failed to read sheet: {sheet_name}"))?;

    let mut rows_iter = range.rows();
    let Some(first) = rows_iter.next() else {
        return Ok(Vec::new());
    };

    let first_strings: Vec<String> = first.iter().map(cell_to_string).collect();
    let (header_map, mut rows) = if looks_like_header_strings(&first_strings) {
        (HeaderMap::from_strings(&first_strings), Vec::new())
    } else {
        (HeaderMap::default(), Vec::new())
    };

    if !header_map.has_header {
        if let Some(row) = row_from_cells(first, &header_map) {
            rows.push(row);
        }
    }

    for row in rows_iter {
        if let Some(row) = row_from_cells(row, &header_map) {
            rows.push(row);
        }
    }

    Ok(rows)
}

fn export_csv(path: &Path, rows: &[MusicRow]) -> Result<()> {
    let mut writer = csv::WriterBuilder::new()
        .from_path(path)
        .with_context(|| format!("failed to create csv: {}", path.display()))?;

    writer.write_record(["Title", "Artist", "YouTube URL"])?;
    for row in rows {
        writer.write_record([
            row.title.clone().unwrap_or_default(),
            row.artist.clone().unwrap_or_default(),
            row.youtube_url.clone(),
        ])?;
    }
    writer.flush()?;
    Ok(())
}

fn export_xlsx(path: &Path, rows: &[MusicRow]) -> Result<()> {
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();

    worksheet.write_string(0, 0, "Title")?;
    worksheet.write_string(0, 1, "Artist")?;
    worksheet.write_string(0, 2, "YouTube URL")?;

    for (index, row) in rows.iter().enumerate() {
        let row_index = (index + 1) as u32;
        worksheet.write_string(row_index, 0, row.title.as_deref().unwrap_or(""))?;
        worksheet.write_string(row_index, 1, row.artist.as_deref().unwrap_or(""))?;
        worksheet.write_string(row_index, 2, &row.youtube_url)?;
    }

    workbook.save(path).map_err(map_xlsx_error)?;
    Ok(())
}

fn map_xlsx_error(err: XlsxError) -> anyhow::Error {
    anyhow!(err.to_string())
}

#[derive(Clone, Copy)]
struct HeaderMap {
    title: usize,
    artist: usize,
    url: usize,
    has_header: bool,
}

impl HeaderMap {
    fn default() -> Self {
        Self {
            title: 0,
            artist: 1,
            url: 2,
            has_header: false,
        }
    }

    fn from_header(record: &csv::StringRecord) -> Self {
        let strings: Vec<String> = record.iter().map(|value| value.to_string()).collect();
        Self::from_strings(&strings)
    }

    fn from_strings(values: &[String]) -> Self {
        let mut map = Self::default();
        map.has_header = true;
        for (idx, value) in values.iter().enumerate() {
            let normalized = value.to_lowercase();
            if normalized.contains("title") {
                map.title = idx;
            } else if normalized.contains("artist") {
                map.artist = idx;
            } else if normalized.contains("url") {
                map.url = idx;
            }
        }
        map
    }
}

fn looks_like_header(record: &csv::StringRecord) -> bool {
    let strings: Vec<String> = record.iter().map(|value| value.to_string()).collect();
    looks_like_header_strings(&strings)
}

fn looks_like_header_strings(values: &[String]) -> bool {
    values.iter().any(|value| {
        let normalized = value.to_lowercase();
        normalized.contains("url") || normalized.contains("title") || normalized.contains("artist")
    })
}

fn row_from_record(record: &csv::StringRecord, map: &HeaderMap) -> Option<MusicRow> {
    let url = record.get(map.url)?.trim().to_string();
    if url.is_empty() {
        return None;
    }
    let title = record.get(map.title).map(|value| value.trim().to_string());
    let artist = record.get(map.artist).map(|value| value.trim().to_string());
    Some(MusicRow {
        title: title.filter(|value| !value.is_empty()),
        artist: artist.filter(|value| !value.is_empty()),
        youtube_url: url,
    })
}

fn row_from_cells(cells: &[Data], map: &HeaderMap) -> Option<MusicRow> {
    let url = cells.get(map.url).map(cell_to_string)?.trim().to_string();
    if url.is_empty() {
        return None;
    }
    let title = cells.get(map.title).map(cell_to_string);
    let artist = cells.get(map.artist).map(cell_to_string);
    Some(MusicRow {
        title: title.filter(|value| !value.trim().is_empty()),
        artist: artist.filter(|value| !value.trim().is_empty()),
        youtube_url: url,
    })
}

fn cell_to_string(cell: &Data) -> String {
    match cell {
        Data::String(value) => value.clone(),
        Data::Float(value) => value.to_string(),
        Data::Int(value) => value.to_string(),
        Data::Bool(value) => value.to_string(),
        Data::DateTime(value) => value.to_string(),
        Data::DateTimeIso(value) => value.clone(),
        Data::DurationIso(value) => value.clone(),
        Data::Error(value) => format!("{value:?}"),
        Data::Empty => String::new(),
    }
}
