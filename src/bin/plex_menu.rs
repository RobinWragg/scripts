use std::collections::HashMap;
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(target_os = "macos")]
use block2::RcBlock;
use curl::easy::{Easy, List};
use serde_json::Value;
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
use tray_icon::menu::{CheckMenuItem, ContextMenu, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::TrayIconBuilder;

#[cfg(target_os = "macos")]
use objc2::runtime::AnyObject;
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSFont, NSFontAttributeName, NSImage, NSMenu, NSMenuDidBeginTrackingNotification,
    NSMenuDidEndTrackingNotification, NSStringDrawing,
};
#[cfg(target_os = "macos")]
use objc2_foundation::{
    NSAttributedStringKey, NSDictionary, NSInteger, NSNotification, NSNotificationCenter, NSSize,
    NSString,
};

const QUIT_ITEM_ID: &str = "quit";
const POLL_INTERVAL: Duration = Duration::from_secs(3);
const UI_UPDATE_INTERVAL: Duration = Duration::from_millis(500);
const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:32400";
const MENU_BAR_TITLE_MAX_WIDTH: f64 = 150.0;

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
    user_rating: Option<u8>,
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
    playlist_rating_key: Option<String>,
    included: bool,
    playlist_item_id: Option<String>,
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
    let response = plex_request(config, path, "GET")?;
    serde_json::from_slice(&response).map_err(|error| error.to_string())
}

fn plex_request(config: &PlexConfig, path: &str, method: &str) -> Result<Vec<u8>, String> {
    let url = format!("{}{}", config.server_url, path);
    let mut easy = Easy::new();
    easy.url(&url).map_err(|error| error.to_string())?;
    easy.custom_request(method)
        .map_err(|error| error.to_string())?;
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

    Ok(response)
}

fn metadata_array(value: &Value) -> &[Value] {
    value["MediaContainer"]["Metadata"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn string_field(value: &Value, name: &str) -> String {
    match &value[name] {
        Value::String(value) => value.trim().to_owned(),
        Value::Number(value) => value.to_string(),
        _ => String::new(),
    }
}

fn track_from_metadata(value: &Value) -> Track {
    let rating_key = string_field(value, "ratingKey");
    let user_rating = match &value["userRating"] {
        Value::Number(value) => value.as_f64().map(|rating| rating.round() as u8),
        Value::String(value) => value
            .trim()
            .parse::<f64>()
            .ok()
            .map(|rating| rating.round() as u8),
        _ => None,
    };
    Track {
        rating_key: (!rating_key.is_empty()).then_some(rating_key),
        title: string_field(value, "title"),
        artist: string_field(value, "grandparentTitle"),
        album: string_field(value, "parentTitle"),
        user_rating,
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
        .find(|session| session_is_playing(session))?;

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

        let playlist_rating_key = string_field(playlist, "ratingKey");
        if playlist_rating_key.is_empty() {
            memberships
                .entry(normalized(&playlist_title))
                .or_insert_with(|| PlaylistMembership {
                    name: playlist_title,
                    playlist_rating_key: None,
                    included: false,
                    playlist_item_id: None,
                });
            continue;
        };

        let items =
            match plex_get_json(config, &format!("/playlists/{}/items", playlist_rating_key)) {
                Ok(items) => items,
                Err(error) => {
                    eprintln!("Failed to load Plex playlist {playlist_title:?}: {error}");
                    memberships
                        .entry(normalized(&playlist_title))
                        .or_insert_with(|| PlaylistMembership {
                            name: playlist_title,
                            playlist_rating_key: Some(playlist_rating_key.clone()),
                            included: false,
                            playlist_item_id: None,
                        });
                    continue;
                }
            };

        let playlist_item = metadata_array(&items)
            .iter()
            .find(|playlist_track| track_matches(track, &track_from_metadata(playlist_track)));
        let included = playlist_item.is_some();
        let playlist_item_id = playlist_item.and_then(|item| {
            let item_id = string_field(item, "playlistItemID");
            (!item_id.is_empty()).then_some(item_id)
        });

        let key = normalized(&playlist_title);
        memberships
            .entry(key)
            .and_modify(|membership| {
                membership.included |= included;
                if playlist_item_id.is_some() {
                    membership.playlist_item_id = playlist_item_id.clone();
                }
            })
            .or_insert_with(|| PlaylistMembership {
                name: playlist_title,
                playlist_rating_key: Some(playlist_rating_key),
                included,
                playlist_item_id,
            });
    }

    let mut memberships: Vec<PlaylistMembership> = memberships.into_values().collect();
    memberships.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(memberships)
}

fn percent_encode_query(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{:02X}", byte));
        }
    }
    encoded
}

