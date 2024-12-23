use std::env;
use std::fs;
use std::path::Path;
use std::process::{exit, Command};

fn main() {
    // Get command-line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        eprintln!("Usage: add_subtitles <mkv_folder> <subtitle_folder> <language_code>");
        eprintln!("Language code: 'en' for English, 'ja' for Japanese");
        exit(1);
    }

    let mkv_dir = &args[1];
    let subtitle_dir = &args[2];
    let language_code = &args[3];
    let output_dir = "output";

    // Validate the language code
    if language_code != "en" && language_code != "ja" {
        eprintln!(
            "Error: Invalid language code '{}'. Use 'en' or 'ja'.",
            language_code
        );
        exit(1);
    }

    // Convert language code to mkvmerge-compatible format
    let language_flag = match language_code.as_str() {
        "en" => "eng",
        "ja" => "jpn",
        _ => unreachable!(),
    };

    // Ensure the directories exist
    if !Path::new(mkv_dir).is_dir() {
        eprintln!("Error: MKV directory '{}' does not exist.", mkv_dir);
        exit(1);
    }
    if !Path::new(subtitle_dir).is_dir() {
        eprintln!(
            "Error: Subtitle directory '{}' does not exist.",
            subtitle_dir
        );
        exit(1);
    }

    // Create the output directory
    fs::create_dir_all(output_dir).expect("Failed to create output directory.");

    // Get sorted lists of MKV and subtitle files
    let mut mkv_files: Vec<_> = fs::read_dir(mkv_dir)
        .expect("Failed to read MKV directory.")
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .map(|ext| ext == "mkv")
                .unwrap_or(false)
        })
        .collect();
    mkv_files.sort_by_key(|entry| entry.file_name());

    let mut subtitle_files: Vec<_> = fs::read_dir(subtitle_dir)
        .expect("Failed to read subtitle directory.")
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .map(|ext| ext == "srt" || ext == "ass")
                .unwrap_or(false)
        })
        .collect();
    subtitle_files.sort_by_key(|entry| entry.file_name());

    // Ensure the file counts match
    if mkv_files.len() != subtitle_files.len() {
        eprintln!("Error: The number of MKV files and subtitle files must be the same.");
        exit(1);
    }

    // Process each pair of MKV and subtitle files
    for (mkv_entry, subtitle_entry) in mkv_files.iter().zip(subtitle_files.iter()) {
        let mkv_path = mkv_entry.path();
        let subtitle_path = subtitle_entry.path();
        let output_path = Path::new(output_dir).join(mkv_entry.file_name());

        // Run mkvmerge command
        println!(
            "Merging '{}' with '{}' (language: {}) into '{}'",
            mkv_path.display(),
            subtitle_path.display(),
            language_flag,
            output_path.display()
        );

        let status = Command::new("mkvmerge")
            .arg("-o")
            .arg(output_path)
            .arg(&mkv_path)
            .arg("--language")
            .arg(format!("0:{}", language_flag))
            .arg(&subtitle_path)
            .status()
            .expect("Failed to execute mkvmerge.");

        if !status.success() {
            eprintln!(
                "Error: Failed to merge '{}' with '{}'.",
                mkv_path.display(),
                subtitle_path.display()
            );
        }
    }

    println!("All files processed. Output files are in '{}'.", output_dir);
}
