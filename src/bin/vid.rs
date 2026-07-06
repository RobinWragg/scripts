use curl;
use regex::Regex;
use serde_json::Value as JsonValue;
use std::collections::HashSet;
use std::error::Error;
use std::fs;
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

const CHANNELS_FILE_PATH: &str =
    "/Users/robin/Library/Mobile Documents/com~apple~CloudDocs/video/yt-channels.txt";
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
            Err(e) => println!("Failed to get video: {} because: {}", video_value, e),
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
    let recency = value["metadata"]["lockupMetadataViewModel"]["metadata"]
        ["contentMetadataViewModel"]["metadataRows"][0]["metadataParts"]
        .as_array()
        .and_then(|parts| {
            parts
                .iter()
                .filter_map(|part| part["text"]["content"].as_str())
                .find(|s| s.contains("ago"))
        })
        .map(|s| s.to_string())
        .ok_or("Couldn't get recency string")?;

    let date = parse_recency(&recency, now)?;
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

fn parse_channel_name(line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }

    let line = line.trim_end_matches('/');
    let line = line
        .strip_prefix("https://www.youtube.com/@")
        .unwrap_or(line);
    let line = line.strip_prefix('@').unwrap_or(line);

    if line.is_empty() {
        None
    } else {
        Some(line.to_string())
    }
}

fn main() -> AppResult<()> {
    let channels_file = fs::read_to_string(CHANNELS_FILE_PATH)?;

    for channel in channels_file.lines().filter_map(parse_channel_name) {
        println!("Channel: {}", channel);
        match get_channel_videos(&channel) {
            Ok(videos) => println!("{:#?}", videos),
            Err(e) => eprintln!("Failed to get videos for {}: {}", channel, e),
        }
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
