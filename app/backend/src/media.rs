use std::env;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use lofty::{AudioFile, ItemKey, MimeType, Picture, PictureType, Tag, TagType, TaggedFileExt};
use sanitize_filename::sanitize;
use tokio::process::Command;

use crate::errors::AppError;
use crate::types::{VideoInfo, YtDlpInfo};

pub fn apply_yt_dlp_common_args(cmd: &mut Command) {
    cmd.arg("--extractor-args")
        .arg("youtube:player_client=default");

    if let Ok(cookies) = env::var("YTDLP_COOKIES") {
        let trimmed = cookies.trim();
        if !trimmed.is_empty() {
            cmd.arg("--cookies").arg(trimmed);
            return;
        }
    }

    if let Ok(browser) = env::var("YTDLP_COOKIES_FROM_BROWSER") {
        let trimmed = browser.trim();
        if !trimmed.is_empty() {
            cmd.arg("--cookies-from-browser").arg(trimmed);
        }
    }
}

pub async fn fetch_video_info(url: &str) -> Result<VideoInfo, AppError> {
    let mut cmd = Command::new("yt-dlp");
    cmd.arg("-J").arg("--no-playlist").arg(url);
    apply_yt_dlp_common_args(&mut cmd);
    let output = cmd.output().await
        .map_err(|err| AppError::bad_request(format!("yt-dlp not available: {err}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::bad_request(format!(
            "yt-dlp failed: {stderr}"
        )));
    }

    let info: YtDlpInfo = serde_json::from_slice(&output.stdout)
        .map_err(|err| AppError::internal(err.to_string()))?;

    let title = info.title.unwrap_or_else(|| "Unknown".to_string());
    let artist = info
        .uploader
        .or(info.channel)
        .unwrap_or_else(|| "Unknown".to_string());
    let thumbnail_url = info.thumbnail.or_else(|| {
        info.thumbnails
            .and_then(|mut thumbs| thumbs.pop())
            .and_then(|thumb| thumb.url)
    });
    let duration = info.duration.map(|value| value.round() as u64);

    Ok(VideoInfo {
        id: info.id,
        title,
        artist,
        thumbnail_url,
        duration,
    })
}

pub async fn download_preview(url: &str, id: &str, dir: &Path) -> Result<PathBuf, AppError> {
    let output_template = dir.join(format!("{id}.%(ext)s"));
    let output_template = output_template
        .to_str()
        .ok_or_else(|| AppError::internal("invalid preview output path"))?
        .to_string();

    let mut cmd = Command::new("yt-dlp");
    cmd.arg("-f")
        .arg("bestaudio")
        .arg("--no-playlist")
        .arg("-o")
        .arg(output_template)
        .arg(url);
    apply_yt_dlp_common_args(&mut cmd);
    let status = cmd.status().await
        .map_err(|err| AppError::bad_request(format!("yt-dlp not available: {err}")))?;

    if !status.success() {
        return Err(AppError::internal("yt-dlp preview download failed"));
    }

    find_preview_file(dir, id).ok_or_else(|| AppError::internal("preview file missing"))
}

pub async fn fetch_thumbnail(client: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let response = client.get(url).send().await?;
    let data = response.bytes().await?;
    Ok(data.to_vec())
}

pub fn tag_audio(path: &Path, artist: &str, thumbnail: Option<Vec<u8>>) -> Result<()> {
    let tag_type = match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "mp3" => TagType::Id3v2,
        "m4a" | "mp4" => TagType::Mp4Ilst,
        "flac" => TagType::VorbisComments,
        "wav" => TagType::Id3v2,
        _ => TagType::Id3v2,
    };

    let mut tagged_file = lofty::read_from_path(path)?;
    if tagged_file.primary_tag().is_none() {
        tagged_file.insert_tag(Tag::new(tag_type));
    }
    let tag = tagged_file
        .primary_tag_mut()
        .ok_or_else(|| anyhow!("unable to access tag"))?;

    tag.insert_text(ItemKey::TrackArtist, artist.to_string());
    tag.insert_text(ItemKey::AlbumArtist, artist.to_string());

    if let Some(bytes) = thumbnail {
        let mime = detect_mime(&bytes);
        let picture = Picture::new_unchecked(PictureType::CoverFront, Some(mime), None, bytes);
        tag.push_picture(picture);
    }

    tagged_file.save_to_path(path)?;
    Ok(())
}

pub fn detect_mime(bytes: &[u8]) -> MimeType {
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        MimeType::Png
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        MimeType::Jpeg
    } else {
        MimeType::Jpeg
    }
}

pub fn find_downloaded_file(dir: &Path, title: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = path.file_name()?.to_str()?;
        if file_name.starts_with(&format!("{title}.")) {
            return Some(path);
        }
    }
    None
}

pub fn find_preview_file(dir: &Path, id: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = path.file_name()?.to_str()?;
        if file_name.starts_with(&format!("{id}.")) {
            return Some(path);
        }
    }
    None
}

pub fn sanitize_text(input: &str) -> String {
    let filtered: String = input
        .chars()
        .filter(|c| !matches!(*c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect();
    sanitize(filtered)
}

pub fn parse_yt_dlp_progress(line: &str) -> Option<f32> {
    let percent_index = line.rfind('%')?;
    let bytes = line.as_bytes();
    let mut start = percent_index;
    while start > 0 {
        let ch = bytes[start - 1] as char;
        if ch.is_ascii_digit() || ch == '.' {
            start -= 1;
        } else {
            break;
        }
    }
    let value = line[start..percent_index].trim();
    value.parse::<f32>().ok()
}
