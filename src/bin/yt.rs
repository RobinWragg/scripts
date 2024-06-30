use curl::easy::Easy;
use open;
use rayon::prelude::*;
use regex::Regex;
use serde_json::Value;
use std::error::Error;
use std::io::{stdin, stdout, Write};
use std::path::PathBuf;
use std::time::*;

const SECONDS_PER_MINUTE: u64 = 60;
const SECONDS_PER_HOUR: u64 = SECONDS_PER_MINUTE * 60;
const SECONDS_PER_DAY: u64 = SECONDS_PER_HOUR * 24;
const SECONDS_PER_WEEK: u64 = SECONDS_PER_DAY * 7;
const SECONDS_PER_MONTH: u64 = SECONDS_PER_DAY * 30; // Approximate!
const SECONDS_PER_YEAR: u64 = SECONDS_PER_DAY * 365; // Approximate!

fn parse_recency(s: &str, now: &SystemTime) -> Result<SystemTime, Box<dyn Error>> {
    let re = Regex::new(r"(\d+) (\w+?)s? ago")?;
    let captures = re.captures(s).ok_or("captures() call failed")?;

    let base_number = captures[1].parse::<u64>()?;

    let seconds = match &captures[2] {
        "second" => base_number,
        "minute" => base_number * SECONDS_PER_MINUTE,
        "hour" => base_number * SECONDS_PER_HOUR,
        "day" => base_number * SECONDS_PER_DAY,
        "week" => base_number * SECONDS_PER_WEEK,
        "month" => base_number * SECONDS_PER_MONTH,
        "year" => base_number * SECONDS_PER_YEAR,
        _ => unreachable!(),
    };

    Ok(*now - Duration::from_secs(seconds))
}

fn video_list_url(channel_id: &str) -> String {
    "https://www.youtube.com/@".to_owned() + channel_id + "/videos"
}

fn json_from_page(page: &str) -> Option<String> {
    let re = Regex::new(r"var ytInitialData = (.*?);</script>").ok()?;
    Some(re.captures(page)?.get(1)?.as_str().to_string())
}

#[derive(Clone)]
struct Video {
    channel_id: String,
    video_id: String,
    title: String,
    recency: String,
    sys_time: SystemTime,
}

fn get_video(channel_id: &str, value: &Value, now: &SystemTime) -> Result<Video, Box<dyn Error>> {
    let recency = value["publishedTimeText"]["simpleText"]
        .as_str()
        .ok_or("Couldn't get simpleText str")?
        .to_string();
    let sys_time = parse_recency(&recency, now)?;
    let video_id = value["videoId"]
        .as_str()
        .ok_or("Couldn't get videoId str")?
        .to_string();
    let title = value["title"]["runs"][0]["text"]
        .as_str()
        .ok_or("Couldn't get runs str")?
        .to_string();

    return Ok(Video {
        channel_id: channel_id.to_string(),
        video_id,
        title,
        recency,
        sys_time,
    });
}

fn get_channel_videos(channel_id: &str, now: &SystemTime) -> Vec<Video> {
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
        transfer.perform().unwrap(); // note, this failed once.
    }
    assert_eq!(easy.response_code().unwrap(), 200);
    let page = std::str::from_utf8(&dst).unwrap();

    if let Some(json) = json_from_page(page) {
        let serde_value: Value = serde_json::from_str(&json).unwrap();
        let tabs = &serde_value["contents"]["twoColumnBrowseResultsRenderer"]["tabs"];
        let videos = &tabs[1]["tabRenderer"]["content"]["richGridRenderer"]["contents"];
        let videos = videos.as_array().unwrap();

        let mut videos_out = vec![];
        for video_value in videos {
            let video_value = &video_value["richItemRenderer"]["content"]["videoRenderer"];

            // Some nulls are expected.
            if !video_value.is_null() {
                match get_video(channel_id, video_value, now) {
                    Ok(video) => videos_out.push(video),
                    Err(e) => println!("Failed to get video: {} because: {}", video_value, e),
                };
            }
        }
        videos_out
    } else {
        println!("Couldn't get json from page: {}", url);
        vec![]
    }
}

