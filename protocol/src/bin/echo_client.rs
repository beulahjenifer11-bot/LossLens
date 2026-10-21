use std::net::UdpSocket;
use std::time::{Duration, Instant};

fn main() {
    // Step 1: server address from the command line, default is this same computer
    let server = std::env::args().nth(1).unwrap_or("127.0.0.1:9000".to_string());

    // Step 2: open a UDP socket on any free port; wait at most 1 second for a reply
    let socket = UdpSocket::bind("0.0.0.0:0").expect("bind failed");
    socket.set_read_timeout(Some(Duration::from_secs(1))).unwrap();

    // Step 3: send 5 pings, one at a time
    for i in 0..5 {
        let msg = format!("ping {}", i);
        let start = Instant::now(); // start the stopwatch
        socket.send_to(msg.as_bytes(), &server).expect("send failed");

        let mut buf = [0u8; 1500];
        match socket.recv_from(&mut buf) {
            Ok((len, _)) => {
                // Step 4a: reply came back, print the round trip time (RTT)
                let reply = String::from_utf8_lossy(&buf[..len]);
                println!("{} -> reply '{}' RTT = {:.3} ms", msg, reply, start.elapsed().as_secs_f64() * 1000.0);
            }
            Err(_) => {
                // Step 4b: nothing came back within 1 second, so the packet was lost
                println!("{} -> timed out (packet lost)", msg);
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}