fn add_track_to_playlist(
    config: &PlexConfig,
    track: &Track,
    playlist_rating_key: &str,
) -> Result<(), String> {
    let track_rating_key = track
        .rating_key
        .as_deref()
        .ok_or_else(|| "Active track has no rating key".to_owned())?;
    let identity = plex_get_json(config, "/identity")?;
    let machine_identifier = string_field(&identity["MediaContainer"], "machineIdentifier");
    if machine_identifier.is_empty() {
        return Err("Plex server identity has no machine identifier".to_owned());
    }

    let uri = format!(
        "server://{machine_identifier}/com.plexapp.plugins.library/library/metadata/{track_rating_key}"
    );
    plex_request(
        config,
        &format!(
            "/playlists/{playlist_rating_key}/items?uri={}",
            percent_encode_query(&uri)
        ),
        "PUT",
    )?;
    Ok(())
}

fn remove_track_from_playlist(
    config: &PlexConfig,
    playlist_rating_key: &str,
    playlist_item_id: &str,
) -> Result<(), String> {
    plex_request(
        config,
        &format!("/playlists/{playlist_rating_key}/items/{playlist_item_id}"),
        "DELETE",
    )?;
    Ok(())
}

fn set_track_rating(config: &PlexConfig, track: &Track, rating: i16) -> Result<(), String> {
    let track_rating_key = track
        .rating_key
        .as_deref()
        .ok_or_else(|| "Active track has no rating key".to_owned())?;
    plex_request(
        config,
        &format!(
            "/:/rate?identifier=com.plexapp.plugins.library&key={track_rating_key}&rating={rating}"
        ),
        "PUT",
    )?;
    Ok(())
}

fn update_star_menu(
    unrated: &CheckMenuItem,
    one_star: &CheckMenuItem,
    three_stars: &CheckMenuItem,
    five_stars: &CheckMenuItem,
    track: Option<&Track>,
) {
    let rating = track.and_then(|track| track.user_rating);
    let selected_rating = selected_star_rating(rating);
    unrated.set_checked(is_unrated_rating(rating));
    one_star.set_checked(selected_rating == Some(2));
    three_stars.set_checked(selected_rating == Some(6));
    five_stars.set_checked(selected_rating == Some(10));

    let enabled = track.is_some() && track.and_then(|track| track.rating_key.as_ref()).is_some();
    unrated.set_enabled(enabled);
    one_star.set_enabled(enabled);
    three_stars.set_enabled(enabled);
    five_stars.set_enabled(enabled);
}

fn refresh_menu_contents(
    menu: &Menu,
    song_item: &MenuItem,
    artist_item: &MenuItem,
    album_item: &MenuItem,
    status_item: &MenuItem,
    unrated_item: &CheckMenuItem,
    one_star_item: &CheckMenuItem,
    three_star_item: &CheckMenuItem,
    five_star_item: &CheckMenuItem,
    playlists_item: &MenuItem,
    playlist_membership_items: &mut Vec<MenuItem>,
    playback: &PlaybackState,
) {
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
            menu,
            playlists_item,
            playlist_membership_items,
            &playback.playlists,
        );
        update_star_menu(
            unrated_item,
            one_star_item,
            three_star_item,
            five_star_item,
            Some(track),
        );
    } else {
        song_item.set_text("Song: none");
        artist_item.set_text("Artist: unknown");
        album_item.set_text("Album: unknown");
        update_playlist_menu(menu, playlists_item, playlist_membership_items, &[]);
        update_star_menu(
            unrated_item,
            one_star_item,
            three_star_item,
            five_star_item,
            None,
        );
    }

    status_item.set_text(format!("Status: {}", playback.status));
}

