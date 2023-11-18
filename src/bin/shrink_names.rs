use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();
    for path in &args[1..] {
        println!("{}", path);
    }
}
