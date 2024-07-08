use std::fs;
use std::path::PathBuf;

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

// rwtodo is it possible to return &str with lifetime specifiers?
// rwtodo how does this handle 日本語 etc?
pub fn common_prefix(names: &[String]) -> String {
    let name_a = &names[0];

    let mut prefix_len = name_a.len();
    for name_b in names {
        while prefix_len > 0 && !name_b.starts_with(&name_a[..prefix_len]) {
            prefix_len -= 1;
        }
    }

    name_a[..prefix_len].to_string()
}

pub fn common_suffix(names: &[String]) -> String {
    let name_a = &names[0];

    let mut suffix_start = 0;
    for name_b in names {
        while suffix_start < name_a.len() && !name_b.ends_with(&name_a[suffix_start..]) {
            suffix_start += 1;
        }
    }

    name_a[suffix_start..].to_string()
}
