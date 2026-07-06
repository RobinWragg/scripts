use curl;
use regex::Regex;
use serde_json::Value as JsonValue;
use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

type AppResult<T> = Result<T, Box<dyn Error>>;
type DateTime = chrono::DateTime<chrono::Utc>;

#[derive(Clone, Debug)]
pub struct VideoDetails {
    pub video_id: String,
    pub channel_id: String,
    pub title: String,
    pub date: DateTime,
}

const CHANNELS_ROOT_PATH: &str = "/Users/robin/Library/Mobile Documents/com~apple~CloudDocs/video";
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

    let now = chrono::Utc::now();
    let mut seen_video_ids = HashSet::new();
    let mut videos_out = vec![];

    for video_value in video_objects {
        match get_video(channel_id, video_value, &now) {
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

fn get_video(channel_id: &str, value: &JsonValue, now: &DateTime) -> AppResult<VideoDetails> {
    let video_id = value["contentId"]
        .as_str()
        .ok_or("Couldn't get contentId str")?
        .to_string();

    let recency = match value["metadata"]["lockupMetadataViewModel"]["metadata"]
        ["contentMetadataViewModel"]["metadataRows"][0]["metadataParts"]
        .as_array()
        .and_then(|parts| {
            parts
                .iter()
                .filter_map(|part| part["text"]["content"].as_str())
                .find(|s| s.contains("ago"))
        }) {
        Some(recency) => recency.to_string(),
        None => {
            eprintln!("Couldn't get recency string for video {}", video_id);
            return Err("Couldn't get recency string".into());
        }
    };

    let date = parse_recency(&recency, now)?;
    let title = value["metadata"]["lockupMetadataViewModel"]["title"]["content"]
        .as_str()
        .ok_or("Couldn't get title string")?
        .to_string();

    Ok(VideoDetails {
        channel_id: channel_id.to_string(),
        video_id,
        title,
        date,
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

fn parse_recency(s: &str, now: &DateTime) -> AppResult<DateTime> {
    const SECONDS_PER_MINUTE: u64 = 60;
    const SECONDS_PER_HOUR: u64 = SECONDS_PER_MINUTE * 60;
    const SECONDS_PER_DAY: u64 = SECONDS_PER_HOUR * 24;
    const SECONDS_PER_WEEK: u64 = SECONDS_PER_DAY * 7;
    const SECONDS_PER_MONTH: u64 = SECONDS_PER_DAY * 30; // Approximate!
    const SECONDS_PER_YEAR: u64 = SECONDS_PER_DAY * 365; // Approximate!

    let re = Regex::new(r"(?i)(?:streamed\s+)?(\d+)\s*([a-z]+)\s*ago")?;
    let captures = re
        .captures(s)
        .ok_or(format!("captures() call failed on str: {s}"))?;

    let base_number = captures[1].parse::<u64>()?;

    let unit = captures[2].to_ascii_lowercase();
    let seconds = match unit.as_str() {
        "s" | "sec" | "secs" | "second" | "seconds" => base_number,
        "m" | "min" | "mins" | "minute" | "minutes" => base_number * SECONDS_PER_MINUTE,
        "h" | "hr" | "hrs" | "hour" | "hours" => base_number * SECONDS_PER_HOUR,
        "d" | "day" | "days" => base_number * SECONDS_PER_DAY,
        "w" | "week" | "weeks" => base_number * SECONDS_PER_WEEK,
        "mo" | "mon" | "month" | "months" => base_number * SECONDS_PER_MONTH,
        "y" | "yr" | "yrs" | "year" | "years" => base_number * SECONDS_PER_YEAR,
        _ => return Err(format!("Unknown recency unit: {}", unit).into()),
    };

    Ok(*now - Duration::from_secs(seconds))
}

#[derive(Debug)]
struct ChannelSource {
    display_name: String,
    channel_id: String,
    config_path: PathBuf,
    channel_path: PathBuf,
}

fn load_channels_from_configs() -> AppResult<Vec<ChannelSource>> {
    let mut channels = vec![];

    for entry in fs::read_dir(CHANNELS_ROOT_PATH)? {
        let path = entry?.path();
        if !path.is_dir() {
            continue;
        }

        let Some(display_name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };

        let config_path = path.join("config.json");
        if !config_path.exists() {
            let config_json = serde_json::json!({
                "channel_id": display_name,
                "downloaded_videos": []
            });
            save_config(&config_path, &config_json)?;
            println!("Created config.json for {}", display_name);
        }

        let config_file = fs::read_to_string(&config_path)?;
        let config_json: JsonValue = serde_json::from_str(&config_file)?;
        let Some(channel_id) = config_json["channel_id"].as_str() else {
            println!(
                "Skipping {}: missing channel_id in config.json",
                display_name
            );
            continue;
        };

        channels.push(ChannelSource {
            display_name: display_name.to_string(),
            channel_id: channel_id.to_string(),
            config_path,
            channel_path: path,
        });
    }

    channels.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    Ok(channels)
}

fn read_config(path: &Path) -> AppResult<JsonValue> {
    let config_file = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&config_file)?)
}

fn get_downloaded_videos(config: &JsonValue) -> Vec<String> {
    config["downloaded_videos"]
        .as_array()
        .map(|ids| {
            ids.iter()
                .filter_map(|value| value.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn set_downloaded_videos(config: &mut JsonValue, videos: &[String]) {
    config["downloaded_videos"] = JsonValue::Array(
        videos
            .iter()
            .map(|id| JsonValue::String(id.clone()))
            .collect(),
    );
}

fn save_config(path: &Path, config: &JsonValue) -> AppResult<()> {
    let serialized = serde_json::to_string_pretty(config)?;
    fs::write(path, format!("{}\n", serialized))?;
    Ok(())
}

fn download_video(video_id: &str, channel_path: &Path, skip_download: bool) -> AppResult<bool> {
    if skip_download {
        println!("Skipping download for {}", video_id);
        return Ok(true);
    }

    let url = format!("https://www.youtube.com/watch?v={}", video_id);
    let output = Command::new("yt-dlp")
        .current_dir(channel_path)
        .arg("-S")
        .arg("+height:500") // minimum height
        .arg("-o")
        .arg("%(title)s.%(ext)s")
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

    Ok(false)
}

fn is_members_only_error(stderr: &str) -> bool {
    let stderr = stderr.to_ascii_lowercase();
    stderr.contains("members-only")
        || stderr.contains("available to this channel's members")
        || stderr.contains("join this channel to get access")
}

fn main() -> AppResult<()> {
    let channels = load_channels_from_configs()?;
    let skip_downloads = std::env::args().any(|arg| arg == "--skip-downloads");

    for channel in channels {
        println!("Channel: {} ({})", channel.display_name, channel.channel_id);
        let mut config = read_config(&channel.config_path)?;
        let mut downloaded_videos = get_downloaded_videos(&config);
        let mut downloaded_set: HashSet<String> = downloaded_videos.iter().cloned().collect();

        let videos = match get_channel_videos(&channel.channel_id) {
            Ok(videos) => videos,
            Err(e) => {
                eprintln!(
                    "Failed to get videos for {} ({}): {}",
                    channel.display_name, channel.channel_id, e
                );
                println!();
                continue;
            }
        };

        println!("Found {} videos", videos.len());
        for video in &videos {
            if downloaded_set.contains(&video.video_id) {
                continue;
            }

            println!("Downloading {} - {}", video.video_id, video.title);
            match download_video(&video.video_id, &channel.channel_path, skip_downloads) {
                Ok(true) => {
                    downloaded_set.insert(video.video_id.clone());
                    downloaded_videos.push(video.video_id.clone());
                    set_downloaded_videos(&mut config, &downloaded_videos);
                    save_config(&channel.config_path, &config)?;
                }
                Ok(false) => {
                    eprintln!("yt-dlp failed for {}", video.video_id);
                }
                Err(e) => {
                    eprintln!("Failed to run yt-dlp for {}: {}", video.video_id, e);
                }
            }
        }

        let current_video_ids: HashSet<String> =
            videos.iter().map(|v| v.video_id.clone()).collect();
        let mut seen = HashSet::new();
        downloaded_videos.retain(|id| current_video_ids.contains(id) && seen.insert(id.clone()));

        set_downloaded_videos(&mut config, &downloaded_videos);
        save_config(&channel.config_path, &config)?;

        println!(
            "Synced downloaded_videos: {} entries",
            downloaded_videos.len()
        );
        println!();
    }

    Ok(())
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
        let now = chrono::Utc::now();
        let video = get_video("mychannel", &json, &now).unwrap();
        assert_eq!(video.video_id, "abc123");
        assert_eq!(video.channel_id, "mychannel");
        assert_eq!(video.title, "Example title");
    }
}
