use std::env;
use std::fs;
use std::process;

fn main() {
    // Get the file path from the command-line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <file_path>", args[0]);
        process::exit(1);
    }

    let file_path = &args[1];

    // Read the file contents
    let content = match fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file {}: {}", file_path, e);
            process::exit(1);
        }
    };

    // Keep only Japanese characters and whitespace
    let filtered: String = content
        .chars()
        .filter(|c| {
            c.is_whitespace() ||
            ('\u{3040}'..='\u{309F}').contains(c) || // Hiragana
            ('\u{30A0}'..='\u{30FF}').contains(c) || // Katakana
            ('\u{4E00}'..='\u{9FFF}').contains(c) || // Common Kanji
            ('\u{3400}'..='\u{4DBF}').contains(c) || // Rare Kanji
            ('\u{F900}'..='\u{FAFF}').contains(c) // Kanji variants
        })
        .collect();

    // Write the filtered content back to the same file
    if let Err(e) = fs::write(file_path, filtered) {
        eprintln!("Error writing to file {}: {}", file_path, e);
        process::exit(1);
    }
}
