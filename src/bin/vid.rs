use chrono::DateTime;
use curl;
use serde_json::Value as JsonValue;
use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::Command;

use rss::Channel;

type AppResult<T> = Result<T, Box<dyn Error>>;

const CHANNELS_ROOT_PATH_DEFAULT: &str = ".";

#[derive(Clone, Debug)]
pub struct VideoDetails {
    pub video_id: String,
    pub channel_id: String,
    pub title: String,
}

const CHANNELS_LIST_FILENAME: &str = "yt-channels.txt";
const DOWNLOADED_LIST_FILENAME: &str = "yt-downloaded.txt";
const PODCASTS_CHANNELS_LIST_FILENAME: &str = "podcasts-channels.txt";
const PODCASTS_DOWNLOADED_LIST_FILENAME: &str = "podcasts-downloaded.txt";
const YTDLP_JS_RUNTIMES_ENV: &str = "VID_YTDLP_JS_RUNTIMES";
const ACCEPT_LANGUAGE_HEADER: &str = "Accept-Language: en-US,en;q=0.9";
const ACCEPT_HEADER: &str =
    "Accept: text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8";
const USER_AGENT_HEADER: &str = "User-Agent: Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

pub fn get_channel_videos(channel_id: &str) -> AppResult<Vec<VideoDetails>> {
    let page = fetch_channel_videos_page(channel_id)?;

    let Some(json) = json_from_page(&page) else {
        println!(
            "Couldn't get json from page: {}",
            video_list_url(channel_id)
        );
        return Ok(vec![]);
    };

    let serde_value: JsonValue = serde_json::from_str(&json)?;
    let mut video_objects = vec![];
    collect_lockup_videos(&serde_value, &mut video_objects);

    let mut seen_video_ids = HashSet::new();
    let mut videos_out = vec![];

    for video_value in video_objects {
        match get_video(channel_id, video_value) {
            Ok(video) => {
                if seen_video_ids.insert(video.video_id.clone()) {
                    videos_out.push(video);
                }
            }
            Err(e) => {
                let video_id = video_value["contentId"].as_str().unwrap_or("<unknown>");
                println!("Failed to get video {} because: {}", video_id, e);
            }
        }
    }

    Ok(videos_out)
}

fn fetch_channel_videos_page(channel_id: &str) -> AppResult<String> {
    let url = video_list_url(channel_id);
    let mut easy_curl = curl::easy::Easy::new();
    easy_curl.follow_location(true)?;
    let mut dst = Vec::new();

    {
        easy_curl.url(&url)?;
        let mut headers = curl::easy::List::new();
        headers.append(ACCEPT_LANGUAGE_HEADER)?;
        headers.append(ACCEPT_HEADER)?;
        headers.append(USER_AGENT_HEADER)?;
        easy_curl.http_headers(headers)?;
        let mut transfer = easy_curl.transfer();
        transfer.write_function(|data| {
            dst.extend_from_slice(data);
            Ok(data.len())
        })?;
        transfer.perform()?; // note, this failed once.
    }

    assert_eq!(easy_curl.response_code(), Ok(200));
    Ok(std::str::from_utf8(&dst)?.to_string())
}

fn get_video(channel_id: &str, value: &JsonValue) -> AppResult<VideoDetails> {
    let video_id = value["contentId"]
        .as_str()
        .ok_or("Couldn't get contentId str")?
        .to_string();

    let title = value["metadata"]["lockupMetadataViewModel"]["title"]["content"]
        .as_str()
        .ok_or("Couldn't get title string")?
        .to_string();

    Ok(VideoDetails {
        channel_id: channel_id.to_string(),
        video_id,
        title,
    })
}

fn json_from_page(page: &str) -> Option<String> {
    extract_js_object(page, "var ytInitialData = ")
        .or_else(|| extract_js_object(page, "ytInitialData = "))
}

fn extract_js_object(page: &str, marker: &str) -> Option<String> {
    let marker_start = page.find(marker)? + marker.len();
    let after_marker = &page[marker_start..];
    let object_rel_start = after_marker.find('{')?;
    let object_start = marker_start + object_rel_start;

    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (offset, ch) in page[object_start..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    let end = object_start + offset + ch.len_utf8();
                    return Some(page[object_start..end].to_string());
                }
            }
            _ => {}
        }
    }

    None
}

