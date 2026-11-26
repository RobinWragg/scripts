use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use curl::easy::{Easy, List};
use serde_json::Value;
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::TrayIconBuilder;

const QUIT_ITEM_ID: &str = "quit";
const POLL_INTERVAL: Duration = Duration::from_secs(3);
const UI_UPDATE_INTERVAL: Duration = Duration::from_millis(500);
const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:32400";

#[derive(Clone, Default)]
struct PlexConfig {
    server_url: String,
    token: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Track {
    rating_key: Option<String>,
    title: String,
    artist: String,
    album: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct PlaybackState {
    status: String,
    track: Option<Track>,
    playlists: Vec<PlaylistMembership>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct PlaylistMembership {
    name: String,
    included: bool,
}

fn parse_dotenv(contents: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }

        let value = value.trim();
        let value = if value.len() >= 2
            && ((value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\'')))
        {
            &value[1..value.len() - 1]
        } else {
            value
        };
        values.insert(key.to_owned(), value.to_owned());
    }

    values
}

fn find_env_path() -> Option<PathBuf> {
    let manifest_env = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".env");
    if manifest_env.is_file() {
        return Some(manifest_env);
    }

    if let Ok(cwd) = std::env::current_dir() {
        let cwd_env = cwd.join(".env");
        if cwd_env.is_file() {
            return Some(cwd_env);
        }

        let mut directory = cwd.as_path();
        while let Some(parent) = directory.parent() {
            let candidate = parent.join(".env");
            if candidate.is_file() {
                return Some(candidate);
            }
            directory = parent;
        }
    }

    if let Ok(executable) = std::env::current_exe() {
        let mut directory = executable.parent();
        while let Some(current) = directory {
            let candidate = current.join(".env");
            if candidate.is_file() {
                return Some(candidate);
            }
            directory = current.parent();
        }
    }

    None
}

fn load_plex_config() -> PlexConfig {
    let mut config = PlexConfig {
        server_url: DEFAULT_SERVER_URL.to_owned(),
        token: None,
    };

    let mut dotenv = HashMap::new();
    if let Some(path) = find_env_path() {
        if let Ok(contents) = std::fs::read_to_string(&path) {
            dotenv = parse_dotenv(&contents);
        }
    }

    let dotenv_server_url = dotenv
        .get("PLEX_SERVER_URL")
        .or_else(|| dotenv.get("PLEX_URL"))
        .or_else(|| dotenv.get("PLEX_SERVER"));
    if let Some(server_url) = dotenv_server_url {
        let server_url = server_url.trim().trim_end_matches('/');
        if !server_url.is_empty() {
            config.server_url = server_url.to_owned();
        }
    }
    let dotenv_token = dotenv
        .get("PLEX_TOKEN")
        .or_else(|| dotenv.get("PLEX_API_TOKEN"));
    if let Some(token) = dotenv_token {
        let token = token.trim();
        if !token.is_empty() {
            config.token = Some(token.to_owned());
        }
    }

    if let Ok(server_url) = std::env::var("PLEX_SERVER_URL")
        .or_else(|_| std::env::var("PLEX_URL"))
        .or_else(|_| std::env::var("PLEX_SERVER"))
    {
        let server_url = server_url.trim().trim_end_matches('/');
        if !server_url.is_empty() {
            config.server_url = server_url.to_owned();
        }
    }
    if let Ok(token) = std::env::var("PLEX_TOKEN").or_else(|_| std::env::var("PLEX_API_TOKEN")) {
        let token = token.trim();
        if !token.is_empty() {
            config.token = Some(token.to_owned());
        }
    }

    config
}

fn plex_get_json(config: &PlexConfig, path: &str) -> Result<Value, String> {
    let url = format!("{}{}", config.server_url, path);
    let mut easy = Easy::new();
    easy.url(&url).map_err(|error| error.to_string())?;
    easy.follow_location(true)
        .map_err(|error| error.to_string())?;
    easy.timeout(Duration::from_secs(4))
        .map_err(|error| error.to_string())?;
    easy.useragent("Plex Menu/1.0")
        .map_err(|error| error.to_string())?;

    let mut headers = List::new();
    headers
        .append("Accept: application/json")
        .map_err(|error| error.to_string())?;
    headers
        .append("X-Plex-Client-Identifier: com.robinwragg.plex-menu")
        .map_err(|error| error.to_string())?;
    if let Some(token) = &config.token {
        headers
            .append(&format!("X-Plex-Token: {token}"))
            .map_err(|error| error.to_string())?;
    }
    easy.http_headers(headers)
        .map_err(|error| error.to_string())?;

    let mut response = Vec::new();
    {
        let mut transfer = easy.transfer();
        transfer
            .write_function(|data| {
                response.extend_from_slice(data);
                Ok(data.len())
            })
            .map_err(|error| error.to_string())?;
        transfer.perform().map_err(|error| error.to_string())?;
    }

    let status = easy.response_code().map_err(|error| error.to_string())?;
    if !(200..300).contains(&status) {
        return Err(format!("Plex returned HTTP {status}"));
    }

    serde_json::from_slice(&response).map_err(|error| error.to_string())
}

