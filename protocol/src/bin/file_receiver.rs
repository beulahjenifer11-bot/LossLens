use std::collections::BTreeSet;
use std::fs;
use std::net::UdpSocket;
use std::time::Instant;

fn main() {
    // Step 1: output file name from the command line, and a UDP socket on port 9001
    let out_path = std::env::args().nth(1).unwrap_or("received.bin".to_string());
    let socket = UdpSocket::bind("0.0.0.0:9001").expect("bind failed");
    println!("Receiver listening on port 9001, will write to {}", out_path);

    // Step 2: CRC32 calculator (same as packet_demo)
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

    // Step 3: packet builder, writes the 37-byte header then the payload, then fills in the CRC
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
    let mut file_data: Vec<u8> = Vec::new();   // the file being rebuilt, bytes placed by stream offset
    let mut got: BTreeSet<u64> = BTreeSet::new(); // packet numbers received so far, kept sorted
    let mut total_len: u64 = 0;                 // file length, learned from the FIN packet
    let mut fin_pn: Option<u64> = None;         // packet number of the FIN
    let mut ack_pn: u64 = 0;                    // our own numbering for the ACK packets we send
    let mut data_packets: u64 = 0;
    let mut ce_count: u32 = 0;                  // CE-marked packets seen (stays 0 until the emulator marks)
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

        // Step 5b: verify the CRC, drop the packet if it is corrupted
        let mut check = rx[..37 + plen].to_vec();
        check[33..37].copy_from_slice(&[0u8; 4]);
        if crc32(&check) != stored_crc {
            println!("Packet {} failed CRC, dropped", pn);
            continue;
        }
        if rx[2] & 0b10 != 0 { ce_count += 1; } // CE flag is bit 1

        // Step 5c: act on the packet type
        if ptype == 0 {
            // DATA: place the payload at its stream offset
            let end = off as usize + plen;
            if file_data.len() < end { file_data.resize(end, 0); }
            file_data[off as usize..end].copy_from_slice(&rx[37..37 + plen]);
            if got.insert(pn) { data_packets += 1; }
        } else if ptype == 4 {
            // FIN: the offset field carries the total file length
            total_len = off;
            fin_pn = Some(pn);
            got.insert(pn);
        } else {
            continue;
        }

        // Step 5d: build SACK ranges from the sorted set of received packet numbers
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
        if ranges.len() > 50 { ranges = ranges.split_off(ranges.len() - 50); } // keep newest 50 ranges
        let largest = *got.iter().next_back().unwrap();

        // Step 5e: encode the ACK frame (same layout as ack_demo) and send it back
        let mut frame: Vec<u8> = Vec::new();
        frame.extend_from_slice(&largest.to_be_bytes());
        frame.extend_from_slice(&0u32.to_be_bytes()); // ack delay is 0, we ACK immediately
        frame.extend_from_slice(&ce_count.to_be_bytes());
        frame.extend_from_slice(&(ranges.len() as u16).to_be_bytes());
        for &(a, b) in ranges.iter() {
            frame.extend_from_slice(&a.to_be_bytes());
            frame.extend_from_slice(&b.to_be_bytes());
        }
        let ack_packet = build(1, 0, conn, ack_pn, 0, 0, &frame);
        ack_pn += 1;
        socket.send_to(&ack_packet, src).expect("send failed");

        // Step 5f: finished when the FIN arrived and every packet number 0..=fin has been received
        if let Some(f) = fin_pn {
            if got.len() as u64 == f + 1 { break; }
        }
    }

    // Step 6: cut the buffer to the exact file length and save it
    file_data.truncate(total_len as usize);
    fs::write(&out_path, &file_data).expect("could not write file");
    println!("Receiver done: {} bytes, {} data packets, {} CE marks, saved to {}", file_data.len(), data_packets, ce_count, out_path);
}