fn collect_lockup_videos<'a>(value: &'a JsonValue, out: &mut Vec<&'a JsonValue>) {
    match value {
        JsonValue::Object(map) => {
            if is_lockup_video_item(value) {
                out.push(value);
            }
            for nested in map.values() {
                collect_lockup_videos(nested, out);
            }
        }
        JsonValue::Array(items) => {
            for item in items {
                collect_lockup_videos(item, out);
            }
        }
        _ => {}
    }
}

fn is_lockup_video_item(value: &JsonValue) -> bool {
    value["contentType"].as_str() == Some("LOCKUP_CONTENT_TYPE_VIDEO")
        && value["contentId"].as_str().is_some()
        && value["metadata"]["lockupMetadataViewModel"]["title"]["content"]
            .as_str()
            .is_some()
}

fn video_list_url(channel_id: &str) -> String {
    "https://www.youtube.com/@".to_owned() + channel_id + "/videos"
}

fn channels_list_path() -> PathBuf {
    channels_root_path().join(CHANNELS_LIST_FILENAME)
}

fn downloaded_list_path() -> PathBuf {
    channels_root_path().join(DOWNLOADED_LIST_FILENAME)
}

fn podcasts_channels_list_path() -> PathBuf {
    channels_root_path().join(PODCASTS_CHANNELS_LIST_FILENAME)
}

fn podcasts_downloaded_list_path() -> PathBuf {
    channels_root_path().join(PODCASTS_DOWNLOADED_LIST_FILENAME)
}

fn channels_root_path() -> PathBuf {
    std::env::var("VID_CHANNELS_ROOT_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(CHANNELS_ROOT_PATH_DEFAULT))
}

fn download_root_path() -> PathBuf {
    channels_root_path().join("media")
}

fn count_directory_items(path: &Path) -> AppResult<usize> {
    Ok(fs::read_dir(path)?.count())
}

#[derive(Clone, Debug)]
struct ChannelRule {
    channel_id: String,
    ignore_substrings: Vec<String>,
    min_height: u32,
}

fn parse_channel_rule(line: &str, line_number: usize) -> AppResult<ChannelRule> {
    let mut parts = line.split_whitespace();
    let Some(channel_id) = parts.next() else {
        return Err(format!("Line {} is missing a channel id", line_number).into());
    };

    let mut ignore_substrings = Vec::new();
    let mut min_height = 800u32;
    let mut saw_height_override = false;

    for token in parts {
        if let Some(ignore) = token.strip_prefix('-') {
            if ignore.is_empty() {
                return Err(format!(
                    "Line {} has an empty ignore modifier: {}",
                    line_number, token
                )
                .into());
            }
            ignore_substrings.push(ignore.to_ascii_lowercase());
            continue;
        }

        if token.chars().all(|c| c.is_ascii_digit()) {
            if saw_height_override {
                return Err(format!(
                    "Line {} has more than one numeric height modifier",
                    line_number
                )
                .into());
            }

            let parsed = token.parse::<u32>()?;
            if parsed == 0 {
                return Err(format!("Line {} has invalid height 0", line_number).into());
            }

            min_height = parsed;
            saw_height_override = true;
            continue;
        }

        return Err(format!("Line {} has invalid modifier: {}", line_number, token).into());
    }

    Ok(ChannelRule {
        channel_id: channel_id.to_string(),
        ignore_substrings,
        min_height,
    })
}

fn load_channel_rules() -> AppResult<Vec<ChannelRule>> {
    let path = channels_list_path();
    let content = fs::read_to_string(&path)?;

    let mut channels = Vec::new();
    for (index, raw_line) in content.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        channels.push(parse_channel_rule(line, line_number)?);
    }

    Ok(channels)
}

fn should_ignore_video(title: &str, ignore_substrings: &[String]) -> bool {
    let lowercase_title = title.to_ascii_lowercase();
    ignore_substrings
        .iter()
        .any(|needle| lowercase_title.contains(needle))
}

