use std::io::{self, Write};
use std::path::Path;
use std::process;

const REDRAW_IN_PLACE: bool = true;

fn space_id() -> String {
    match wallpaper::get() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("Failed to get current wallpaper: {error}");
            process::exit(1);
        }
    }
}

fn main() {
    let id = space_id();
    println!("Timing space with ID: {id}");

    loop {
        let wallpaper_path = space_id();

        let wallpaper_name = Path::new(&wallpaper_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&wallpaper_path);

        if REDRAW_IN_PLACE {
            print!("\x1b[2K\r{wallpaper_name}");
            io::stdout().flush().expect("failed to flush stdout");
        } else {
            println!("{wallpaper_name}");
        }

        // sleep
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
