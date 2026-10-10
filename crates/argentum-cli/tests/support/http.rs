use serde_json::Value;
use std::io::{BufRead, BufReader, Read};
use std::net::TcpStream;

pub fn read_request(stream: &mut TcpStream) -> Value {
    let mut reader = BufReader::new(stream);
    let mut first = String::new();
    reader.read_line(&mut first).unwrap();
    assert_eq!(first, "POST /v1/chat/completions HTTP/1.1\r\n");
    let mut length = None;
    let mut header_bytes = first.len();
    loop {
        let mut line = String::new();
        assert!(reader.read_line(&mut line).unwrap() > 0);
        header_bytes += line.len();
        assert!(header_bytes <= 16 * 1024);
        if line == "\r\n" {
            break;
        }
        let (name, value) = line.split_once(':').unwrap();
        assert!(!name.eq_ignore_ascii_case("authorization"));
        if name.eq_ignore_ascii_case("content-length") {
            length = Some(value.trim().parse::<usize>().unwrap());
        }
    }
    let length = length.expect("content length");
    assert!(length <= 2 * 1024 * 1024);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    serde_json::from_slice(&body).unwrap()
}
