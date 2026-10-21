fn main() {
    // Step 1: pretend the receiver got these packet numbers (4, 7 and 8 never arrived)
    let received: Vec<u64> = vec![0, 1, 2, 3, 5, 6, 9];
    let ack_delay_us: u32 = 2500; // receiver waited 2.5 ms before sending this ACK
    let ce_count: u32 = 3;        // total CE-marked packets seen so far (AccECN style)
    println!("Received: {:?}", received);

    // Step 2: RECEIVER SIDE, group consecutive numbers into SACK ranges
    // start and prev track the run we are currently building
    let mut ranges: Vec<(u64, u64)> = Vec::new();
    let mut start = received[0];
    let mut prev = received[0];
    for &pn in received.iter().skip(1) {
        if pn == prev + 1 {
            prev = pn; // still consecutive, extend the current run
        } else {
            ranges.push((start, prev)); // gap found, close the run
            start = pn;
            prev = pn;
        }
    }
    ranges.push((start, prev)); // close the last run
    let largest: u64 = *received.last().unwrap();
    println!("SACK ranges: {:?}", ranges);

    // Step 3: ENCODE, layout: largest(8) ack_delay(4) ce_count(4) range_count(2) then 16 bytes per range
    let mut frame: Vec<u8> = Vec::new();
    frame.extend_from_slice(&largest.to_be_bytes());
    frame.extend_from_slice(&ack_delay_us.to_be_bytes());
    frame.extend_from_slice(&ce_count.to_be_bytes());
    frame.extend_from_slice(&(ranges.len() as u16).to_be_bytes());
    for &(s, e) in ranges.iter() {
        frame.extend_from_slice(&s.to_be_bytes());
        frame.extend_from_slice(&e.to_be_bytes());
    }
    println!("Encoded ACK frame length: {} bytes", frame.len());

    // Step 4: DECODE, read the fields back from the bytes
    let d_largest = u64::from_be_bytes(frame[0..8].try_into().unwrap());
    let d_delay = u32::from_be_bytes(frame[8..12].try_into().unwrap());
    let d_ce = u32::from_be_bytes(frame[12..16].try_into().unwrap());
    let d_n = u16::from_be_bytes(frame[16..18].try_into().unwrap()) as usize;
    let mut d_ranges: Vec<(u64, u64)> = Vec::new();
    for i in 0..d_n {
        let off = 18 + i * 16; // each range starts 16 bytes after the previous one
        let s = u64::from_be_bytes(frame[off..off + 8].try_into().unwrap());
        let e = u64::from_be_bytes(frame[off + 8..off + 16].try_into().unwrap());
        d_ranges.push((s, e));
    }
    println!("Decoded: largest={} ack_delay={} us ce_count={} ranges={:?}", d_largest, d_delay, d_ce, d_ranges);

    // Step 5: SENDER SIDE, mark every packet number covered by a range as acknowledged
    let mut acked = vec![false; (d_largest + 1) as usize];
    for &(s, e) in d_ranges.iter() {
        for pn in s..=e {
            acked[pn as usize] = true;
        }
    }

    // Step 6: every packet not acknowledged is either LOST or still pending
    // packet threshold rule: lost if largest_acked >= pn + 3
    let mut lost: Vec<u64> = Vec::new();
    for pn in 0..=d_largest {
        if !acked[pn as usize] {
            if d_largest >= pn + 3 {
                println!("Packet {}: LOST (largest acked {} is at least {} + 3)", pn, d_largest, pn);
                lost.push(pn);
            } else {
                println!("Packet {}: pending (needs largest acked >= {})", pn, pn + 3);
            }
        }
    }

    // Step 7: final checks, the program stops with an error if any is false
    assert_eq!(d_largest, largest);
    assert_eq!(d_ranges, ranges);
    assert_eq!(d_ce, ce_count);
    assert_eq!(lost, vec![4u64]);
    println!("All checks passed");
}