fn load_downloaded_video_ids() -> AppResult<HashSet<String>> {
    let path = downloaded_list_path();
    if !path.exists() {
        return Ok(HashSet::new());
    }

    let content = fs::read_to_string(&path)?;
    Ok(content
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect())
}

fn save_downloaded_video_ids(ids: &HashSet<String>) -> AppResult<()> {
    let mut ids_sorted: Vec<&String> = ids.iter().collect();
    ids_sorted.sort();

    let mut output = String::new();
    for id in ids_sorted {
        output.push_str(id);
        output.push('\n');
    }

    fs::write(downloaded_list_path(), output)?;
    Ok(())
}

fn load_downloaded_podcast_ids() -> AppResult<HashSet<String>> {
    let path = podcasts_downloaded_list_path();
    if !path.exists() {
        return Ok(HashSet::new());
    }

    let content = fs::read_to_string(&path)?;
    Ok(content
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect())
}

fn save_downloaded_podcast_ids(ids: &HashSet<String>) -> AppResult<()> {
    let mut ids_sorted: Vec<&String> = ids.iter().collect();
    ids_sorted.sort();

    let mut output = String::new();
    for id in ids_sorted {
        output.push_str(id);
        output.push('\n');
    }

    fs::write(podcasts_downloaded_list_path(), output)?;
    Ok(())
}

fn command_exists(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn yt_dlp_js_runtimes() -> Vec<String> {
    if let Ok(from_env) = std::env::var(YTDLP_JS_RUNTIMES_ENV) {
        let configured: Vec<String> = from_env
            .split(|c: char| c == ',' || c.is_whitespace())
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .map(|token| token.to_string())
            .collect();

        if !configured.is_empty() {
            return configured;
        }
    }

    let mut runtimes = Vec::new();

    // yt-dlp defaults to deno. Prefer it first when available.
    if command_exists("deno") {
        runtimes.push("deno".to_string());
    }

    if command_exists("node") {
        runtimes.push("node".to_string());
    } else if Path::new("/usr/bin/nodejs").exists() {
        runtimes.push("node:/usr/bin/nodejs".to_string());
    }

    runtimes
}

fn download_video(
    channel_id: &str,
    video_id: &str,
    download_root: &Path,
    min_height: u32,
    skip_download: bool,
    js_runtimes: &[String],
) -> AppResult<bool> {
    if skip_download {
        println!("Skipping download for {}", video_id);
        return Ok(true);
    }

    let url = format!("https://www.youtube.com/watch?v={}", video_id);
    let output_template = format!("{} - %(title)s.%(ext)s", channel_id);
    let height_selector = format!("+height:{}", min_height);
    let mut command = Command::new("yt-dlp");
    command.current_dir(download_root);

    for js_runtime in js_runtimes {
        command.arg("--js-runtimes").arg(js_runtime);
    }

    let output = command
        .arg("-S")
        .arg(height_selector)
        .arg("-o")
        .arg(output_template)
        .arg(&url)
        .output()?;

    if output.status.success() {
        return Ok(true);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if is_members_only_error(&stderr) {
        println!(
            "Treating members-only video {} as successfully handled",
            video_id
        );
        return Ok(true);
    }

    if is_scheduled_premiere_error(&stderr) {
        println!(
            "Skipping scheduled premiere {} until it becomes available",
            video_id
        );
        return Ok(false);
    }

    let stderr_trimmed = stderr.trim();
    if !stderr_trimmed.is_empty() {
        eprintln!("yt-dlp error for {}:\n{}", video_id, stderr_trimmed);
    } else {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stdout_trimmed = stdout.trim();
        if !stdout_trimmed.is_empty() {
            eprintln!("yt-dlp output for {}:\n{}", video_id, stdout_trimmed);
        }
    }

    Ok(false)
}

fn is_members_only_error(stderr: &str) -> bool {
    let stderr = stderr.to_ascii_lowercase();
    stderr.contains("members-only")
        || stderr.contains("available to this channel's members")
        || stderr.contains("join this channel to get access")
}

fn is_scheduled_premiere_error(stderr: &str) -> bool {
    let stderr = stderr.to_ascii_lowercase();
    stderr.contains("premieres in ")
        || stderr.contains("premiere will begin shortly")
        || stderr.contains("this live event will begin in")
}

#[derive(Clone, Debug)]
struct PodcastEpisode {
    id: String,
    url: String,
    title: String,
    artist: String,
    album: String,
    published_timestamp: Option<i64>,
}

#[derive(Clone, Debug, Default)]
struct AudioMetadata {
    artist: Option<String>,
    album: Option<String>,
    title: Option<String>,
}

fn load_podcast_feed_urls() -> AppResult<Vec<String>> {
    let path = podcasts_channels_list_path();
    if !path.exists() {
        return Ok(vec![]);
    }

    let content = fs::read_to_string(&path)?;
    Ok(content
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.to_string())
        .collect())
}

fn fetch_podcast_feed(feed_url: &str) -> AppResult<String> {
    let mut easy_curl = curl::easy::Easy::new();
    easy_curl.follow_location(true)?;
    let mut dst = Vec::new();

    {
        easy_curl.url(feed_url)?;
        let mut transfer = easy_curl.transfer();
        transfer.write_function(|data| {
            dst.extend_from_slice(data);
            Ok(data.len())
        })?;
        transfer.perform()?;
    }

    let response_code = easy_curl.response_code()?;
    if response_code != 200 {
        return Err(format!(
            "Podcast feed request failed with HTTP status {}",
            response_code
        )
        .into());
    }

    Ok(std::str::from_utf8(&dst)?.to_string())
}

fn get_podcast_episodes(feed_url: &str) -> AppResult<Vec<PodcastEpisode>> {
    let feed = fetch_podcast_feed(feed_url)?;
    let channel = Channel::read_from(BufReader::new(feed.as_bytes()))?;
    let album = channel.title().trim();
    let artist = channel.managing_editor().unwrap_or_else(|| "").trim();
    let artist = if artist.is_empty() { album } else { artist };

    let mut episodes = Vec::new();
    for item in channel.items() {
        let Some(enclosure) = item.enclosure() else {
            continue;
        };
        let url = enclosure.url().trim();
        if url.is_empty() {
            continue;
        }

        let title = item
            .title()
            .map(|title| title.trim())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| enclosure_filename(url));
        let episode_artist = item
            .author()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(artist);
        let id = item
            .guid()
            .map(|guid| guid.value().trim())
            .filter(|guid| !guid.is_empty())
            .unwrap_or(url)
            .to_string();
        let published_timestamp = item.pub_date().and_then(parse_podcast_timestamp);

        episodes.push(PodcastEpisode {
            id,
            url: url.to_string(),
            title: title.to_string(),
            artist: episode_artist.to_string(),
            album: album.to_string(),
            published_timestamp,
        });
    }

    sort_podcast_episodes(&mut episodes);
    Ok(episodes)
}

