use std::fs;

fn main() {
    let file_access_time = std::time::Instant::now();

    // let json_path = std::env::args().last().expect("No json arg!");
    let json_path = "/Users/robin/Desktop/perfaware/haversine/haversine.json";
    // let contents = fs::read_to_string(&json_path).expect("Should have been able to read the file");
    let contents = fs::read(&json_path).expect("Should have been able to read the file");

    println!(
        "File access: {:.3}s",
        file_access_time.elapsed().as_secs_f32()
    );
    println!("\tPath: {}", json_path);
    println!("\tLength (bytes): {}", contents.len());

    let parse_time = std::time::Instant::now();

    let mut numbers = Vec::<f64>::new();
    let mut n = 0;
    while n < contents.len() {
        if contents[n] == b':' {
            n += 2;
            let start = n;
            while contents[n].is_ascii_digit() || contents[n] == b'.' || contents[n] == b'-' {
                n += 1;
            }

            if n > start {
                let num_to_parse = &contents[start..n];
                let s = std::str::from_utf8(num_to_parse).expect("couldn't convert to utf8");
                let num = s.parse().expect("Couldn't parse");
                numbers.push(num);
                // println!("num: >{}<", s);
            }
            n += 5;
        } else {
            n += 1;
        }
    }

    println!(
        "Parse: {:.3}s - {} numbers",
        parse_time.elapsed().as_secs_f32(),
        numbers.len()
    );
}
// rwtodo cargo instruments
