use std::path::Path;
use std::path::PathBuf;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2_app_kit::{NSWorkspace, NSWorkspaceActiveSpaceDidChangeNotification};
use objc2_foundation::NSNotification;
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::TrayIconBuilder;
use vigilia_platform::LidState;

const PAUSE_ITEM_ID: &str = "pause";
const RESET_ITEM_ID: &str = "reset";
const QUIT_ITEM_ID: &str = "quit";
const TIMER_FILE_NAME: &str = "spacetime.elapsed";
const UI_UPDATE_INTERVAL: Duration = Duration::from_millis(100);
const TIMER_SAVE_INTERVAL: Duration = Duration::from_millis(1000);

#[derive(Default)]
struct AppState {
    elapsed: Duration,
    last_tick: Option<Instant>,
    manually_paused: bool,
    wallpaper_name: Option<String>,
    lid_state: Option<LidState>,
}

fn space_id() -> Option<String> {
    wallpaper::get().ok()
}

fn wallpaper_name() -> Option<String> {
    wallpaper::get().ok().and_then(|path| {
        Path::new(&path)
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
    })
}

fn is_paused(state: &AppState) -> bool {
    state.manually_paused
        || state.wallpaper_name.as_deref() != Some("dgnfog.png")
        || state.lid_state == Some(LidState::Closed)
}

fn lid_status(lid_state: Option<LidState>) -> &'static str {
    match lid_state {
        Some(LidState::Open) => "Lid: open",
        Some(LidState::Closed) => "Lid: closed",
        None => "Lid: unknown",
    }
}

fn format_duration(duration: Duration, long_form: bool) -> String {
    let seconds = duration.as_secs();
    let hours = seconds / 3600;
    let minutes = (seconds / 60) % 60;
    let remaining_seconds = seconds % 60;
    if long_form {
        format!("{hours}h {minutes}m {remaining_seconds}s")
    } else {
        format!("{hours}:{minutes}")
    }
}

fn status_title(state: &AppState) -> String {
    let duration = format_duration(state.elapsed, false);
    if is_paused(state) {
        format!("⏸ {duration}")
    } else {
        format!("▶ {duration}")
    }
}

fn elapsed_delta(last_tick: Instant, now: Instant) -> Duration {
    let delta = now.saturating_duration_since(last_tick);
    if delta > Duration::from_secs(3) {
        Duration::ZERO
    } else {
        delta
    }
}

fn timer_file_path() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(Path::to_path_buf))
        .map(|directory| directory.join(TIMER_FILE_NAME))
}

