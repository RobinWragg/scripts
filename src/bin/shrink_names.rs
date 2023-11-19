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
    if let Some(paths) = path_args_as_components(&args[1..]) {
        for path in &paths {
            {
                let mut buf = path::PathBuf::new();
                buf.push(&path.0);
                let name_with_ext = path.1.clone() + "." + &path.2;
                buf.push(name_with_ext);
            }
        }
    }
}