fn parse_podcast_timestamp(pub_date: &str) -> Option<i64> {
    DateTime::parse_from_rfc2822(pub_date)
        .ok()
        .map(|date| date.timestamp())
}

fn sort_podcast_episodes(episodes: &mut [PodcastEpisode]) {
    episodes.sort_by_key(|episode| episode.published_timestamp.unwrap_or(i64::MAX));
}

fn enclosure_filename(url: &str) -> &str {
    url.rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("untitled")
}

fn download_podcast_episode(
    episode: &PodcastEpisode,
    download_root: &Path,
    skip_download: bool,
) -> AppResult<bool> {
    if skip_download {
        println!("Skipping podcast download for {}", episode.id);
        return Ok(true);
    }

    let source = tempfile::Builder::new()
        .prefix(".podcast-")
        .tempfile_in(download_root)?;
    let output = source.path();

    let download = Command::new("curl")
        .arg("-fL")
        .arg("--retry")
        .arg("3")
        .arg("-o")
        .arg(output)
        .arg(&episode.url)
        .output();

    match download {
        Ok(result) if result.status.success() => {}
        Ok(result) => {
            let stderr = String::from_utf8_lossy(&result.stderr);
            let stdout = String::from_utf8_lossy(&result.stdout);
            eprintln!("Podcast download failed for {}:", episode.id);
            if !stderr.trim().is_empty() {
                eprintln!("{}", stderr.trim());
            } else if !stdout.trim().is_empty() {
                eprintln!("{}", stdout.trim());
            }
            return Ok(false);
        }
        Err(e) => {
            eprintln!("Failed to run curl for {}: {}", episode.id, e);
            return Ok(false);
        }
    }

    if source.as_file().metadata()?.len() == 0 {
        eprintln!("Podcast download was empty for {}", episode.id);
        return Ok(false);
    }

    let source_path = source.path().to_path_buf();
    let metadata = probe_audio_metadata(&source_path).unwrap_or_default();
    let artist = metadata
        .artist
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| episode.artist.clone());
    let album = metadata
        .album
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| episode.album.clone());
    let title = metadata
        .title
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| episode.title.clone());

    let output_filename = podcast_output_filename(&artist, &title);
    let output_path = download_root.join(output_filename);

    let has_artwork = probe_attached_artwork(&source_path).unwrap_or(false);
    let mut ffmpeg = Command::new("ffmpeg");
    ffmpeg.arg("-y").arg("-v").arg("error");

    if has_artwork {
        ffmpeg
            .arg("-i")
            .arg(&source_path)
            .arg("-map")
            .arg("0:a:0")
            .arg("-map")
            .arg("0:v:0")
            .arg("-map_metadata")
            .arg("0")
            .arg("-c")
            .arg("copy")
            .arg("-disposition:v:0")
            .arg("attached_pic");
    } else {
        ffmpeg
            .arg("-f")
            .arg("lavfi")
            .arg("-i")
            .arg("color=c=black:s=320x180:r=1")
            .arg("-i")
            .arg(&source_path)
            .arg("-map")
            .arg("1:a:0")
            .arg("-map")
            .arg("0:v:0")
            .arg("-map_metadata")
            .arg("1")
            .arg("-c:a")
            .arg("copy")
            .arg("-c:v")
            .arg("mpeg4")
            .arg("-q:v")
            .arg("2")
            .arg("-shortest");
    }

    let result = ffmpeg
        .arg("-metadata")
        .arg(format!("artist={}", artist))
        .arg("-metadata")
        .arg(format!("album={}", album))
        .arg("-metadata")
        .arg(format!("title={}", title))
        .arg(&output_path)
        .output();

    match result {
        Ok(result) if result.status.success() => {
            println!("Remuxed podcast episode to {}", output_path.display());
            Ok(true)
        }
        Ok(result) => {
            let _ = fs::remove_file(&output_path);
            let stderr = String::from_utf8_lossy(&result.stderr);
            eprintln!("ffmpeg remux failed for {}:\n{}", episode.id, stderr.trim());
            Ok(false)
        }
        Err(e) => {
            let _ = fs::remove_file(&output_path);
            eprintln!("Failed to run ffmpeg for {}: {}", episode.id, e);
            Ok(false)
        }
    }
}

