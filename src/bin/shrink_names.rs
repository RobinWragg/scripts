use scripts::*;
use std::fs;
use std::io;
use std::{env, path};

use unicode_normalization::UnicodeNormalization;

fn path_args_as_components(args: &[String]) -> Option<Vec<(String, String, String)>> {
    let mut components = Vec::<(String, String, String)>::new();

    let paths: Vec<&path::Path> = args.iter().map(|a| path::Path::new(a)).collect();

    for path in paths {
        let name_os = path.file_name()?;
        let ext_os = path.extension()?;

        let name = name_os.to_str()?.nfc().collect::<String>();
        let ext = ext_os.to_str()?.nfc().collect::<String>();

        let mut buf = path.to_path_buf();
        buf.pop();
        let dir = buf.to_str()?.nfc().collect::<String>();

        // SAFER: slice using .chars() to handle multibyte Unicode properly
        let name_chars: Vec<char> = name.chars().collect();
        let ext_len = ext.chars().count();

        if ext_len + 1 > name_chars.len() {
            eprintln!("Skipping path with invalid name/ext: {:?}", path);
            continue;
        }

        let name_without_ext: String = name_chars[..(name_chars.len() - ext_len - 1)]
            .iter()
            .collect();

        components.push((
            String::from(dir),
            name_without_ext,
            ext, // Already normalized
        ));
    }

    Some(components)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let Some(mut paths) = path_args_as_components(&args[1..]) else {
        eprintln!("Could not parse path arguments");
        return;
    };

    let mut names_without_ext = vec![];
    for path in &paths {
        names_without_ext.push(path.1.clone());
    }

    let prefix = common_prefix(&names_without_ext);
    let suffix = common_suffix(&names_without_ext);
    println!("\"{}<UNIQUE>{}\"", prefix, suffix);
    println!("Proceed to rename? (y/n)");

    let mut s = String::new();
    io::stdin()
        .read_line(&mut s)
        .expect("Did not enter a correct string");

    if s.trim() != "y" {
        println!("Aborting.");
        return;
    }

    for path in &mut paths {
        let mut original_buf = path::PathBuf::from(&path.0);
        let name_with_ext = format!("{}.{}", path.1, path.2);
        original_buf.push(&name_with_ext);

        let name_chars: Vec<char> = path.1.chars().collect();
        let shrunk_chars =
            &name_chars[prefix.chars().count()..(name_chars.len() - suffix.chars().count())];
        let shrunk_name: String = shrunk_chars.iter().collect();

        let mut shrunk_buf = path::PathBuf::from(&path.0);
        let shrunk_with_ext = format!("{}.{}", shrunk_name, path.2);
        shrunk_buf.push(&shrunk_with_ext);

        println!("Renaming: {:?} -> {:?}", original_buf, shrunk_buf);

        if let Err(e) = fs::rename(&original_buf, &shrunk_buf) {
            eprintln!(
                "Failed to rename {:?} -> {:?}: {}",
                original_buf, shrunk_buf, e
            );
        }
    }
}
