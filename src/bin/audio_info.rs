use scripts::*;
use std::fs;
use std::path::PathBuf;
use symphonia;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut music_dir = PathBuf::new();
    music_dir.push(std::env::home_dir().ok_or("no home dir")?);
    music_dir.push("Music");
    music_dir.push("Music");
    music_dir.push("Music");

    let all_files = get_all_files(&music_dir)?;
    println!("{}", all_files.len());

    Ok(())
}
