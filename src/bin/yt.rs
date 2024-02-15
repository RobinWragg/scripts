use curl::easy::Easy;
use regex::Regex;
use serde_json::Value;
use std::io::{stdout, Write};

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
    id: String,
    title: String,
    date: String,
}

fn get_video(value: &Value) -> Option<Video> {
    if *value != Value::Null {
        return Some(Video {
            id: value["videoId"].to_string(),
            title: value["title"]["runs"][0]["text"].to_string(),
            date: value["publishedTimeText"]["simpleText"].to_string(),
        });
    }

    return None;
}

fn main() {
    let url = video_list_url("NASA");
    let mut easy = Easy::new();
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
    let page = std::str::from_utf8(&dst).unwrap();
    let json = json_from_page(page);
    let serde_value: Value = serde_json::from_str(&json).unwrap();
    let tabs = &serde_value["contents"]["twoColumnBrowseResultsRenderer"]["tabs"];
    let videos = &tabs[1]["tabRenderer"]["content"]["richGridRenderer"]["contents"];
    let videos = videos.as_array().unwrap();

    let mut results: Vec<Video> = vec![];
    for video in videos {
        let video = &video["richItemRenderer"]["content"]["videoRenderer"];
        if let Some(video) = get_video(video) {
            results.push(video);
        }
    }

    dbg!(&results[0].id);
    dbg!(&results[0].title);
    dbg!(&results[0].date);

    // for c in re.captures_iter(hay) {
    //     let m = c.get(1).unwrap().as_str();
    //     dbg!(m);
    // }
}
