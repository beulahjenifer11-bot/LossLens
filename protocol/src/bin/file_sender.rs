use std::collections::VecDeque;
use std::fs;
use std::net::UdpSocket;
use std::time::{Duration, Instant};

fn main() {
    // Step 1: command line, file to send and optional receiver address
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("usage: cargo run --bin file_sender -- <file> [receiver_addr]").clone();
    let server = args.get(2).cloned().unwrap_or("127.0.0.1:9001".to_string());
    let data = fs::read(&path).expect("could not read file");

    // Step 2: settings. A "chunk" is one 1200-byte slice of the file; chunk number total_chunks stands for the FIN
    let mss: usize = 1200;
    let window: usize = 20;
    let conn_id: u32 = 0x1000_0001;
    let total_chunks = (data.len() + mss - 1) / mss;
    let fin_chunk = total_chunks;
    println!("Sending {}: {} bytes = {} chunks + FIN, window {} packets", path, data.len(), total_chunks, window);

    // Step 3: socket, waits at most 10 ms for each ACK so loss checks run often
    let socket = UdpSocket::bind("0.0.0.0:0").expect("bind failed");
    socket.set_read_timeout(Some(Duration::from_millis(10))).unwrap();

    // Step 4: CRC32 and packet builder
    let crc32 = |data: &[u8]| -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        for &b in data.iter() {
            crc ^= b as u32;
            for _ in 0..8 {
                if crc & 1 == 1 { crc = (crc >> 1) ^ 0xEDB8_8320; } else { crc >>= 1; }
            }
        }
        !crc
    };
    let build = |ptype: u8, flags: u8, conn: u32, pn: u64, off: u64, ts: u64, payload: &[u8]| -> Vec<u8> {
        let mut p: Vec<u8> = Vec::new();
        p.push(1u8);
        p.push(ptype);
        p.push(flags);
        p.extend_from_slice(&conn.to_be_bytes());
        p.extend_from_slice(&pn.to_be_bytes());
        p.extend_from_slice(&off.to_be_bytes());
        p.extend_from_slice(&ts.to_be_bytes());
        p.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        p.extend_from_slice(&[0u8; 4]);
        p.extend_from_slice(payload);
        let c = crc32(&p);
        p[33..37].copy_from_slice(&c.to_be_bytes());
        p
    };

    // Step 5: sender state. The three pkt_ vectors are indexed by packet number
    let clock = Instant::now();
    let mut pkt_chunk: Vec<usize> = Vec::new();      // which chunk this packet carried
    let mut pkt_time: Vec<Instant> = Vec::new();     // when it was sent
    let mut pkt_state: Vec<u8> = Vec::new();         // 0 = in flight, 1 = acked, 2 = declared lost
    let mut chunk_acked = vec![false; total_chunks + 1];
    let mut chunks_acked_count: usize = 0;
    let mut next_new: usize = 0;                     // next chunk never sent before
    let mut fin_sent = false;
    let mut retx_queue: VecDeque<usize> = VecDeque::new(); // chunks waiting to be sent again
    let mut in_flight: usize = 0;
    let mut largest_acked: i64 = -1;
    let mut srtt: f64 = 0.0;
    let mut latest_rtt: f64 = 0.0;
    let mut min_rtt: f64 = f64::MAX;
    let mut have_rtt = false;
    let mut sent_total: u64 = 0;
    let mut retx_total: u64 = 0;
    let mut lost_declared: u64 = 0;
    let mut spurious: u64 = 0;   // declared lost, but the packet arrived after all
    let mut ce_echoed: u32 = 0;
    let mut last_ack_time = Instant::now(); // for the timeout rule
    let mut last_rx = Instant::now();       // for giving up
    let mut buf = [0u8; 2000];

    // Step 6: main loop, runs until every chunk and the FIN are acknowledged
    while chunks_acked_count < total_chunks + 1 {

        // Step 6a: send while the window has room. Retransmissions go first, then new data, then the FIN
        while in_flight < window {
            let mut is_retx = false;
            let mut chunk: Option<usize> = None;
            while let Some(c) = retx_queue.pop_front() {
                if !chunk_acked[c] { chunk = Some(c); is_retx = true; break; } // skip chunks acked in the meantime
            }
            if chunk.is_none() {
                if next_new < total_chunks { chunk = Some(next_new); next_new += 1; }
                else if !fin_sent { chunk = Some(fin_chunk); fin_sent = true; }
            }
            let c = match chunk { Some(c) => c, None => break };
            let pn = pkt_chunk.len() as u64; // every transmission gets a brand new packet number
            let ts = clock.elapsed().as_micros() as u64;
            let pkt = if c < total_chunks {
                let off = c * mss;
                let end = (off + mss).min(data.len());
                build(0, 0b01, conn_id, pn, off as u64, ts, &data[off..end])
            } else {
                build(4, 0b01, conn_id, pn, data.len() as u64, ts, &[])
            };
            socket.send_to(&pkt, &server).expect("send failed");
            pkt_chunk.push(c);
            pkt_time.push(Instant::now());
            pkt_state.push(0);
            in_flight += 1;
            sent_total += 1;
            if is_retx { retx_total += 1; }
        }

        // Step 6b: wait up to 10 ms for an ACK, keep its frame only if the CRC is good
        let mut frame_opt: Option<Vec<u8>> = None;
        if let Ok((len, _)) = socket.recv_from(&mut buf) {
            if len >= 37 + 18 && buf[1] == 1 {
                let plen = u16::from_be_bytes(buf[31..33].try_into().unwrap()) as usize;
                if plen >= 18 && len >= 37 + plen {
                    let stored = u32::from_be_bytes(buf[33..37].try_into().unwrap());
                    let mut check = buf[..37 + plen].to_vec();
                    check[33..37].copy_from_slice(&[0u8; 4]);
                    if crc32(&check) == stored { frame_opt = Some(buf[37..37 + plen].to_vec()); }
                }
            }
        }

        // Step 6c: process a valid ACK frame
        if let Some(frame) = frame_opt {
            last_ack_time = Instant::now();
            last_rx = Instant::now();
            let largest = u64::from_be_bytes(frame[0..8].try_into().unwrap());
            let delay_us = u32::from_be_bytes(frame[8..12].try_into().unwrap());
            let ce = u32::from_be_bytes(frame[12..16].try_into().unwrap());
            let n = u16::from_be_bytes(frame[16..18].try_into().unwrap()) as usize;
            if ce > ce_echoed { ce_echoed = ce; }
            if largest as i64 > largest_acked { largest_acked = largest as i64; }

            for i in 0..n {
                let o = 18 + i * 16;
                if frame.len() < o + 16 { break; }
                let s = u64::from_be_bytes(frame[o..o + 8].try_into().unwrap());
                let e = u64::from_be_bytes(frame[o + 8..o + 16].try_into().unwrap());
                for pn in s..=e {
                    let p = pn as usize;
                    if p >= pkt_state.len() || pkt_state[p] == 1 { continue; } // unknown or already acked
                    let was = pkt_state[p];
                    pkt_state[p] = 1;
                    if was == 0 { in_flight -= 1; }   // it was still in flight
                    if was == 2 { spurious += 1; }    // we had declared it lost, it arrived anyway
                    let c = pkt_chunk[p];
                    if !chunk_acked[c] { chunk_acked[c] = true; chunks_acked_count += 1; }
                    // RTT sample from the largest acknowledged packet
                    if pn == largest {
                        let rtt = pkt_time[p].elapsed().as_secs_f64() * 1000.0 - delay_us as f64 / 1000.0;
                        latest_rtt = rtt;
                        if !have_rtt { srtt = rtt; have_rtt = true; } else { srtt = 0.875 * srtt + 0.125 * rtt; }
                        if rtt < min_rtt { min_rtt = rtt; }
                    }
                }
            }
        }

        // Step 6d: loss detection over every packet still in flight
        let time_thresh_ms = if have_rtt { 1.125 * srtt.max(latest_rtt) } else { 500.0 };
        let rto_ms = if have_rtt { (4.0 * srtt).max(200.0) } else { 1000.0 };
        let timed_out = in_flight > 0 && last_ack_time.elapsed().as_secs_f64() * 1000.0 > rto_ms;
        for p in 0..pkt_state.len() {
            if pkt_state[p] != 0 { continue; }
            let age_ms = pkt_time[p].elapsed().as_secs_f64() * 1000.0;
            let by_packets = largest_acked >= p as i64 + 3;                      // rule 1: packet threshold
            let by_time = largest_acked > p as i64 && age_ms > time_thresh_ms;   // rule 2: time threshold
            let by_timeout = timed_out && age_ms > rto_ms;                       // rule 3: timeout
            if by_packets || by_time || by_timeout {
                pkt_state[p] = 2;
                in_flight -= 1;
                lost_declared += 1;
                if !chunk_acked[pkt_chunk[p]] { retx_queue.push_back(pkt_chunk[p]); }
            }
        }
        if timed_out { last_ack_time = Instant::now(); } // wait one more timeout period before the next round

        // Step 6e: give up if nothing valid has arrived for 10 seconds
        if last_rx.elapsed().as_secs() >= 10 {
            println!("Stalled: no ACKs for 10 s. Is the receiver running? ({} of {} acked)", chunks_acked_count, total_chunks + 1);
            break;
        }
    }

    // Step 7: report
    let secs = clock.elapsed().as_secs_f64();
    println!("Done: {} bytes in {:.3} s, goodput {:.2} Mbps", data.len(), secs, data.len() as f64 * 8.0 / secs / 1e6);
    if have_rtt {
        println!("srtt {:.3} ms, min RTT {:.3} ms, CE marks echoed {}", srtt, min_rtt, ce_echoed);
    }
    println!("Packets sent {}, retransmissions {} ({:.2}%), losses declared {}, spurious {}",
        sent_total, retx_total, retx_total as f64 * 100.0 / sent_total as f64, lost_declared, spurious);
}