fn sanitize_filename_component(value: &str) -> String {
    let mut sanitized = String::new();
    for ch in value.trim().chars() {
        if ch.is_control() || matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
            sanitized.push('_');
        } else {
            sanitized.push(ch);
        }
    }

    let sanitized = sanitized.trim().trim_end_matches('.').trim().to_string();
    if sanitized.is_empty() {
        "Unknown".to_string()
    } else {
        sanitized
    }
}

fn podcast_output_filename(artist: &str, title: &str) -> String {
    format!(
        "{} - {}.mkv",
        sanitize_filename_component(artist),
        sanitize_filename_component(title)
    )
}

fn probe_audio_metadata(source_path: &Path) -> AppResult<AudioMetadata> {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-show_entries")
        .arg("format_tags=artist,album,title:stream_tags=artist,album,title")
        .arg("-of")
        .arg("json")
        .arg(source_path)
        .output()?;

    if !output.status.success() {
        return Ok(AudioMetadata::default());
    }

    let value: JsonValue = serde_json::from_slice(&output.stdout)?;
    let mut metadata = AudioMetadata::default();

    if let Some(format) = value.get("format") {
        if let Some(tags) = format.get("tags") {
            metadata.artist = json_tag(tags, "artist");
            metadata.album = json_tag(tags, "album");
            metadata.title = json_tag(tags, "title");
        }
    }

    if let Some(streams) = value.get("streams").and_then(JsonValue::as_array) {
        for stream in streams {
            let Some(tags) = stream.get("tags") else {
                continue;
            };
            if metadata.artist.is_none() {
                metadata.artist = json_tag(tags, "artist");
            }
            if metadata.album.is_none() {
                metadata.album = json_tag(tags, "album");
            }
            if metadata.title.is_none() {
                metadata.title = json_tag(tags, "title");
            }
        }
    }

    Ok(metadata)
}

