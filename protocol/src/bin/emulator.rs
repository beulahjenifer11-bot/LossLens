use std::collections::VecDeque;
use std::fs::File;
use std::io::Write;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

fn main() {
    // Step 1: command line, loss percent (default 0), one-way delay in ms (default 20), seed (default 1)
    let args: Vec<String> = std::env::args().collect();
    let loss_pct: f64 = args.get(1).map(|s| s.parse().unwrap()).unwrap_or(0.0);
    let delay_ms: u64 = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(20);
    let seed: u64 = args.get(3).map(|s| s.parse().unwrap()).unwrap_or(1);
    let delay = Duration::from_millis(delay_ms);

    // Step 2: socket on port 9100, set to non-blocking so the loop never sleeps
    // the sender talks to this port, and we forward to the receiver on port 9001
    let socket = UdpSocket::bind("0.0.0.0:9100").expect("bind failed");
    socket.set_nonblocking(true).unwrap();
    let receiver_addr: SocketAddr = "127.0.0.1:9001".parse().unwrap();
    println!("Emulator on port 9100 -> receiver 127.0.0.1:9001, random loss {}%, one-way delay {} ms (RTT about {} ms), seed {}", loss_pct, delay_ms, delay_ms * 2, seed);

    // Step 3: seeded random generator (xorshift), same seed always gives the same drop pattern
    let mut rng_state: u64 = seed.max(1);
    for _ in 0..20 {
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
    }

    // Step 4: ground-truth log, one line per dropped packet with the TRUE cause
    let mut log = File::create("emulator_drops.csv").expect("cannot create log");
    writeln!(log, "packet_number,type,cause").unwrap();

    // Step 5: state. The queue holds (time to release, bytes, destination), oldest first.
    // A fixed delay means packets are released in the same order they arrived.
    let mut queue: VecDeque<(Instant, Vec<u8>, SocketAddr)> = VecDeque::new();
    let mut sender_addr: Option<SocketAddr> = None; // learned from the first packet that is not from the receiver
    let mut forwarded_data: u64 = 0;
    let mut dropped_random: u64 = 0;
    let mut forwarded_acks: u64 = 0;
    let mut started = false;
    let mut last_activity = Instant::now();
    let mut buf = [0u8; 2000];

    // Step 6: main loop
    loop {
        // Step 6a: take one packet from the socket if one is waiting
        match socket.recv_from(&mut buf) {
            Ok((len, src)) => {
                started = true;
                last_activity = Instant::now();
                if len >= 37 {
                    let data = buf[..len].to_vec();
                    if src == receiver_addr {
                        // Step 6b: reverse direction (an ACK), delay it and send it to the sender
                        if let Some(s) = sender_addr {
                            queue.push_back((Instant::now() + delay, data, s));
                            forwarded_acks += 1;
                        }
                    } else {
                        // Step 6c: forward direction (DATA or FIN), remember who the sender is
                        sender_addr = Some(src);
                        let ptype = data[1];
                        let pn = u64::from_be_bytes(data[7..15].try_into().unwrap());

                        // Step 6d: roll a number from 0.00 to 99.99, drop if below loss_pct
                        rng_state ^= rng_state << 13;
                        rng_state ^= rng_state >> 7;
                        rng_state ^= rng_state << 17;
                        let roll = (rng_state % 10000) as f64 / 100.0;
                        if (ptype == 0 || ptype == 4) && roll < loss_pct {
                            dropped_random += 1;
                            writeln!(log, "{},{},RANDOM", pn, ptype).unwrap(); // ground truth label
                        } else {
                            queue.push_back((Instant::now() + delay, data, receiver_addr));
                            forwarded_data += 1;
                        }
                    }
                }
            }
            Err(_) => {} // nothing waiting (or a harmless Windows UDP error), carry on
        }

        // Step 6e: release every packet whose delay has passed
        while queue.front().map_or(false, |f| f.0 <= Instant::now()) {
            let (_, bytes, to) = queue.pop_front().unwrap();
            socket.send_to(&bytes, to).ok();
        }

        // Step 6f: stop 3 seconds after the last packet, once the queue is empty
        if started && queue.is_empty() && last_activity.elapsed() > Duration::from_secs(3) {
            break;
        }
        std::thread::yield_now();
    }

    // Step 7: summary
    println!("Emulator done: forwarded {} DATA/FIN, dropped {} (ground truth RANDOM), forwarded {} ACKs", forwarded_data, dropped_random, forwarded_acks);
    println!("Drop labels saved to emulator_drops.csv");
}