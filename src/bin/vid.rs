use curl;
use serde_json::Value as JsonValue;
use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Command;

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

fn channels_list_path() -> String {
    format!("{}/{}", channels_root_path(), CHANNELS_LIST_FILENAME)
}

fn downloaded_list_path() -> String {
    format!("{}/{}", channels_root_path(), DOWNLOADED_LIST_FILENAME)
}

fn channels_root_path() -> String {
    std::env::var("VID_CHANNELS_ROOT_PATH")
        .unwrap_or_else(|_| CHANNELS_ROOT_PATH_DEFAULT.to_string())
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
    let mut min_height = 700u32;
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
    if !Path::new(&path).exists() {
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

fn download_video(
    channel_id: &str,
    video_id: &str,
    download_root: &Path,
    min_height: u32,
    skip_download: bool,
) -> AppResult<bool> {
    if skip_download {
        println!("Skipping download for {}", video_id);
        return Ok(true);
    }

    let url = format!("https://www.youtube.com/watch?v={}", video_id);
    let output_template = format!("{} - %(title)s.%(ext)s", channel_id);
    let height_selector = format!("+height:{}", min_height);
    let output = Command::new("yt-dlp")
        .current_dir(download_root)
        .arg("--js-runtimes")
        .arg("node")
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

fn main() {
    println!("vid main() running");
    println!("Operating in path: {}", channels_root_path());
    let channels = load_channel_rules().expect("Failed to load channel rules");
    let download_root_root = channels_root_path();
    let download_root = Path::new(&download_root_root);
    let download_root_item_count =
        count_directory_items(download_root).expect("Failed to count dir items");
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
                download_root,
                channel.min_height,
                skip_downloads,
            ) {
                Ok(true) => {
                    downloaded_videos.insert(video.video_id.clone());
                    save_downloaded_video_ids(&downloaded_videos)
                        .expect("Failed to save downloaded video IDs");
                }
                Ok(false) => {
                    eprintln!("yt-dlp failed for {}", video.video_id);
                }
                Err(e) => {
                    eprintln!("Failed to run yt-dlp for {}: {}", video.video_id, e);
                }
            }
        }
        println!();
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
}