fn selected_star_rating(rating: Option<u8>) -> Option<u8> {
    match rating {
        Some(2) => Some(2),
        Some(6) => Some(6),
        Some(10) => Some(10),
        _ => None,
    }
}

fn is_unrated_rating(rating: Option<u8>) -> bool {
    rating.is_none() || rating == Some(0)
}

fn rating_after_update(rating: i16) -> Option<u8> {
    (rating != -1).then_some(rating as u8)
}

fn playlist_menu_lines(playlists: &[PlaylistMembership]) -> Vec<String> {
    playlists
        .iter()
        .map(|playlist| {
            format!(
                "{}: {}",
                playlist.name,
                if playlist.included {
                    "Added"
                } else {
                    "Removed"
                }
            )
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn set_playlist_laptop_symbol(menu: &Menu, position: usize) {
    let symbol_name = NSString::from_str("laptopcomputer");
    let symbol = NSImage::imageWithSystemSymbolName_accessibilityDescription(&symbol_name, None);
    let Some(symbol) = symbol else {
        return;
    };

    symbol.setSize(NSSize::new(18.0, 18.0));

    let ns_menu = menu.ns_menu();
    if ns_menu.is_null() {
        return;
    }

    let ns_menu = unsafe { &*(ns_menu as *const NSMenu) };
    let item = ns_menu.itemAtIndex(position as NSInteger);
    if let Some(item) = item {
        item.setImage(Some(&symbol));
    }
}

fn playlist_should_show_laptop_symbol(name: &str) -> bool {
    name == "hacky hack"
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
        let playlist = &playlists[index];
        let item = match &playlist.playlist_rating_key {
            Some(rating_key) => {
                MenuItem::with_id(format!("playlist-toggle:{rating_key}"), line, true, None)
            }
            None => MenuItem::new(line, false, None),
        };
        let position = 11 + index;
        if let Err(error) = menu.insert(&item, position) {
            eprintln!("Failed to add Plex playlist menu item: {error}");
            continue;
        }
        #[cfg(target_os = "macos")]
        if playlist_should_show_laptop_symbol(&playlist.name) {
            set_playlist_laptop_symbol(menu, position);
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
        Some(track) => truncate_to_menu_bar_width(&track.title),
        None => "Plex".to_owned(),
    }
}

fn should_update_menu(
    last_playback: Option<&PlaybackState>,
    playback: &PlaybackState,
    menu_is_open: bool,
    refresh_after_close: bool,
) -> bool {
    !menu_is_open && (refresh_after_close || last_playback != Some(playback))
}

fn truncate_by_width<F>(value: &str, max_width: f64, measure: F) -> String
where
    F: Fn(&str) -> f64,
{
    if measure(value) <= max_width {
        return value.to_owned();
    }

    const ELLIPSIS: &str = "...";
    let ellipsis_width = measure(ELLIPSIS);
    if ellipsis_width >= max_width {
        return ELLIPSIS.to_owned();
    }

    let available_width = max_width - ellipsis_width;
    let mut prefix = String::new();
    for character in value.chars() {
        let mut candidate = prefix.clone();
        candidate.push(character);
        if measure(&candidate) > available_width {
            break;
        }
        prefix = candidate;
    }

    format!("{prefix}{ELLIPSIS}")
}

#[cfg(target_os = "macos")]
fn menu_bar_text_width(value: &str) -> f64 {
    let ns_value = NSString::from_str(value);
    let font = NSFont::menuBarFontOfSize(0.0);
    let font_object = font.into_super().into_super();
    let font_attribute_name = unsafe { NSFontAttributeName };
    let attributes = NSDictionary::<NSAttributedStringKey, AnyObject>::from_retained_objects(
        &[font_attribute_name],
        &[font_object],
    );

    unsafe { ns_value.sizeWithAttributes(Some(&attributes)).width }
}

fn truncate_to_menu_bar_width(value: &str) -> String {
    truncate_by_width(value, MENU_BAR_TITLE_MAX_WIDTH, menu_bar_text_width)
}

fn print_text_diagnostics(label: &str, value: &str) {
    let suspicious_characters: Vec<String> = value
        .chars()
        .filter(|character| {
            character.is_control()
                || matches!(character, '\u{fffd}')
                || (*character as u32) == 0x2028
                || (*character as u32) == 0x2029
                || (*character as u32) == 0x202a
                || (*character as u32) == 0x202b
                || (*character as u32) == 0x202c
                || (*character as u32) == 0x202d
                || (*character as u32) == 0x202e
                || (*character as u32) == 0x2066
                || (*character as u32) == 0x2067
                || (*character as u32) == 0x2068
                || (*character as u32) == 0x2069
        })
        .map(|character| format!("U+{:04X}", character as u32))
        .collect();
    let byte_hex: String = value
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<String>>()
        .join(" ");

    println!(
        "[plex_menu] {label}: debug={value:?}, chars={}, bytes={}, suspicious_chars={:?}, byte_hex={byte_hex}",
        value.chars().count(),
        value.len(),
        suspicious_characters,
    );
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
        let mut previous_status: Option<String> = None;
        let mut previous_track: Option<Track> = None;
        while !worker_stop.load(Ordering::Relaxed) {
            let playback = poll_playback(&load_plex_config());
            let changed = previous_status.as_deref() != Some(playback.status.as_str())
                || previous_track.as_ref() != playback.track.as_ref();
            if changed {
                println!(
                    "[plex_menu] playback changed: status={:?}, track={:?}",
                    playback.status, playback.track
                );
                if let Some(track) = &playback.track {
                    print_text_diagnostics("active title", &track.title);
                    print_text_diagnostics("active artist", &track.artist);
                    print_text_diagnostics("active album", &track.album);
                    println!(
                        "[plex_menu] active track fields: rating_key={:?}, user_rating={:?}",
                        track.rating_key, track.user_rating
                    );
                    for playlist in &playback.playlists {
                        println!(
                            "[plex_menu] playlist: name={:?}, rating_key={:?}, included={}, item_id={:?}",
                            playlist.name,
                            playlist.playlist_rating_key,
                            playlist.included,
                            playlist.playlist_item_id
                        );
                        print_text_diagnostics("playlist name", &playlist.name);
                    }
                } else {
                    println!("[plex_menu] no active track");
                }
                previous_status = Some(playback.status.clone());
                previous_track = playback.track.clone();
            }
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
    let unrated_item = CheckMenuItem::with_id("rating:-1", "-", false, false, None);
    let one_star_item = CheckMenuItem::with_id("rating:2", "★", false, false, None);
    let three_star_item = CheckMenuItem::with_id("rating:6", "★★★", false, false, None);
    let five_star_item = CheckMenuItem::with_id("rating:10", "★★★★★", false, false, None);
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
        &unrated_item,
        &one_star_item,
        &three_star_item,
        &five_star_item,
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

    let menu_is_open = Arc::new(AtomicBool::new(false));
    let menu_needs_refresh = Arc::new(AtomicBool::new(false));
    let notification_center = NSNotificationCenter::defaultCenter();

    let begin_name = unsafe { NSMenuDidBeginTrackingNotification };
    let begin_menu_observer = {
        let menu_is_open = Arc::clone(&menu_is_open);
        let block = RcBlock::new(move |_notification: NonNull<NSNotification>| {
            menu_is_open.store(true, Ordering::Relaxed);
        });
        unsafe {
            notification_center.addObserverForName_object_queue_usingBlock(
                Some(begin_name),
                None,
                None,
                &block,
            )
        }
    };

    let end_name = unsafe { NSMenuDidEndTrackingNotification };
    let end_menu_observer = {
        let menu_is_open = Arc::clone(&menu_is_open);
        let menu_needs_refresh = Arc::clone(&menu_needs_refresh);
        let block = RcBlock::new(move |_notification: NonNull<NSNotification>| {
            menu_is_open.store(false, Ordering::Relaxed);
            menu_needs_refresh.store(true, Ordering::Relaxed);
        });
        unsafe {
            notification_center.addObserverForName_object_queue_usingBlock(
                Some(end_name),
                None,
                None,
                &block,
            )
        }
    };
    let menu_observers = (begin_menu_observer, end_menu_observer);

    let mut should_exit = false;
    let mut last_rendered_playback: Option<PlaybackState> = None;
    event_loop.run(move |_event, _event_loop, control_flow| {
        let _menu_observers = &menu_observers;
        while let Ok(menu_event) = MenuEvent::receiver().try_recv() {
            match menu_event.id().as_ref() {
                QUIT_ITEM_ID => {
                    stop.store(true, Ordering::Relaxed);
                    should_exit = true;
                }
                rating_id if rating_id.starts_with("rating:") => {
                    if let Some(rating) = rating_id.strip_prefix("rating:") {
                        if let Ok(rating) = rating.parse::<i16>() {
                            if matches!(rating, -1 | 2 | 6 | 10) {
                                let track = {
                                    let playback = state.lock().unwrap();
                                    playback.track.clone()
                                };
                                if let Some(track) = track {
                                    let config = load_plex_config();
                                    match set_track_rating(&config, &track, rating) {
                                        Ok(()) => {
                                            let mut playback = state.lock().unwrap();
                                            playback.status = "Connected".to_owned();
                                            if let Some(active_track) = playback.track.as_mut() {
                                                active_track.user_rating =
                                                    rating_after_update(rating);
                                            }
                                        }
                                        Err(error) => {
                                            let mut playback = state.lock().unwrap();
                                            playback.status =
                                                format!("Rating update failed: {error}");
                                        }
                                    }
                                    menu_needs_refresh.store(true, Ordering::Relaxed);
                                }
                            }
                        }
                    }
                }
                playlist_id if playlist_id.starts_with("playlist-toggle:") => {
                    if let Some(rating_key) = playlist_id.strip_prefix("playlist-toggle:") {
                        let snapshot = {
                            let playback = state.lock().unwrap();
                            playback.track.clone().and_then(|track| {
                                playback
                                    .playlists
                                    .iter()
                                    .find(|playlist| {
                                        playlist.playlist_rating_key.as_deref() == Some(rating_key)
                                    })
                                    .cloned()
                                    .map(|playlist| (track, playlist))
                            })
                        };

                        if let Some((track, playlist)) = snapshot {
                            let config = load_plex_config();
                            let result = if playlist.included {
                                match playlist.playlist_item_id.as_deref() {
                                    Some(item_id) => {
                                        remove_track_from_playlist(&config, rating_key, item_id)
                                    }
                                    None => {
                                        Err("Plex did not provide this playlist item ID".to_owned())
                                    }
                                }
                            } else {
                                add_track_to_playlist(&config, &track, rating_key)
                            };

                            match result {
                                Ok(()) => {
                                    let mut playback = state.lock().unwrap();
                                    playback.status = "Connected".to_owned();
                                    if let Some(membership) =
                                        playback.playlists.iter_mut().find(|membership| {
                                            membership.playlist_rating_key.as_deref()
                                                == Some(rating_key)
                                        })
                                    {
                                        membership.included = !membership.included;
                                        if !membership.included {
                                            membership.playlist_item_id = None;
                                        }
                                    }
                                }
                                Err(error) => {
                                    let mut playback = state.lock().unwrap();
                                    playback.status = format!("Playlist update failed: {error}");
                                }
                            }
                            menu_needs_refresh.store(true, Ordering::Relaxed);
                        }
                    }
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
            let menu_is_open = menu_is_open.load(Ordering::Relaxed);
            let refresh_after_close = if menu_is_open {
                false
            } else {
                menu_needs_refresh.swap(false, Ordering::Relaxed)
            };
            if should_update_menu(
                last_rendered_playback.as_ref(),
                &playback,
                menu_is_open,
                refresh_after_close,
            ) {
                refresh_menu_contents(
                    &menu,
                    &song_item,
                    &artist_item,
                    &album_item,
                    &status_item,
                    &unrated_item,
                    &one_star_item,
                    &three_star_item,
                    &five_star_item,
                    &playlists_item,
                    &mut playlist_membership_items,
                    &playback,
                );
                let menu_bar_title = menu_bar_title(&playback);
                print_text_diagnostics("menu bar title before set_title", &menu_bar_title);
                tray.set_title(Some(menu_bar_title));
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
                last_rendered_playback = Some(playback);
            }
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
                  "userRating": 6,
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
                user_rating: Some(6),
            })
        );
    }

    #[test]
    fn ignores_paused_tracks() {
        let json: Value = serde_json::from_str(
            r#"{
              "MediaContainer": {
                "Metadata": [{
                  "type": "track",
                  "ratingKey": "456",
                  "title": "Paused Song",
                  "grandparentTitle": "Paused Artist",
                  "parentTitle": "Paused Album",
                  "Player": {"state": "paused"}
                }]
              }
            }"#,
        )
        .unwrap();

        assert_eq!(parse_active_track(&json), None);
    }

    #[test]
    fn ignores_stopped_or_stale_sessions() {
        let json: Value = serde_json::from_str(
            r#"{
              "MediaContainer": {
                "Metadata": [{
                  "type": "track",
                  "ratingKey": "789",
                  "title": "Old Song",
                  "grandparentTitle": "Old Artist",
                  "parentTitle": "Old Album",
                  "Player": {"state": "stopped"}
                }]
              }
            }"#,
        )
        .unwrap();

        assert_eq!(parse_active_track(&json), None);
    }

    #[test]
    fn matches_tracks_by_rating_key() {
        let track = Track {
            rating_key: Some("123".to_owned()),
            title: "Other title".to_owned(),
            artist: "Other artist".to_owned(),
            album: "Other album".to_owned(),
            user_rating: None,
        };
        let playlist_track = Track {
            rating_key: Some("123".to_owned()),
            title: "Different metadata title".to_owned(),
            artist: String::new(),
            album: String::new(),
            user_rating: None,
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
            user_rating: None,
        };
        let playlist_track = Track {
            rating_key: None,
            title: "my song".to_owned(),
            artist: "the artist".to_owned(),
            album: "the album".to_owned(),
            user_rating: None,
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
                playlist_rating_key: Some("1".to_owned()),
                included: true,
                playlist_item_id: Some("10".to_owned()),
            },
            PlaylistMembership {
                name: "Road Trip".to_owned(),
                playlist_rating_key: Some("2".to_owned()),
                included: false,
                playlist_item_id: None,
            },
        ];

        assert_eq!(
            playlist_menu_lines(&playlists),
            vec![
                "Favorites: Added".to_owned(),
                "Road Trip: Removed".to_owned()
            ]
        );
        assert!(playlist_menu_lines(&[]).is_empty());
    }

    #[test]
    fn shows_laptop_symbol_only_for_hacky_hack() {
        assert!(playlist_should_show_laptop_symbol("hacky hack"));
        assert!(!playlist_should_show_laptop_symbol("Hacky Hack"));
        assert!(!playlist_should_show_laptop_symbol("Favorites"));
    }

    #[test]
    fn maps_supported_plex_star_ratings() {
        assert_eq!(selected_star_rating(Some(2)), Some(2));
        assert_eq!(selected_star_rating(Some(6)), Some(6));
        assert_eq!(selected_star_rating(Some(10)), Some(10));
        assert_eq!(selected_star_rating(Some(4)), None);
        assert_eq!(selected_star_rating(Some(0)), None);
        assert_eq!(selected_star_rating(None), None);
        assert!(is_unrated_rating(Some(0)));
        assert!(is_unrated_rating(None));
        assert!(!is_unrated_rating(Some(2)));
        assert_eq!(rating_after_update(-1), None);
        assert_eq!(rating_after_update(2), Some(2));
    }

    #[test]
    fn truncates_menu_bar_titles() {
        let measure = |value: &str| value.chars().count() as f64;

        assert_eq!(truncate_by_width("A Song", 45.0, measure), "A Song");
        assert_eq!(
            truncate_by_width("A very long song title", 10.0, measure),
            "A very ..."
        );
    }

    #[test]
    fn only_updates_menu_for_changed_playback_when_closed() {
        let playback = PlaybackState {
            status: "Connected".to_owned(),
            track: Some(Track {
                title: "A Song".to_owned(),
                ..Track::default()
            }),
            playlists: Vec::new(),
        };

        assert!(should_update_menu(None, &playback, false, false));
        assert!(!should_update_menu(
            Some(&playback),
            &playback,
            false,
            false
        ));
        assert!(should_update_menu(Some(&playback), &playback, false, true));
        assert!(!should_update_menu(None, &playback, true, false));
    }
}