fn json_tag(tags: &JsonValue, name: &str) -> Option<String> {
    let object = tags.as_object()?;
    object
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .and_then(|(_, value)| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn probe_attached_artwork(source_path: &Path) -> AppResult<bool> {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v")
        .arg("-show_entries")
        .arg("stream=index:stream_disposition=attached_pic")
        .arg("-of")
        .arg("json")
        .arg(source_path)
        .output()?;

    if !output.status.success() {
        return Ok(false);
    }

    let value: JsonValue = serde_json::from_slice(&output.stdout)?;
    let Some(streams) = value.get("streams").and_then(JsonValue::as_array) else {
        return Ok(false);
    };

    Ok(streams.iter().any(|stream| {
        stream
            .get("disposition")
            .and_then(|disposition| disposition.get("attached_pic"))
            .and_then(JsonValue::as_i64)
            == Some(1)
    }))
}

fn main() {
    println!("vid main() running");
    let download_root = download_root_path();
    println!("Operating in path: {}", download_root.display());
    let yt_dlp_js_runtimes = yt_dlp_js_runtimes();
    if !yt_dlp_js_runtimes.is_empty() {
        println!(
            "Using yt-dlp JS runtimes: {}",
            yt_dlp_js_runtimes.join(", ")
        );
    } else {
        println!("No yt-dlp JS runtime detected; downloads may fail with YouTube runtime warnings");
    }
    let channels = load_channel_rules().expect("Failed to load channel rules");
    fs::create_dir_all(&download_root).expect("Failed to create download root directory");
    let download_root_item_count =
        count_directory_items(download_root.as_path()).expect("Failed to count dir items");
    let skip_downloads_due_to_root_size = download_root_item_count > 200;
    let mut downloaded_videos =
        load_downloaded_video_ids().expect("Failed to load downloaded video IDs");
    let skip_downloads = std::env::args().any(|arg| arg == "--skip-downloads");

    if skip_downloads_due_to_root_size {
        println!(
            "Skipping downloads because root contains {} items (> 200)",
            download_root_item_count
        );
    }

    for channel in channels {
        println!("Channel: {}", channel.channel_id);
        let mut downloaded_for_channel = false;

        let videos = match get_channel_videos(&channel.channel_id) {
            Ok(videos) => videos,
            Err(e) => {
                eprintln!("Failed to get videos for {}: {}", channel.channel_id, e);
                println!();
                continue;
            }
        };

        println!("Found {} videos", videos.len());
        if skip_downloads_due_to_root_size {
            continue;
        }

        for video in &videos {
            if downloaded_videos.contains(&video.video_id) {
                continue;
            }

            if should_ignore_video(&video.title, &channel.ignore_substrings) {
                println!("Ignoring {} due to title filter", video.video_id);
                continue;
            }

            println!("Downloading {} - {}", video.video_id, video.title);
            match download_video(
                &channel.channel_id,
                &video.video_id,
                download_root.as_path(),
                channel.min_height,
                skip_downloads,
                &yt_dlp_js_runtimes,
            ) {
                Ok(true) => {
                    downloaded_videos.insert(video.video_id.clone());
                    save_downloaded_video_ids(&downloaded_videos)
                        .expect("Failed to save downloaded video IDs");
                    downloaded_for_channel = true;
                }
                Ok(false) => {
                    eprintln!("yt-dlp failed for {}", video.video_id);
                }
                Err(e) => {
                    eprintln!("Failed to run yt-dlp for {}: {}", video.video_id, e);
                }
            }

            if downloaded_for_channel {
                break;
            }
        }
        println!();
    }

    if !skip_downloads_due_to_root_size {
        let podcast_feeds = load_podcast_feed_urls().expect("Failed to load podcast feeds");
        let mut downloaded_podcasts =
            load_downloaded_podcast_ids().expect("Failed to load downloaded podcast IDs");

        for feed_url in podcast_feeds {
            println!("Podcast feed: {}", feed_url);
            let mut downloaded_for_feed = false;
            let episodes = match get_podcast_episodes(&feed_url) {
                Ok(episodes) => episodes,
                Err(e) => {
                    eprintln!("Failed to get podcast episodes for {}: {}", feed_url, e);
                    println!();
                    continue;
                }
            };

            println!("Found {} podcast episodes", episodes.len());
            for episode in &episodes {
                if downloaded_podcasts.contains(&episode.id) {
                    continue;
                }

                println!(
                    "Downloading podcast episode {} - {}",
                    episode.id, episode.title
                );
                match download_podcast_episode(episode, download_root.as_path(), skip_downloads) {
                    Ok(true) => {
                        downloaded_podcasts.insert(episode.id.clone());
                        save_downloaded_podcast_ids(&downloaded_podcasts)
                            .expect("Failed to save downloaded podcast IDs");
                        downloaded_for_feed = true;
                    }
                    Ok(false) => {
                        eprintln!("Podcast download/remux failed for {}", episode.id);
                    }
                    Err(e) => {
                        eprintln!("Failed to process podcast {}: {}", episode.id, e);
                    }
                }

                if downloaded_for_feed {
                    break;
                }
            }
            println!();
        }

        println!(
            "Synced downloaded podcast IDs: {} entries",
            downloaded_podcasts.len()
        );
    }

    println!("Synced downloaded IDs: {} entries", downloaded_videos.len());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_video() {
        let json: serde_json::Value = serde_json::json!({
            "contentType": "LOCKUP_CONTENT_TYPE_VIDEO",
            "contentId": "abc123",
            "metadata": {
                "lockupMetadataViewModel": {
                    "title": { "content": "Example title" },
                    "metadata": {
                        "contentMetadataViewModel": {
                            "metadataRows": [
                                {"metadataParts": [
                                    {"text": {"content": "1.2K views"}},
                                    {"text": {"content": "12d ago"}}
                                ]}
                            ]
                        }
                    }
                }
            }
        });
        let video = get_video("mychannel", &json).unwrap();
        assert_eq!(video.video_id, "abc123");
        assert_eq!(video.channel_id, "mychannel");
        assert_eq!(video.title, "Example title");
    }

    #[test]
    fn test_sanitize_filename_component() {
        assert_eq!(
            sanitize_filename_component("  artist:name  "),
            "artist_name"
        );
        assert_eq!(sanitize_filename_component(""), "Unknown");
    }

    #[test]
    fn test_podcast_filename_shape() {
        let artist = sanitize_filename_component("Test Artist");
        let title = sanitize_filename_component("Test Title");
        assert_eq!(
            podcast_output_filename(&artist, &title),
            "Test Artist - Test Title.mkv"
        );
    }

    #[test]
    fn test_podcast_episodes_are_sorted_chronologically() {
        let mut episodes = vec![
            PodcastEpisode {
                id: "new".to_string(),
                url: "https://example.com/new.mp3".to_string(),
                title: "New".to_string(),
                artist: "Artist".to_string(),
                album: "Feed".to_string(),
                published_timestamp: Some(2),
            },
            PodcastEpisode {
                id: "undated".to_string(),
                url: "https://example.com/undated.mp3".to_string(),
                title: "Undated".to_string(),
                artist: "Artist".to_string(),
                album: "Feed".to_string(),
                published_timestamp: None,
            },
            PodcastEpisode {
                id: "old".to_string(),
                url: "https://example.com/old.mp3".to_string(),
                title: "Old".to_string(),
                artist: "Artist".to_string(),
                album: "Feed".to_string(),
                published_timestamp: Some(1),
            },
        ];

        sort_podcast_episodes(&mut episodes);

        assert_eq!(episodes[0].id, "old");
        assert_eq!(episodes[1].id, "new");
        assert_eq!(episodes[2].id, "undated");
    }

    #[test]
    fn test_parse_podcast_timestamp() {
        assert!(parse_podcast_timestamp("Tue, 24 Nov 2026 09:44:20 GMT").is_some());
        assert!(parse_podcast_timestamp("not a date").is_none());
    }
}
