fn main() {
    // Step 1: CRC32 calculator, used for encoding, verifying, and the corruption test
    // it processes every byte bit by bit using the standard CRC32 polynomial
    let crc32 = |data: &[u8]| -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        for &b in data.iter() {
            crc ^= b as u32;
            for _ in 0..8 {
                if crc & 1 == 1 {
                    crc = (crc >> 1) ^ 0xEDB8_8320;
                } else {
                    crc >>= 1;
                }
            }
        }
        !crc
    };

    // Step 2: the values we want to put in the packet
    let version: u8 = 1;
    let ptype: u8 = 0; // 0 means DATA
    let flags: u8 = 0b0000_0011; // bit0 ECT and bit1 CE are both on
    let conn_id: u32 = 0xABCD1234;
    let packet_number: u64 = 42;
    let stream_offset: u64 = 1200 * 42;
    let send_ts: u64 = 1_500_000; // microseconds
    let payload: Vec<u8> = b"hello losslens".to_vec();
    let payload_len: u16 = payload.len() as u16;

    // Step 3: ENCODE, write each field in order, big-endian
    // layout: version(1) type(1) flags(1) conn(4) pktnum(8) offset(8) ts(8) len(2) crc(4) payload
    let mut pkt: Vec<u8> = Vec::new();
    pkt.push(version);
    pkt.push(ptype);
    pkt.push(flags);
    pkt.extend_from_slice(&conn_id.to_be_bytes());
    pkt.extend_from_slice(&packet_number.to_be_bytes());
    pkt.extend_from_slice(&stream_offset.to_be_bytes());
    pkt.extend_from_slice(&send_ts.to_be_bytes());
    pkt.extend_from_slice(&payload_len.to_be_bytes());
    pkt.extend_from_slice(&[0u8; 4]); // CRC placeholder (bytes 33 to 36), filled in below
    pkt.extend_from_slice(&payload);

    // Step 4: compute the CRC over the whole packet (CRC field is zero right now) and store it
    let crc = crc32(&pkt);
    pkt[33..37].copy_from_slice(&crc.to_be_bytes());
    println!("Encoded packet length: {} bytes", pkt.len());

    // Step 5: DECODE, pretend these bytes just arrived from the network
    let rx = pkt.clone();
    assert!(rx.len() >= 37, "packet shorter than the 37-byte header");
    let d_version = rx[0];
    let d_type = rx[1];
    let d_flags = rx[2];
    let d_conn = u32::from_be_bytes(rx[3..7].try_into().unwrap());
    let d_pn = u64::from_be_bytes(rx[7..15].try_into().unwrap());
    let d_off = u64::from_be_bytes(rx[15..23].try_into().unwrap());
    let d_ts = u64::from_be_bytes(rx[23..31].try_into().unwrap());
    let d_len = u16::from_be_bytes(rx[31..33].try_into().unwrap());
    let d_crc = u32::from_be_bytes(rx[33..37].try_into().unwrap());
    let d_payload = &rx[37..37 + d_len as usize];
    println!(
        "Decoded: version={} type={} flags={} conn_id={:#X} packet_number={} stream_offset={} send_ts={} payload_len={}",
        d_version, d_type, d_flags, d_conn, d_pn, d_off, d_ts, d_len
    );
    println!("Payload text: {}", String::from_utf8_lossy(d_payload));

    // Step 6: VERIFY, zero the CRC field again, recompute, compare with the stored CRC
    let mut check = rx.clone();
    check[33..37].copy_from_slice(&[0u8; 4]);
    let good = crc32(&check) == d_crc;
    println!("CRC check on good packet: {}", if good { "OK" } else { "FAILED" });

    // Step 7: CORRUPTION TEST, flip one bit in the payload and verify again
    let mut bad = rx.clone();
    let last = bad.len() - 1;
    bad[last] ^= 0b0000_0001; // flip the lowest bit of the last byte
    let stored_crc = u32::from_be_bytes(bad[33..37].try_into().unwrap());
    bad[33..37].copy_from_slice(&[0u8; 4]);
    let detected = crc32(&bad) != stored_crc;
    println!(
        "CRC check after flipping one bit: {}",
        if detected { "FAILED (corruption detected)" } else { "OK (BAD, not detected)" }
    );

    // Step 8: final checks, the program stops with an error if any of these is false
    assert_eq!(d_pn, packet_number);
    assert_eq!(d_conn, conn_id);
    assert_eq!(d_payload, &payload[..]);
    assert!(good);
    assert!(detected);
    println!("All checks passed");
}