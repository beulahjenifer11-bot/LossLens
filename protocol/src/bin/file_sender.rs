use std::fs;
use std::net::UdpSocket;
use std::time::{Duration, Instant};

fn main() {
    // Step 1: command line, file to send and optional receiver address
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("usage: cargo run --bin file_sender -- <file> [receiver_addr]").clone();
    let server = args.get(2).cloned().unwrap_or("127.0.0.1:9001".to_string());
    let data = fs::read(&path).expect("could not read file");

    // Step 2: settings and packet counts
    let mss: usize = 1200;      // payload bytes per packet
    let window: usize = 20;     // fixed window: at most 20 packets in flight
    let conn_id: u32 = 0x1000_0001;
    let total_chunks = (data.len() + mss - 1) / mss;  // data packets (rounded up)
    let total_packets = total_chunks + 1;             // plus one FIN packet
    println!("Sending {}: {} bytes = {} data packets + 1 FIN, window {} packets", path, data.len(), total_chunks, window);

    // Step 3: socket, waits at most 200 ms for each ACK
    let socket = UdpSocket::bind("0.0.0.0:0").expect("bind failed");
    socket.set_read_timeout(Some(Duration::from_millis(200))).unwrap();

    // Step 4: CRC32 and packet builder (same as the receiver)
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

    // Step 5: sender state
    let clock = Instant::now();
    let mut next_pn: usize = 0;                        // next packet number to send
    let mut in_flight: usize = 0;                      // sent but not yet acknowledged
    let mut acked = vec![false; total_packets];        // acked[pn] is true once pn is acknowledged
    let mut acked_count: usize = 0;
    let mut sent_at: Vec<Instant> = Vec::new();        // sent_at[pn] is the send time of packet pn
    let mut srtt: f64 = 0.0;                           // smoothed RTT in ms
    let mut min_rtt: f64 = f64::MAX;                   // lowest RTT seen in ms
    let mut have_rtt = false;
    let mut ce_echoed: u32 = 0;
    let mut idle: u32 = 0;                             // consecutive 200 ms waits with no ACK
    let mut buf = [0u8; 2000];

    // Step 6: main loop, runs until every packet (data and FIN) is acknowledged
    while acked_count < total_packets {
        // Step 6a: send while the window has room
        while next_pn < total_packets && in_flight < window {
            let ts = clock.elapsed().as_micros() as u64;
            let pkt = if next_pn < total_chunks {
                let off = next_pn * mss;
                let end = (off + mss).min(data.len());
                build(0, 0b01, conn_id, next_pn as u64, off as u64, ts, &data[off..end]) // DATA, ECT flag on
            } else {
                build(4, 0b01, conn_id, next_pn as u64, data.len() as u64, ts, &[])      // FIN, offset = file length
            };
            socket.send_to(&pkt, &server).expect("send failed");
            sent_at.push(Instant::now());
            in_flight += 1;
            next_pn += 1;
        }

        // Step 6b: wait for an ACK
        match socket.recv_from(&mut buf) {
            Ok((len, _)) => {
                let rx = &buf[..len];
                if len < 37 + 18 || rx[1] != 1 { continue; } // not a complete ACK packet
                let plen = u16::from_be_bytes(rx[31..33].try_into().unwrap()) as usize;
                if len < 37 + plen { continue; }
                let stored_crc = u32::from_be_bytes(rx[33..37].try_into().unwrap());
                let mut check = rx[..37 + plen].to_vec();
                check[33..37].copy_from_slice(&[0u8; 4]);
                if crc32(&check) != stored_crc { continue; } // corrupted ACK, ignore it
                idle = 0;

                // Step 6c: decode the ACK frame
                let frame = &rx[37..37 + plen];
                let largest = u64::from_be_bytes(frame[0..8].try_into().unwrap());
                let delay_us = u32::from_be_bytes(frame[8..12].try_into().unwrap());
                let ce = u32::from_be_bytes(frame[12..16].try_into().unwrap());
                let n = u16::from_be_bytes(frame[16..18].try_into().unwrap()) as usize;
                if ce > ce_echoed { ce_echoed = ce; }

                // Step 6d: mark every packet inside the SACK ranges as acknowledged
                for i in 0..n {
                    let o = 18 + i * 16;
                    if frame.len() < o + 16 { break; }
                    let s = u64::from_be_bytes(frame[o..o + 8].try_into().unwrap());
                    let e = u64::from_be_bytes(frame[o + 8..o + 16].try_into().unwrap());
                    for pn in s..=e {
                        let p = pn as usize;
                        if p < next_pn && !acked[p] {
                            acked[p] = true;
                            acked_count += 1;
                            in_flight = in_flight.saturating_sub(1);
                            // Step 6e: RTT sample from the largest newly acknowledged packet
                            if pn == largest {
                                let rtt = sent_at[p].elapsed().as_secs_f64() * 1000.0 - delay_us as f64 / 1000.0;
                                if !have_rtt { srtt = rtt; have_rtt = true; } else { srtt = 0.875 * srtt + 0.125 * rtt; }
                                if rtt < min_rtt { min_rtt = rtt; }
                            }
                        }
                    }
                }
            }
            Err(_) => {
                // Step 6f: no ACK within 200 ms; give up after 5 seconds (retransmission comes in Step 12)
                idle += 1;
                if idle >= 25 {
                    println!("Stalled: no ACKs for 5 s. Is the receiver running? Was a packet lost? ({} of {} acked)", acked_count, total_packets);
                    break;
                }
            }
        }
    }

    // Step 7: report
    let secs = clock.elapsed().as_secs_f64();
    println!("Done: {} bytes in {:.3} s, goodput {:.2} Mbps", data.len(), secs, data.len() as f64 * 8.0 / secs / 1e6);
    if have_rtt {
        println!("srtt {:.3} ms, min RTT {:.3} ms, CE marks echoed {}", srtt, min_rtt, ce_echoed);
    }
}