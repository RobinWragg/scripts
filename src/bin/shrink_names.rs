use scripts::*;
use std::fs;
use std::io;
use std::{env, path};

fn path_args_as_components(args: &[String]) -> Option<Vec<(String, String, String)>> {
    let mut components = Vec::<(String, String, String)>::new();

    let paths: Vec<&path::Path> = args.iter().map(|a| path::Path::new(a)).collect();

    for path in paths {
        let name = path.file_name()?.to_str()?;
        let ext = path.extension()?.to_str()?;
        let mut buf = path.to_path_buf();
        buf.pop();
        let dir = buf.to_str()?;
        assert!(ext.len() - 1 < name.len());
        let name_without_ext = &name[..name.len() - ext.len() - 1];
        components.push((
            String::from(dir),
            String::from(name_without_ext),
            String::from(ext),
        ));
    }

    return Some(components);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut paths = path_args_as_components(&args[1..]).unwrap();

    let mut names_without_ext = vec![];
    for path in &paths {
        {
            // let mut buf = path::PathBuf::new();
            // buf.push(&path.0);
            // let name_with_ext = path.1.clone() + "." + &path.2;
            // buf.push(name_with_ext.clone());
            names_without_ext.push(path.1.clone());
        }
    }

    let prefix = common_prefix(&names_without_ext);
    let suffix = common_suffix(&names_without_ext);
    println!("\"{}<UNIQUE>{}\"", prefix, suffix);
    println!("Proceed to rename? (y/n)");

    let mut s = String::new();
    io::stdin()
        .read_line(&mut s)
        .expect("Did not enter a correct string");

    if s != "y\n" {
        println!("Aborting.");
        return;
    }

    for path in &mut paths {
        {
            let mut original_buf = path::PathBuf::from(&path.0);
            let name_with_ext = path.1.clone() + "." + &path.2;
            original_buf.push(name_with_ext.clone());

            let shrunk_name = &path.1[prefix.len()..(path.1.len() - suffix.len())];
            let mut shrunk_buf = path::PathBuf::from(&path.0);
            let name_with_ext = shrunk_name.to_owned() + "." + &path.2;
            shrunk_buf.push(name_with_ext.clone());

            fs::rename(original_buf, shrunk_buf).unwrap();
        }
    }
}
