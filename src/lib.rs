use std::fs;
use std::path::PathBuf;
use unicode_normalization::UnicodeNormalization;

pub type BaseResult<T> = Result<T, Box<dyn std::error::Error>>;

pub fn get_all_files(root_path: &PathBuf) -> BaseResult<Vec<PathBuf>> {
    let paths = fs::read_dir(root_path)?;
    let mut file_paths = vec![];

    for dir_entry in paths {
        let path = dir_entry?.path();
        if path.is_dir() {
            file_paths.append(&mut get_all_files(&path)?);
        } else {
            file_paths.push(path);
        }
    }

    Ok(file_paths)
}

pub fn common_prefix(names: &[String]) -> String {
    if names.is_empty() {
        return String::new();
    }

    let normalized: Vec<Vec<char>> = names
        .iter()
        .map(|s| s.nfc().collect::<Vec<char>>())
        .collect();

    let mut prefix = vec![];
    let first = &normalized[0];

    'outer: for i in 0..first.len() {
        let ch = first[i];
        for name in &normalized[1..] {
            if i >= name.len() || name[i] != ch {
                break 'outer;
            }
        }
        prefix.push(ch);
    }

    prefix.into_iter().collect()
}

pub fn common_suffix(names: &[String]) -> String {
    if names.is_empty() {
        return String::new();
    }

    let normalized: Vec<Vec<char>> = names
        .iter()
        .map(|s| s.nfc().collect::<Vec<char>>())
        .collect();

    let mut suffix = vec![];
    let first = &normalized[0];
    let first_len = first.len();

    'outer: for i in 0..first_len {
        let ch = first[first_len - 1 - i];
        for name in &normalized[1..] {
            if i >= name.len() || name[name.len() - 1 - i] != ch {
                break 'outer;
            }
        }
        suffix.push(ch);
    }

    suffix.into_iter().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_common_prefix_ascii() {
        let names = vec![
            "hello_world".to_string(),
            "hello_there".to_string(),
            "hello_rustaceans".to_string(),
        ];
        assert_eq!(common_prefix(&names), "hello_");
    }

    #[test]
    fn test_common_suffix_ascii() {
        let names = vec![
            "my_file.txt".to_string(),
            "your_file.txt".to_string(),
            "our_file.txt".to_string(),
        ];
        let c = common_suffix(&names);
        assert_eq!(&c, "_file.txt");
    }

    #[test]
    fn test_common_prefix_japanese() {
        let names = vec![
            "東京駅_地図".to_string(),
            "東京駅_構内".to_string(),
            "東京駅_案内".to_string(),
        ];
        assert_eq!(common_prefix(&names), "東京駅_");
    }

    #[test]
    fn test_common_suffix_japanese() {
        let names = vec![
            "案内図日本語".to_string(),
            "説明書日本語".to_string(),
            "手順日本語".to_string(),
        ];
        assert_eq!(common_suffix(&names), "日本語");
    }

    #[test]
    fn test_common_prefix_unicode_normalization() {
        // が = か + ゙
        let composed = "ひらがな_が".to_string(); // precomposed
        let decomposed = "ひらがな_が".to_string(); // decomposed
        let names = vec![composed, decomposed];
        assert_eq!(common_prefix(&names), "ひらがな_が");
    }

    #[test]
    fn test_common_suffix_unicode_normalization() {
        let composed = "説明が".to_string();
        let decomposed = "内容が".to_string();
        let names = vec![composed, decomposed];
        let c = common_suffix(&names);
        assert_eq!(&c, "が");
    }

    #[test]
    fn test_empty_input() {
        let names: Vec<String> = vec![];
        assert_eq!(common_prefix(&names), "");
        assert_eq!(common_suffix(&names), "");
    }

    #[test]
    fn test_no_common_prefix() {
        let names = vec!["abc".to_string(), "xyz".to_string()];
        assert_eq!(common_prefix(&names), "");
    }

    #[test]
    fn test_no_common_suffix() {
        let names = vec!["abc".to_string(), "xyz".to_string()];
        assert_eq!(common_suffix(&names), "");
    }
}
