use std::collections::VecDeque;
use std::fs::File;
use std::io::Write;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

fn main() {
    // Step 1: command line: loss percent, one-way delay ms, seed, link rate Mbps, buffer size in packets
    let args: Vec<String> = std::env::args().collect();
    let loss_pct: f64 = args.get(1).map(|s| s.parse().unwrap()).unwrap_or(0.0);
    let delay_ms: u64 = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(20);
    let seed: u64 = args.get(3).map(|s| s.parse().unwrap()).unwrap_or(1);
    let rate_mbps: f64 = args.get(4).map(|s| s.parse().unwrap()).unwrap_or(20.0);
    let buffer_pkts: usize = args.get(5).map(|s| s.parse().unwrap()).unwrap_or(80);
    let delay = Duration::from_millis(delay_ms);
    let rate_bps = rate_mbps * 1e6;

    // Step 2: non-blocking socket on port 9100, forwarding to the receiver on port 9001
    let socket = UdpSocket::bind("0.0.0.0:9100").expect("bind failed");
    socket.set_nonblocking(true).unwrap();
    let receiver_addr: SocketAddr = args.get(6).cloned().unwrap_or("127.0.0.1:9001".to_string()).parse().unwrap(); let mark_thr: usize = args.get(7).map(|s| s.parse().unwrap()).unwrap_or(0); let mut marked: u64 = 0;
    println!("Emulator: random loss {}%, one-way delay {} ms (RTT about {} ms), seed {}, link {} Mbps, buffer {} packets", loss_pct, delay_ms, delay_ms * 2, seed, rate_mbps, buffer_pkts);

    // Step 3: seeded random generator (xorshift)
    let mut rng_state: u64 = seed.max(1);
    for _ in 0..20 {
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
    }

    // Step 4: ground-truth log, one line per dropped packet with the TRUE cause
    let mut log = File::create("emulator_drops.csv").expect("cannot create log");
    writeln!(log, "packet_number,type,cause").unwrap();

    // Step 5: state
    // bottleneck = packets waiting for the link, as (arrival time, bytes), oldest first
    let mut bottleneck: VecDeque<(Instant, Vec<u8>)> = VecDeque::new();
    // serving = the one packet currently being transmitted, as (finish time, bytes)
    let mut serving: Option<(Instant, Vec<u8>)> = None;
    let mut last_finish = Instant::now(); // when the link finished its previous packet
    // delay_q = packets now travelling along the wire, as (release time, bytes, destination)
    let mut delay_q: Vec<(Instant, Vec<u8>, SocketAddr)> = Vec::new();
    let mut sender_addr: Option<SocketAddr> = None;
    let mut forwarded_data: u64 = 0;
    let mut dropped_random: u64 = 0;
    let mut dropped_overflow: u64 = 0;
    let mut forwarded_acks: u64 = 0;
    let mut max_depth: usize = 0;
    let mut started = false;
    let mut last_activity = Instant::now();
    let mut buf = [0u8; 2000];

    // Step 6: main loop
    loop {
        // Step 6a: take one packet from the socket if one is waiting
        if let Ok((len, src)) = socket.recv_from(&mut buf) {
            started = true;
            last_activity = Instant::now();
            if len >= 37 {
                let data = buf[..len].to_vec();
                if src == receiver_addr {
                    // Step 6b: ACK going back to the sender, only the delay applies
                    if let Some(s) = sender_addr {
                        delay_q.push((Instant::now() + delay, data, s));
                        forwarded_acks += 1;
                    }
                } else {
                    // Step 6c: DATA or FIN going forward, remember who the sender is
                    sender_addr = Some(src);
                    let ptype = data[1];
                    let pn = u64::from_be_bytes(data[7..15].try_into().unwrap());

                    // Step 6d: roll 0.00 to 99.99 for random radio loss
                    rng_state ^= rng_state << 13;
                    rng_state ^= rng_state >> 7;
                    rng_state ^= rng_state << 17;
                    let roll = (rng_state % 10000) as f64 / 100.0;
                    if (ptype == 0 || ptype == 4) && roll < loss_pct {
                        dropped_random += 1;
                        writeln!(log, "{},{},RANDOM", pn, ptype).unwrap();
                    } else if bottleneck.len() >= buffer_pkts {
                        // Step 6e: buffer full, drop-tail drops the newest arrival
                        dropped_overflow += 1;
                        writeln!(log, "{},{},QUEUE_OVERFLOW", pn, ptype).unwrap();
                    } else {
                        // Step 6f: room in the buffer, join the queue
                        let mut data = data;
                        if mark_thr > 0 && ptype == 0 && bottleneck.len() >= mark_thr {
                            data[2] |= 0b10;                              // set the CE flag
                            data[33..37].copy_from_slice(&[0u8; 4]);      // zero the CRC field, then recompute the CRC over the packet
                            let mut crc: u32 = 0xFFFF_FFFF;
                            for &b in data.iter() { crc ^= b as u32; for _ in 0..8 { if crc & 1 == 1 { crc = (crc >> 1) ^ 0xEDB8_8320; } else { crc >>= 1; } } }
                            data[33..37].copy_from_slice(&(!crc).to_be_bytes());
                            marked += 1;
                        }
                        bottleneck.push_back((Instant::now(), data));
                        forwarded_data += 1;
                        if bottleneck.len() > max_depth { max_depth = bottleneck.len(); }
                    }
                }
            }
        }

        // Step 6g: if the link is idle, start transmitting the oldest waiting packet
        // transmission time = bits / rate; it starts when the packet arrived or the link became free, whichever is later
        if serving.is_none() {
            if let Some((arrived, bytes)) = bottleneck.pop_front() {
                let start = if arrived > last_finish { arrived } else { last_finish };
                let tx = Duration::from_secs_f64(bytes.len() as f64 * 8.0 / rate_bps);
                serving = Some((start + tx, bytes));
            }
        }

        // Step 6h: if the packet being transmitted has finished, put it on the wire (propagation delay starts now)
        let done = serving.as_ref().map_or(false, |s| s.0 <= Instant::now());
        if done {
            let (finish, bytes) = serving.take().unwrap();
            last_finish = finish;
            delay_q.push((finish + delay, bytes, receiver_addr));
        }

        // Step 6i: release every packet on the wire whose delay has passed
        let now = Instant::now();
        let mut i = 0;
        while i < delay_q.len() {
            if delay_q[i].0 <= now {
                let (_, bytes, to) = delay_q.remove(i);
                socket.send_to(&bytes, to).ok();
            } else {
                i += 1;
            }
        }

        // Step 6j: stop 3 seconds after the last packet, once everything has drained
        if started && bottleneck.is_empty() && serving.is_none() && delay_q.is_empty() && last_activity.elapsed() > Duration::from_secs(3) {
            break;
        }
        std::thread::yield_now();
    }

    // Step 7: summary
    println!("Emulator done: forwarded {} DATA/FIN, dropped {} RANDOM + {} QUEUE_OVERFLOW (ground truth), forwarded {} ACKs", forwarded_data, dropped_random, dropped_overflow, forwarded_acks);
    println!("Max queue depth reached: {} of {} packets", max_depth, buffer_pkts); println!("CE marked {} packets (mark threshold {} packets, 0 = off)", marked, mark_thr); println!("CE marked {} packets (mark threshold {} packets, 0 = off)", marked, mark_thr); println!("CE marked {} packets (mark threshold {} packets, 0 = off)", marked, mark_thr); println!("CE marked {} packets (mark threshold {} packets, 0 = off)", marked, mark_thr); println!("CE marked {} packets (mark threshold {} packets, 0 = off)", marked, mark_thr);
    println!("Drop labels saved to emulator_drops.csv");
}