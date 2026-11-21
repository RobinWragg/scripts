use std::io::{self, Write};
use std::path::Path;
use std::process;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::*;

use chrono::Duration;
use crossterm::event::{self, KeyCode, KeyModifiers};
use crossterm::terminal;
use vigilia_platform::LidState;

struct RawMode;

#[derive(Default)]
struct DisplayState {
    wallpaper_name: Option<String>,
    lid_state: Option<LidState>,
}

impl RawMode {
    fn enable() -> Self {
        terminal::enable_raw_mode().expect("failed to enable raw terminal mode");
        Self
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        terminal::disable_raw_mode().expect("failed to disable raw terminal mode");
    }
}

fn space_id() -> String {
    match wallpaper::get() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("Failed to get current wallpaper: {error}");
            process::exit(1);
        }
    }
}

fn wallpaper_name() -> Option<String> {
    wallpaper::get().ok().and_then(|path| {
        Path::new(&path)
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
    })
}

fn spinner(i: u64, paused: bool) -> String {
    if paused {
        "⏸".to_string()
    } else {
        match i % 4 {
            0 => "|".to_string(),
            1 => "/".to_string(),
            2 => "-".to_string(),
            3 => "\\".to_string(),
            _ => unreachable!(),
        }
    }
}

fn main() {
    let mut spinner_state = 0;

    let id = space_id();
    println!("Timing space with ID: {id}");
    let _raw_mode = RawMode::enable();

    let state = Arc::new(Mutex::new(DisplayState::default()));
    let stop = Arc::new(AtomicBool::new(false));
    let worker_state = Arc::clone(&state);
    let worker_stop = Arc::clone(&stop);
    let worker = std::thread::spawn(move || {
        let platform = vigilia_platform::native();
        let mut lid_events = platform.lid_events();
        let mut last_wallpaper_check = Instant::now() - std::time::Duration::from_secs(1);

        while !worker_stop.load(Ordering::Relaxed) {
            if let Some(events) = &mut lid_events {
                while let Ok(lid_state) = events.try_recv() {
                    worker_state.lock().unwrap().lid_state = Some(lid_state);
                }
            }

            if last_wallpaper_check.elapsed() >= std::time::Duration::from_secs(1) {
                if let Some(wallpaper_name) = wallpaper_name() {
                    worker_state.lock().unwrap().wallpaper_name = Some(wallpaper_name);
                }
                last_wallpaper_check = Instant::now();
            }

            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    });

    let mut duration = Duration::zero();
    let mut last_timestamp = Instant::now();
    let mut paused_manually = false;

    loop {
        if event::poll(std::time::Duration::from_millis(100))
            .expect("failed to poll terminal input")
        {
            if let event::Event::Key(key) = event::read().expect("failed to read terminal input") {
                match (key.code, key.modifiers) {
                    (KeyCode::Char('r') | KeyCode::Char('R'), _) => {
                        duration = Duration::zero();
                        last_timestamp = Instant::now();
                    }
                    (KeyCode::Char('p') | KeyCode::Char('P'), _) => {
                        paused_manually = !paused_manually;
                    }
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                        stop.store(true, Ordering::Relaxed);
                        break;
                    }
                    _ => {}
                }
            }
        }

        let (wallpaper_name, lid_state) = {
            let state = state.lock().unwrap();
            (
                state
                    .wallpaper_name
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
                state.lid_state,
            )
        };
        let lid_name = match lid_state {
            Some(LidState::Open) => "lid open",
            Some(LidState::Closed) => "lid closed",
            None => "lid unknown",
        };

        let paused = paused_manually
            || wallpaper_name != "dgnfog.png"
            || lid_state == Some(LidState::Closed);

        {
            let now = Instant::now();
            if !paused {
                let delta = Duration::from_std(now - last_timestamp).unwrap();
                if delta.num_milliseconds() < 1000 {
                    duration = duration + delta;
                }
            }
            last_timestamp = now;
        }

        let duration_str = {
            let hours = duration.num_hours();
            let minutes = duration.num_minutes() - hours * 60;
            let seconds = duration.num_seconds() - hours * 3600 - minutes * 60;
            format!("{hours}h {minutes}m {seconds}s")
        };

        print!(
            "\x1b[2K\r{spinner} {duration_str}",
            spinner = spinner(spinner_state, paused)
        );
        io::stdout().flush().expect("failed to flush stdout");

        // sleep
        std::thread::sleep(std::time::Duration::from_millis(50));
        spinner_state = spinner_state.wrapping_add(1);
    }

    worker.join().expect("wallpaper and lid worker panicked");
}