fn load_elapsed(path: Option<&Path>) -> Duration {
    let Some(path) = path else {
        return Duration::ZERO;
    };

    match std::fs::read_to_string(path) {
        Ok(contents) => match contents.trim().parse::<u64>() {
            Ok(seconds) => Duration::from_secs(seconds),
            Err(error) => {
                eprintln!("Failed to parse timer file {}: {error}", path.display());
                Duration::ZERO
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Duration::ZERO,
        Err(error) => {
            eprintln!("Failed to load timer file {}: {error}", path.display());
            Duration::ZERO
        }
    }
}

fn save_elapsed(path: Option<&Path>, elapsed: Duration) {
    let Some(path) = path else {
        return;
    };

    if let Err(error) = std::fs::write(path, elapsed.as_secs().to_string()) {
        eprintln!("Failed to save timer file {}: {error}", path.display());
    }
}

fn main() {
    let space_id = space_id();
    let timer_file = timer_file_path();

    let state = Arc::new(Mutex::new(AppState {
        elapsed: load_elapsed(timer_file.as_deref()),
        last_tick: Some(Instant::now()),
        ..AppState::default()
    }));

    if let Some(wallpaper_name) = wallpaper_name() {
        state.lock().unwrap().wallpaper_name = Some(wallpaper_name);
    }

    let stop = Arc::new(AtomicBool::new(false));

    let worker_state = Arc::clone(&state);
    let worker_stop = Arc::clone(&stop);
    let worker_timer_file = timer_file.clone();
    std::thread::spawn(move || {
        let platform = vigilia_platform::native();
        let mut lid_events = platform.lid_events();

        while !worker_stop.load(Ordering::Relaxed) {
            if let Some(events) = &mut lid_events {
                while let Ok(lid_state) = events.try_recv() {
                    worker_state.lock().unwrap().lid_state = Some(lid_state);
                }
            }

            let elapsed = worker_state.lock().unwrap().elapsed;
            save_elapsed(worker_timer_file.as_deref(), elapsed);

            std::thread::sleep(TIMER_SAVE_INTERVAL);
        }
    });

    let mut event_loop = EventLoop::new();
    event_loop.set_activation_policy(ActivationPolicy::Accessory);
    event_loop.set_dock_visibility(false);

    let notification_state = Arc::clone(&state);
    let space_change_block = RcBlock::new(move |_notification: NonNull<NSNotification>| {
        if let Some(wallpaper_name) = wallpaper_name() {
            notification_state.lock().unwrap().wallpaper_name = Some(wallpaper_name);
        }
    });
    let notification_center = NSWorkspace::sharedWorkspace().notificationCenter();
    let _space_change_observer = unsafe {
        notification_center.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceActiveSpaceDidChangeNotification),
            None,
            None,
            &space_change_block,
        )
    };

    let menu = Menu::new();
    let time_item = MenuItem::with_id("time", "Time: 0h 0m 0s", false, None);
    let space_item = MenuItem::with_id(
        "space",
        format!(
            "Space: {}",
            space_id
                .as_deref()
                .and_then(|path| Path::new(path).file_name())
                .and_then(|name| name.to_str())
                .unwrap_or("unknown")
        ),
        false,
        None,
    );
    let wallpaper_item = MenuItem::with_id("wallpaper", "Wallpaper: unknown", false, None);
    let lid_item = MenuItem::with_id("lid", lid_status(None), false, None);
    let pause_item = MenuItem::with_id(PAUSE_ITEM_ID, "Pause", true, None);
    let reset_item = MenuItem::with_id(RESET_ITEM_ID, "Reset", true, None);
    let quit_item = MenuItem::with_id(QUIT_ITEM_ID, "Quit", true, None);
    let separator_before_actions = PredefinedMenuItem::separator();
    let separator_before_quit = PredefinedMenuItem::separator();

    menu.append_items(&[
        &time_item,
        &space_item,
        &wallpaper_item,
        &lid_item,
        &separator_before_actions,
        &pause_item,
        &reset_item,
        &separator_before_quit,
        &quit_item,
    ])
    .expect("failed to build the menu bar menu");

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_title(status_title(&state.lock().unwrap()))
        .with_tooltip("Spacetime")
        .build()
        .expect("failed to create the macOS menu bar item");

    let mut should_exit = false;
    event_loop.run(move |_event, _event_loop, control_flow| {
        while let Ok(menu_event) = MenuEvent::receiver().try_recv() {
            let mut app_state = state.lock().unwrap();
            match menu_event.id().as_ref() {
                PAUSE_ITEM_ID => {
                    app_state.manually_paused = !app_state.manually_paused;
                    app_state.last_tick = Some(Instant::now());
                    pause_item.set_text(if app_state.manually_paused {
                        "Resume"
                    } else {
                        "Pause"
                    });
                    save_elapsed(timer_file.as_deref(), app_state.elapsed);
                }
                RESET_ITEM_ID => {
                    app_state.elapsed = Duration::ZERO;
                    app_state.last_tick = Some(Instant::now());
                    save_elapsed(timer_file.as_deref(), app_state.elapsed);
                }
                QUIT_ITEM_ID => {
                    stop.store(true, Ordering::Relaxed);
                    save_elapsed(timer_file.as_deref(), app_state.elapsed);
                    should_exit = true;
                }
                _ => {}
            }
            drop(app_state);
        }

        if !should_exit
            && matches!(
                _event,
                Event::NewEvents(StartCause::Init)
                    | Event::NewEvents(StartCause::ResumeTimeReached { .. })
                    | Event::NewEvents(StartCause::Poll)
            )
        {
            let now = Instant::now();
            let mut app_state = state.lock().unwrap();

            if let Some(wallpaper_name) = app_state.wallpaper_name.as_deref() {
                wallpaper_item.set_text(format!("Wallpaper: {wallpaper_name}"));
            } else {
                wallpaper_item.set_text("Wallpaper: unknown");
            }
            lid_item.set_text(lid_status(app_state.lid_state));

            if is_paused(&app_state) {
                app_state.last_tick = Some(now);
            } else if let Some(last_tick) = app_state.last_tick {
                app_state.elapsed = app_state
                    .elapsed
                    .saturating_add(elapsed_delta(last_tick, now));
                app_state.last_tick = Some(now);
            } else {
                app_state.last_tick = Some(now);
            }

            let title = status_title(&app_state);
            time_item.set_text(format!(
                "Time: {}",
                format_duration(app_state.elapsed, true)
            ));
            tray.set_title(Some(title));
            let _ = tray.set_tooltip(Some(format!(
                "Spacetime — {}",
                format_duration(app_state.elapsed, true)
            )));
        }

        if should_exit {
            *control_flow = ControlFlow::Exit;
        } else {
            *control_flow = ControlFlow::WaitUntil(Instant::now() + UI_UPDATE_INTERVAL);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_elapsed_duration() {
        let duration = Duration::from_secs(3 * 3600 + 12 * 60 + 7);
        assert_eq!(format_duration(duration, true), "3h 12m 7s");
    }

    #[test]
    fn discards_elapsed_delta_over_two_seconds() {
        let now = Instant::now();

        assert_eq!(
            elapsed_delta(now - Duration::from_secs(1), now),
            Duration::from_secs(1)
        );
        assert_eq!(
            elapsed_delta(now - Duration::from_secs(2), now),
            Duration::from_secs(2)
        );
        assert_eq!(
            elapsed_delta(now - Duration::from_secs(60), now),
            Duration::ZERO
        );
    }

    #[test]
    fn pauses_for_manual_pause_wallpaper_or_closed_lid() {
        assert!(is_paused(&AppState {
            manually_paused: true,
            ..AppState::default()
        }));
        assert!(is_paused(&AppState {
            wallpaper_name: Some("other.png".to_string()),
            ..AppState::default()
        }));
        assert!(is_paused(&AppState {
            wallpaper_name: Some("dgnfog.png".to_string()),
            lid_state: Some(LidState::Closed),
            ..AppState::default()
        }));
        assert!(!is_paused(&AppState {
            wallpaper_name: Some("dgnfog.png".to_string()),
            lid_state: Some(LidState::Open),
            ..AppState::default()
        }));
    }

    #[test]
    fn saves_and_loads_elapsed_duration() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(TIMER_FILE_NAME);
        let elapsed = Duration::from_secs(3675);

        save_elapsed(Some(&path), elapsed);

        assert_eq!(load_elapsed(Some(&path)), elapsed);
    }
}
