use std::collections::BTreeSet;
use std::fs;
use std::net::UdpSocket;

fn main() {
    // Step 1: command line, output file, loss percent (default 0), seed (default 12345)
    let args: Vec<String> = std::env::args().collect();
    let out_path = args.get(1).cloned().unwrap_or("received.bin".to_string());
    let loss_pct: f64 = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(0.0);
    let seed: u64 = args.get(3).map(|s| s.parse().unwrap()).unwrap_or(12345);
    let socket = UdpSocket::bind("0.0.0.0:9001").expect("bind failed");
    println!("Receiver listening on port 9001, writing to {}, random loss {}%, seed {}", out_path, loss_pct, seed);

    // Step 2: seeded random generator (xorshift), same seed always gives the same drop pattern
    let mut rng_state: u64 = seed.max(1);
    for _ in 0..20 {
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
    }

    // Step 3: CRC32 calculator and packet builder (same as before)
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

    // Step 4: receiver state
    let mut file_data: Vec<u8> = Vec::new();        // file being rebuilt, bytes placed by stream offset
    let mut got: BTreeSet<u64> = BTreeSet::new();   // packet numbers received (used for SACK ranges)
    let mut chunks_got: BTreeSet<u64> = BTreeSet::new(); // distinct stream offsets received (used to decide when the file is complete)
    let mut total_len: u64 = 0;
    let mut fin_pn: Option<u64> = None;
    let mut ack_pn: u64 = 0; let mut sack_trunc: u64 = 0; let mut sack_trunc: u64 = 0;
    let mut accepted: u64 = 0;   // DATA packets accepted (duplicates included)
    let mut dropped: u64 = 0;    // packets we threw away on purpose, ground truth label RANDOM
    let mut ce_count: u32 = 0;
    let mut buf = [0u8; 2000];

    // Step 5: main loop, one iteration per arriving packet
    loop {
        let (len, src) = socket.recv_from(&mut buf).expect("recv failed");
        if len < 37 { continue; }
        let rx = &buf[..len];

        // Step 5a: decode the header
        let ptype = rx[1];
        let conn = u32::from_be_bytes(rx[3..7].try_into().unwrap());
        let pn = u64::from_be_bytes(rx[7..15].try_into().unwrap());
        let off = u64::from_be_bytes(rx[15..23].try_into().unwrap());
        let plen = u16::from_be_bytes(rx[31..33].try_into().unwrap()) as usize;
        let stored_crc = u32::from_be_bytes(rx[33..37].try_into().unwrap());
        if len < 37 + plen { continue; }

        // Step 5b: verify the CRC, drop the packet if corrupted
        let mut check = rx[..37 + plen].to_vec();
        check[33..37].copy_from_slice(&[0u8; 4]);
        if crc32(&check) != stored_crc {
            println!("Packet {} failed CRC, dropped", pn);
            continue;
        }

        // Step 5c: emulate random radio loss, roll a number from 0.00 to 99.99 and drop if below loss_pct
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
        let roll = (rng_state % 10000) as f64 / 100.0;
        if (ptype == 0 || ptype == 4) && roll < loss_pct {
            dropped += 1;
            continue; // no ACK is sent, the packet simply vanishes
        }
        if rx[2] & 0b10 != 0 { ce_count += 1; }

        // Step 5d: act on the packet type
        if ptype == 0 {
            let end = off as usize + plen;
            if file_data.len() < end { file_data.resize(end, 0); }
            file_data[off as usize..end].copy_from_slice(&rx[37..37 + plen]);
            got.insert(pn);
            chunks_got.insert(off);
            accepted += 1;
        } else if ptype == 4 {
            total_len = off; // FIN carries the total file length
            fin_pn = Some(pn);
            got.insert(pn);
        } else {
            continue;
        }

        // Step 5e: build SACK ranges from the sorted set of received packet numbers
        let mut ranges: Vec<(u64, u64)> = Vec::new();
        let mut it = got.iter();
        let first = *it.next().unwrap();
        let mut s = first;
        let mut prev = first;
        for &p in it {
            if p == prev + 1 {
                prev = p;
            } else {
                ranges.push((s, prev));
                s = p;
                prev = p;
            }
        }
        ranges.push((s, prev));
        if ranges.len() > 50 { sack_trunc += 1; ranges = ranges.split_off(ranges.len() - 50); } // keep newest 50 ranges
        let largest = *got.iter().next_back().unwrap();

        // Step 5f: encode the ACK frame and send it back
        let mut frame: Vec<u8> = Vec::new();
        frame.extend_from_slice(&largest.to_be_bytes());
        frame.extend_from_slice(&0u32.to_be_bytes());
        frame.extend_from_slice(&ce_count.to_be_bytes());
        frame.extend_from_slice(&(ranges.len() as u16).to_be_bytes());
        for &(a, b) in ranges.iter() {
            frame.extend_from_slice(&a.to_be_bytes());
            frame.extend_from_slice(&b.to_be_bytes());
        }
        let ack_packet = build(1, 0, conn, ack_pn, 0, 0, &frame);
        ack_pn += 1;
        socket.send_to(&ack_packet, src).expect("send failed");

        // Step 5g: finished when FIN has arrived and every 1200-byte chunk of the file has arrived
        if fin_pn.is_some() {
            let needed = (total_len + 1199) / 1200;
            if chunks_got.len() as u64 == needed { break; }
        }
    }

    // Step 6: cut the buffer to the exact file length and save it
    file_data.truncate(total_len as usize);
    fs::write(&out_path, &file_data).expect("could not write file");
    println!("Receiver done: {} bytes saved to {}", file_data.len(), out_path);
    println!("DATA packets accepted {}, packets dropped on purpose (ground truth RANDOM) {}, CE marks {}", accepted, dropped, ce_count); println!("SACK truncations {}", sack_trunc);
}