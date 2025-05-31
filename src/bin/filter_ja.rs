use std::env;
use std::fs;
use std::process;

/// Filters a string, keeping only Japanese characters and replacing contiguous whitespace with a single newline.
pub fn filter_japanese_and_collapse_whitespace(input: &str) -> String {
    // Keep only Japanese characters and whitespace
    let filtered: String = input
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

    // Replace every contiguous chunk of whitespace with a single newline
    let mut result = String::new();
    let mut in_whitespace = false;
    for c in filtered.chars() {
        if c.is_whitespace() {
            if !in_whitespace {
                result.push('\n');
                in_whitespace = true;
            }
        } else {
            result.push(c);
            in_whitespace = false;
        }
    }

    // Remove leading newline if present
    result.strip_prefix('\n').unwrap_or(&result).to_string()
}

fn main() {
    // Get the file path from the command-line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 || args.len() > 3 {
        eprintln!("Usage: {} <file_path> [-dd]", args[0]);
        process::exit(1);
    }

    let file_path = &args[1];
    let dedup = args.len() == 3 && args[2] == "-dd";

    // Read the file contents
    let content = match fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file {}: {}", file_path, e);
            process::exit(1);
        }
    };

    let mut result = filter_japanese_and_collapse_whitespace(&content);

    if dedup {
        let mut seen = std::collections::HashSet::new();
        let mut deduped = String::new();
        for line in result.lines() {
            if seen.insert(line) && !line.is_empty() {
                deduped.push_str(line);
                deduped.push('\n');
            }
        }
        result = deduped;
    }

    // Write the filtered content back to the same file
    if let Err(e) = fs::write(file_path, result) {
        eprintln!("Error writing to file {}: {}", file_path, e);
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_only_japanese() {
        let input = "こんにちは世界";
        let expected = "こんにちは世界";
        assert_eq!(filter_japanese_and_collapse_whitespace(input), expected);
    }

    #[test]
    fn test_japanese_and_english() {
        let input = "hello こんにちは world 世界";
        let expected = "こんにちは\n世界";
        assert_eq!(filter_japanese_and_collapse_whitespace(input), expected);
    }

    #[test]
    fn test_whitespace_collapse() {
        let input = "  こんにちは   世界 \n\t さようなら ";
        let expected = "こんにちは\n世界\nさようなら\n";
        assert_eq!(filter_japanese_and_collapse_whitespace(input), expected);
    }

    #[test]
    fn test_only_whitespace() {
        let input = "   \n\t  ";
        let expected = "";
        assert_eq!(filter_japanese_and_collapse_whitespace(input), expected);
    }

    #[test]
    fn test_leading_and_trailing_whitespace() {
        let input = "  こんにちは  ";
        let expected = "こんにちは\n";
        assert_eq!(filter_japanese_and_collapse_whitespace(input), expected);
    }

    #[test]
    fn test_mixed_characters() {
        let input = "abc 123 こんにちは\t\t世界!@#";
        let expected = "こんにちは\n世界";
        assert_eq!(filter_japanese_and_collapse_whitespace(input), expected);
    }
}
