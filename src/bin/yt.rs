use curl::easy::Easy;
use rayon::prelude::*;
use regex::Regex;
use serde_json::Value;
use std::{error::Error, io::Result, time::Instant};

const SECONDS_PER_MINUTE: u64 = 60;
const SECONDS_PER_HOUR: u64 = SECONDS_PER_MINUTE * 60;
const SECONDS_PER_DAY: u64 = SECONDS_PER_HOUR * 24;
const SECONDS_PER_WEEK: u64 = SECONDS_PER_DAY * 7;
const SECONDS_PER_MONTH: u64 = SECONDS_PER_DAY * 30; // Approximate!
const SECONDS_PER_YEAR: u64 = SECONDS_PER_DAY * 365; // Approximate!

fn parse_recency(s: &str) -> u64 {
    let re = Regex::new(r"(\d+) (\w+?)s? ago").unwrap();
    let captures = re.captures(s).unwrap();

    let base_number = captures[1].parse::<u64>().unwrap();

    match &captures[2] {
        "second" => base_number,
        "minute" => base_number * SECONDS_PER_MINUTE,
        "hour" => base_number * SECONDS_PER_HOUR,
        "day" => base_number * SECONDS_PER_DAY,
        "week" => base_number * SECONDS_PER_WEEK,
        "month" => base_number * SECONDS_PER_MONTH,
        "year" => base_number * SECONDS_PER_YEAR,
        _ => unreachable!(),
    }
}

fn video_list_url(channel_id: &str) -> String {
    "https://www.youtube.com/@".to_owned() + channel_id + "/videos"
}

fn json_from_page(page: &str) -> String {
    let re = Regex::new(r"var ytInitialData = (.*?);</script>").unwrap();
    re.captures(page)
        .unwrap()
        .get(1)
        .unwrap()
        .as_str()
        .to_string()
}

struct Video {
    channel_id: String,
    video_id: String,
    title: String,
    recency: String,
    seconds_ago: u64,
}

fn get_video(channel_id: &str, value: &Value) -> Option<Video> {
    if *value != Value::Null {
        let recency = value["publishedTimeText"]["simpleText"].to_string();
        let seconds_ago = parse_recency(&recency);
        return Some(Video {
            channel_id: channel_id.to_string(),
            video_id: value["videoId"].to_string(),
            title: value["title"]["runs"][0]["text"].to_string(),
            recency,
            seconds_ago,
        });
    }

    return None;
}

fn get_channel_videos(channel_id: &str, videos_out: &mut Vec<Video>) {
    let url = video_list_url(channel_id);
    let mut easy = Easy::new();
    easy.follow_location(true).unwrap();
    let mut dst = Vec::new();
    {
        easy.url(&url).unwrap();
        let mut transfer = easy.transfer();
        transfer
            .write_function(|data| {
                dst.extend_from_slice(data);
                Ok(data.len())
            })
            .unwrap();
        transfer.perform().unwrap();
    }
    assert_eq!(easy.response_code().unwrap(), 200);
    let page = std::str::from_utf8(&dst).unwrap();
    let json = json_from_page(page);
    let serde_value: Value = serde_json::from_str(&json).unwrap();
    let tabs = &serde_value["contents"]["twoColumnBrowseResultsRenderer"]["tabs"];
    let videos = &tabs[1]["tabRenderer"]["content"]["richGridRenderer"]["contents"];
    let videos = videos.as_array().unwrap();

    for video in videos {
        let video = &video["richItemRenderer"]["content"]["videoRenderer"];
        if let Some(video) = get_video(channel_id, video) {
            videos_out.push(video);
        }
    }
}

fn get_no_show_list() -> String {
    let exe_path = std::env::current_exe().unwrap();
    let dir = exe_path.parent().unwrap();
    let dir = dir.parent().unwrap();
    let dir = dir.parent().unwrap();
    let now_show_path = dir.join("yt_no_show_list.txt");
    std::fs::read_to_string(now_show_path).unwrap()
}

fn main() {
    let timer = Instant::now();

    let loaded_no_shows = get_no_show_list();

    let channel_ids = [
        "SSkorkowsky",
        "tested",
        "AbroadinJapan",
        "HealthyGamerGG",
        "kylehill",
        "mcolville",
        "NASA",
        "smartereveryday",
        "TechnologyConnections",
        "GameGrumps",
        "Wintergatan",
        "TheGrumps",
        "veritasium",
        "MarcusHouse",
        "scottmanley",
        "TomScottGo",
        "kanamenaito",
        "jameshoffmann",
        "BPSspace",
        "ICHIKEN1",
        "EverydayAstronaut",
        "SpaceX",
        "kaguiro",
        "AdamNeely",
        "pbsspacetime",
        "Radiohead",
        "SteveMould",
        "strandbeestfilm",
        "gendesign4714",
        "Einzelganger",
        "StuffMadeHere",
        "theklainc.5724",
        "StarTalk",
        "CosmicPerspective",
        "StrangeParts",
        "helloMCDM",
        "_noisecode",
        "Onomappu",
        "SciShow",
        "JonTronShow",
        "Mystery_Quest",
        "TheGlassCannon",
    ];

    struct Channel {
        id: String,
        videos: Vec<Video>,
    }

    let mut channels: Vec<Channel> = vec![];

    for id in channel_ids {
        channels.push(Channel {
            id: id.to_string(),
            videos: vec![],
        });
    }

    channels.par_iter_mut().for_each(|channel| {
        get_channel_videos(&channel.id, &mut channel.videos);
    });

    let mut all_videos: Vec<Video> = vec![];

    for channel in &mut channels {
        all_videos.append(&mut channel.videos);
    }
    all_videos.sort_by_key(|v| v.seconds_ago);

    println!("Elapsed: {:.2?}", timer.elapsed());
    for video in all_videos {
        println!(
            "{}\t{} {}",
            video.channel_id, video.recency, video.seconds_ago
        );
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn recency_parsing() {
        use crate::parse_recency;
        assert_eq!(parse_recency("1 second ago"), 1);
        assert_eq!(parse_recency("2 seconds ago"), 2);
        assert_eq!(parse_recency("1 minute ago"), 60);
        assert_eq!(parse_recency("2 minutes ago"), 120);
        assert_eq!(parse_recency("1 hour ago"), 3600);
        assert_eq!(parse_recency("2 hours ago"), 7200);
        assert_eq!(parse_recency("1 day ago"), 86400);
        assert_eq!(parse_recency("2 days ago"), 172800);
        assert_eq!(parse_recency("1 week ago"), 604800);
        assert_eq!(parse_recency("2 weeks ago"), 1209600);
        assert_eq!(parse_recency("1 month ago"), 2592000);
        assert_eq!(parse_recency("2 months ago"), 5184000);
        assert_eq!(parse_recency("1 year ago"), 31536000);
        assert_eq!(parse_recency("2 years ago"), 63072000);
    }
}
