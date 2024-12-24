/*
client test: nc bmo.local 8080
docker pat: dckr_pat_kFLQHIDpOjZY7WNtdG3glDrJaaM
*/

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

fn handle_client(mut stream: TcpStream) {
    // Buffer to hold incoming data
    let mut buffer = [0; 1024];

    // Read data from the stream
    match stream.read(&mut buffer) {
        Ok(size) => {
            // Convert buffer to string
            let message = String::from_utf8_lossy(&buffer[..size]);
            println!("Received message: <{}>", message);

            let response =
                "{\"desired_host\":\"bmo.local\",\"desired_interval\":600,\"desired_timeout\":5}\n";

            stream.write_all(response.as_bytes()).unwrap();
            println!("Sent response <{}>", response);
        }
        Err(e) => {
            eprintln!("Failed to read from stream: {}", e);
        }
    }
}

fn main() -> std::io::Result<()> {
    // Bind the server to the specified address and port
    let listener = TcpListener::bind("0.0.0.0:3000")?;

    println!("Server listening on port 3000");

    // Loop to accept incoming connections
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                println!("New connection: {}", stream.peer_addr().unwrap());

                // Spawn a new thread to handle the client
                thread::spawn(move || {
                    handle_client(stream);
                });
            }
            Err(e) => {
                eprintln!("Failed to accept connection: {}", e);
            }
        }
    }

    Ok(())
}