fn get_no_show_list_path() -> PathBuf {
    let exe_path = std::env::current_exe().unwrap();
    let dir = exe_path.parent().unwrap();
    let dir = dir.parent().unwrap();
    let dir = dir.parent().unwrap();
    dir.join("yt_no_show_list.txt")
}

fn main() {
    println!("Getting no-shows...");
    let no_shows_path = get_no_show_list_path();
    let loaded_no_shows = std::fs::read_to_string(&no_shows_path).unwrap();

    let channel_ids = [
        "SSkorkowsky",
        "tested",
        "AbroadinJapan",
        "HealthyGamerGG",
        "kylehill",
        "mcolville",
        "smartereveryday",
        "TechnologyConnections",
        "corkboardsandcuriosities",
        "GameGrumps",
        "Wintergatan",
        "TheGrumps",
        "veritasium",
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
        "andrewhuang",
        "iamjoshuadavies",
        "strandbeestfilm",
        "gendesign4714",
        "TheAlexandrian",
        "StuffMadeHere",
        "theklainc.5724",
        "cijapanese",
        "CosmicPerspective",
        "StrangeParts",
        "helloMCDM",
        "_noisecode",
        "kasso_0318",
        "Onomappu",
        "JonTronShow",
        "NihongoDekita",
        "Mystery_Quest",
        "MarkRober",
        "robscallon",
        "TheGlassCannon",
        "jblow888",
    ];

    struct Channel {
        id: String,
        videos: Vec<Video>,
    }

    let mut channels: Vec<Channel> = vec![];

    println!("Reticulating splines...");
    for id in channel_ids {
        channels.push(Channel {
            id: id.to_string(),
            videos: vec![],
        });
    }

    println!("Curling channels...");
    let now = SystemTime::now(); // Use same 'now' value for all videos
    channels.par_iter_mut().for_each(|channel| {
        channel.videos = get_channel_videos(&channel.id, &now);
    });

    let mut all_videos: Vec<Video> = vec![];

    println!("Sorting videos...");
    for channel in &mut channels {
        all_videos.append(&mut channel.videos);
    }
    all_videos.sort_by_key(|v| v.sys_time);

    let mut unwatched_videos: Vec<Video> = vec![];

    println!("Finding unwatched videos...");
    let mut new_no_shows = String::new();
    for video in &all_videos {
        if loaded_no_shows.contains(&video.video_id) {
            new_no_shows += &video.video_id;
            new_no_shows += "\n";
        } else {
            unwatched_videos.push(video.clone());
        }
    }

    let print_recency = false;

    for i in 0..unwatched_videos.len() {
        if print_recency
            && i > 0
            && unwatched_videos[i]
                .sys_time
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                != unwatched_videos[i - 1]
                    .sys_time
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
        {
            let dur = unwatched_videos[i].sys_time.duration_since(UNIX_EPOCH);
            println!(
                "\n{} {}",
                unwatched_videos[i].recency,
                dur.unwrap().as_secs()
            );
        }

        println!(
            "{} {}\t{}",
            i, unwatched_videos[i].channel_id, unwatched_videos[i].title
        );
    }

    print!(">> ");
    let _ = stdout().flush();

    let mut user_input = String::new();
    stdin().read_line(&mut user_input).expect("...wat?");

    let inputs = user_input.split(" ");

    for i in inputs {
        let re = Regex::new(r"x(\d+)").unwrap();
        if let Some(captures) = re.captures(i) {
            if let Ok(index) = captures[1].parse::<usize>() {
                println!("Removing {}", index);
                new_no_shows += &unwatched_videos[index].video_id;
                new_no_shows += "\n";
            } else {
                panic!();
            }
        } else if let Ok(index) = i.trim().parse::<usize>() {
            new_no_shows += &unwatched_videos[index].video_id;
            new_no_shows += "\n";
            let url =
                "https://www.youtube.com/watch?v=".to_owned() + &unwatched_videos[index].video_id;
            open::that(url).unwrap();
        }
    }

    std::fs::write(no_shows_path, new_no_shows).unwrap();
}
