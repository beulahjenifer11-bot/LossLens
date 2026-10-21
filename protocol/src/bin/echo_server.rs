use std::net::UdpSocket;

fn main() {
    // Step 1: open a UDP socket on port 9000, listening on all network interfaces
    let socket = UdpSocket::bind("0.0.0.0:9000").expect("bind failed");
    println!("Echo server listening on port 9000");

    // Step 2: a buffer to hold one incoming packet (1500 bytes = typical network packet size)
    let mut buf = [0u8; 1500];

    // Step 3: repeat forever
    loop {
        // Step 3a: wait for a packet; len = bytes received, src = sender's address
        let (len, src) = socket.recv_from(&mut buf).expect("recv failed");
        println!("Got {} bytes from {}", len, src);

        // Step 3b: send exactly those bytes back to whoever sent them
        socket.send_to(&buf[..len], src).expect("send failed");
    }
}