fn metadata_array(value: &Value) -> &[Value] {
    value["MediaContainer"]["Metadata"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn string_field(value: &Value, name: &str) -> String {
    value[name].as_str().unwrap_or("").trim().to_owned()
}

fn track_from_metadata(value: &Value) -> Track {
    Track {
        rating_key: value["ratingKey"].as_str().map(str::to_owned),
        title: string_field(value, "title"),
        artist: string_field(value, "grandparentTitle"),
        album: string_field(value, "parentTitle"),
    }
}

fn session_is_playing(value: &Value) -> bool {
    value["Player"]["state"].as_str() == Some("playing")
        || value["player"]["state"].as_str() == Some("playing")
}

fn parse_active_track(value: &Value) -> Option<Track> {
    let sessions: Vec<&Value> = metadata_array(value)
        .iter()
        .filter(|session| {
            let media_type = session["type"].as_str();
            media_type.is_none() || media_type == Some("track")
        })
        .collect();

    let track = sessions
        .iter()
        .copied()
        .find(|session| session_is_playing(session))
        .or_else(|| sessions.first().copied())?;

    let track = track_from_metadata(track);
    if track.title.is_empty() {
        None
    } else {
        Some(track)
    }
}

fn normalized(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn track_matches(track: &Track, playlist_track: &Track) -> bool {
    if let (Some(track_key), Some(playlist_key)) = (
        track.rating_key.as_deref(),
        playlist_track.rating_key.as_deref(),
    ) {
        if track_key == playlist_key {
            return true;
        }
    }

    normalized(&track.title) == normalized(&playlist_track.title)
        && (!track.artist.is_empty()
            && normalized(&track.artist) == normalized(&playlist_track.artist))
        && (track.album.is_empty()
            || playlist_track.album.is_empty()
            || normalized(&track.album) == normalized(&playlist_track.album))
}

fn is_smart_playlist(playlist: &Value) -> bool {
    match &playlist["smart"] {
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_i64() != Some(0),
        Value::String(value) => {
            let value = value.trim().to_lowercase();
            matches!(value.as_str(), "1" | "true" | "yes")
        }
        _ => false,
    }
}

fn find_playlists(config: &PlexConfig, track: &Track) -> Result<Vec<PlaylistMembership>, String> {
    let playlists = plex_get_json(config, "/playlists?playlistType=audio")?;
    let mut memberships: HashMap<String, PlaylistMembership> = HashMap::new();

    for playlist in metadata_array(&playlists) {
        if is_smart_playlist(playlist) {
            continue;
        }

        let playlist_title = string_field(playlist, "title");
        if playlist_title.is_empty() {
            continue;
        }

        let Some(rating_key) = playlist["ratingKey"].as_str() else {
            memberships
                .entry(normalized(&playlist_title))
                .or_insert_with(|| PlaylistMembership {
                    name: playlist_title,
                    included: false,
                });
            continue;
        };

        let items = match plex_get_json(config, &format!("/playlists/{rating_key}/items")) {
            Ok(items) => items,
            Err(error) => {
                eprintln!("Failed to load Plex playlist {playlist_title:?}: {error}");
                memberships
                    .entry(normalized(&playlist_title))
                    .or_insert_with(|| PlaylistMembership {
                        name: playlist_title,
                        included: false,
                    });
                continue;
            }
        };

        let included = metadata_array(&items)
            .iter()
            .map(track_from_metadata)
            .any(|playlist_track| track_matches(track, &playlist_track));

        let key = normalized(&playlist_title);
        memberships
            .entry(key)
            .and_modify(|membership| membership.included |= included)
            .or_insert_with(|| PlaylistMembership {
                name: playlist_title,
                included,
            });
    }

    let mut memberships: Vec<PlaylistMembership> = memberships.into_values().collect();
    memberships.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(memberships)
}

fn playlist_menu_lines(playlists: &[PlaylistMembership]) -> Vec<String> {
    playlists
        .iter()
        .map(|playlist| format!("{}={}", playlist.name, playlist.included))
        .collect()
}

fn update_playlist_menu(
    menu: &Menu,
    header: &MenuItem,
    membership_items: &mut Vec<MenuItem>,
    playlists: &[PlaylistMembership],
) {
    for item in membership_items.iter() {
        let _ = menu.remove(item);
    }
    membership_items.clear();

    let lines = playlist_menu_lines(playlists);
    if lines.is_empty() {
        header.set_text("Playlists: none");
        return;
    }

    header.set_text("Playlists:");
    for (index, line) in lines.iter().enumerate() {
        let item = MenuItem::new(line, false, None);
        if let Err(error) = menu.insert(&item, 6 + index) {
            eprintln!("Failed to add Plex playlist menu item: {error}");
            continue;
        }
        membership_items.push(item);
    }
}

fn poll_playback(config: &PlexConfig) -> PlaybackState {
    let sessions = match plex_get_json(config, "/status/sessions") {
        Ok(sessions) => sessions,
        Err(error) => {
            return PlaybackState {
                status: format!("Plex unavailable: {error}"),
                ..PlaybackState::default()
            };
        }
    };

    let Some(track) = parse_active_track(&sessions) else {
        return PlaybackState {
            status: "No active track".to_owned(),
            track: None,
            playlists: Vec::new(),
        };
    };

    let playlists = match find_playlists(config, &track) {
        Ok(playlists) => playlists,
        Err(error) => {
            eprintln!("Failed to load Plex playlists: {error}");
            Vec::new()
        }
    };

    PlaybackState {
        status: "Connected".to_owned(),
        track: Some(track),
        playlists,
    }
}

fn menu_bar_title(playback: &PlaybackState) -> String {
    match &playback.track {
        Some(track) => truncate_chars(&track.title, 45),
        None => "Plex".to_owned(),
    }
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let prefix: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{prefix}...")
    } else {
        prefix
    }
}

fn main() {
    let state = Arc::new(Mutex::new(PlaybackState {
        status: "Connecting".to_owned(),
        ..PlaybackState::default()
    }));
    let stop = Arc::new(AtomicBool::new(false));

    let worker_state = Arc::clone(&state);
    let worker_stop = Arc::clone(&stop);
    std::thread::spawn(move || {
        while !worker_stop.load(Ordering::Relaxed) {
            let playback = poll_playback(&load_plex_config());
            *worker_state.lock().unwrap() = playback;
            std::thread::sleep(POLL_INTERVAL);
        }
    });

    let mut event_loop = EventLoop::new();
    event_loop.set_activation_policy(ActivationPolicy::Accessory);
    event_loop.set_dock_visibility(false);

    let song_item = MenuItem::with_id("song", "Song: none", false, None);
    let artist_item = MenuItem::with_id("artist", "Artist: none", false, None);
    let album_item = MenuItem::with_id("album", "Album: none", false, None);
    let status_item = MenuItem::with_id("status", "Status: Connecting", false, None);
    let playlists_item = MenuItem::with_id("playlists", "Playlists: none", false, None);
    let quit_item = MenuItem::with_id(QUIT_ITEM_ID, "Quit", true, None);
    let mut playlist_membership_items = Vec::<MenuItem>::new();

    let menu = Menu::new();
    menu.append_items(&[
        &song_item,
        &artist_item,
        &album_item,
        &status_item,
        &PredefinedMenuItem::separator(),
        &playlists_item,
        &PredefinedMenuItem::separator(),
        &quit_item,
    ])
    .expect("failed to build the Plex Menu menu");

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu.clone()))
        .with_title("Plex")
        .with_tooltip("Plex Menu")
        .build()
        .expect("failed to create the macOS menu bar item");

    let mut should_exit = false;
    event_loop.run(move |_event, _event_loop, control_flow| {
        while let Ok(menu_event) = MenuEvent::receiver().try_recv() {
            match menu_event.id().as_ref() {
                QUIT_ITEM_ID => {
                    stop.store(true, Ordering::Relaxed);
                    should_exit = true;
                }
                _ => {}
            }
        }

        if !should_exit
            && matches!(
                _event,
                Event::NewEvents(StartCause::Init)
                    | Event::NewEvents(StartCause::ResumeTimeReached { .. })
                    | Event::NewEvents(StartCause::Poll)
            )
        {
            let playback = state.lock().unwrap().clone();

            if let Some(track) = &playback.track {
                song_item.set_text(format!("Song: {}", track.title));
                artist_item.set_text(format!(
                    "Artist: {}",
                    if track.artist.is_empty() {
                        "unknown"
                    } else {
                        &track.artist
                    }
                ));
                album_item.set_text(format!(
                    "Album: {}",
                    if track.album.is_empty() {
                        "unknown"
                    } else {
                        &track.album
                    }
                ));
                update_playlist_menu(
                    &menu,
                    &playlists_item,
                    &mut playlist_membership_items,
                    &playback.playlists,
                );
            } else {
                song_item.set_text("Song: none");
                artist_item.set_text("Artist: unknown");
                album_item.set_text("Album: unknown");
                update_playlist_menu(&menu, &playlists_item, &mut playlist_membership_items, &[]);
            }

            status_item.set_text(format!("Status: {}", playback.status));
            tray.set_title(Some(menu_bar_title(&playback)));
            let tooltip = match &playback.track {
                Some(track) => format!(
                    "Plex Menu — {} — {}",
                    track.title,
                    if track.artist.is_empty() {
                        "unknown artist"
                    } else {
                        &track.artist
                    }
                ),
                None => "Plex Menu".to_owned(),
            };
            let _ = tray.set_tooltip(Some(tooltip));
        }

        if should_exit {
            *control_flow = ControlFlow::Exit;
        } else {
            *control_flow = ControlFlow::WaitUntil(std::time::Instant::now() + UI_UPDATE_INTERVAL);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dotenv_values_and_comments() {
        let values = parse_dotenv(
            r#"
# Plex settings
PLEX_SERVER_URL="http://192.168.1.10:32400"
PLEX_TOKEN='secret'
export PLEX_EXTRA=value
"#,
        );

        assert_eq!(
            values.get("PLEX_SERVER_URL").map(String::as_str),
            Some("http://192.168.1.10:32400")
        );
        assert_eq!(values.get("PLEX_TOKEN").map(String::as_str), Some("secret"));
        assert_eq!(values.get("PLEX_EXTRA").map(String::as_str), Some("value"));
    }

    #[test]
    fn parses_a_playing_track() {
        let json: Value = serde_json::from_str(
            r#"{
              "MediaContainer": {
                "Metadata": [{
                  "type": "track",
                  "ratingKey": "123",
                  "title": "Song",
                  "grandparentTitle": "Artist",
                  "parentTitle": "Album",
                  "Player": {"state": "playing"}
                }]
              }
            }"#,
        )
        .unwrap();

        assert_eq!(
            parse_active_track(&json),
            Some(Track {
                rating_key: Some("123".to_owned()),
                title: "Song".to_owned(),
                artist: "Artist".to_owned(),
                album: "Album".to_owned(),
            })
        );
    }

    #[test]
    fn matches_tracks_by_rating_key() {
        let track = Track {
            rating_key: Some("123".to_owned()),
            title: "Other title".to_owned(),
            artist: "Other artist".to_owned(),
            album: "Other album".to_owned(),
        };
        let playlist_track = Track {
            rating_key: Some("123".to_owned()),
            title: "Different metadata title".to_owned(),
            artist: String::new(),
            album: String::new(),
        };

        assert!(track_matches(&track, &playlist_track));
    }

    #[test]
    fn matches_tracks_by_normalized_title_artist_and_album() {
        let track = Track {
            rating_key: None,
            title: "My Song".to_owned(),
            artist: "The Artist".to_owned(),
            album: "The Album".to_owned(),
        };
        let playlist_track = Track {
            rating_key: None,
            title: "my song".to_owned(),
            artist: "the artist".to_owned(),
            album: "the album".to_owned(),
        };

        assert!(track_matches(&track, &playlist_track));
    }

    #[test]
    fn identifies_smart_playlists() {
        let smart_boolean: Value = serde_json::from_str(r#"{"smart": true}"#).unwrap();
        let smart_string: Value = serde_json::from_str(r#"{"smart": "1"}"#).unwrap();
        let regular_boolean: Value = serde_json::from_str(r#"{"smart": false}"#).unwrap();
        let missing_smart_field: Value = serde_json::from_str(r#"{"title": "Road Trip"}"#).unwrap();

        assert!(is_smart_playlist(&smart_boolean));
        assert!(is_smart_playlist(&smart_string));
        assert!(!is_smart_playlist(&regular_boolean));
        assert!(!is_smart_playlist(&missing_smart_field));
    }

    #[test]
    fn formats_all_playlist_memberships() {
        let playlists = vec![
            PlaylistMembership {
                name: "Favorites".to_owned(),
                included: true,
            },
            PlaylistMembership {
                name: "Road Trip".to_owned(),
                included: false,
            },
        ];

        assert_eq!(
            playlist_menu_lines(&playlists),
            vec!["Favorites=true".to_owned(), "Road Trip=false".to_owned()]
        );
        assert!(playlist_menu_lines(&[]).is_empty());
    }

    #[test]
    fn truncates_menu_bar_titles() {
        assert_eq!(truncate_chars("A Song", 45), "A Song");
        assert_eq!(
            truncate_chars("A very long song title", 10),
            "A very lon..."
        );
    }
}
