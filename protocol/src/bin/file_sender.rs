use std::collections::VecDeque;
use std::fs;
use std::io::Write;
use std::net::UdpSocket;
use std::time::{Duration, Instant};

fn main() {
    // Step 1: command line: file, receiver address, controller, then the two classifier thresholds
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("usage: file_sender <file> [receiver_addr] [reno|cubic|fixed|losslens] [q_frac] [slope_thr_ms]").clone();
    let server = args.get(2).cloned().unwrap_or("127.0.0.1:9001".to_string());
    let cc = args.get(3).cloned().unwrap_or("reno".to_string());
    if cc != "reno" && cc != "cubic" && cc != "fixed" && cc != "losslens" { panic!("controller must be reno, cubic, fixed or losslens"); }
    let q_frac: f64 = args.get(4).map(|s| s.parse().unwrap()).unwrap_or(0.1);        // queueing delay must exceed q_frac x minRTT
    let slope_thr_ms: f64 = args.get(5).map(|s| s.parse().unwrap()).unwrap_or(1.0);  // and must have risen by this many ms over the last RTT
    let data = fs::read(&path).expect("could not read file");

    // Step 2: settings. A "chunk" is one 1200-byte slice of the file; chunk number total_chunks stands for the FIN
    let mss: usize = 1200;
    let mss_f: f64 = 1200.0;
    let window: usize = 20; // only used by the "fixed" controller
    let conn_id: u32 = 0x1000_0001;
    let total_chunks = (data.len() + mss - 1) / mss;
    let fin_chunk = total_chunks;
    println!("Sending {}: {} bytes = {} chunks + FIN, controller {}", path, data.len(), total_chunks, cc);

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

    // Step 5: congestion control state
    // cwnd = bytes allowed in flight, ssthresh = slow start threshold (infinite until the first loss)
    let mut cwnd: f64 = if cc == "fixed" { window as f64 * mss_f } else { 10.0 * mss_f };
    let mut ssthresh: f64 = f64::MAX;
    // recovery_pn = packets numbered below this were sent before the last cut, so their losses do not cut again
    let mut recovery_pn: usize = 0;
    let mut reductions: u64 = 0;
    let mut max_cwnd: f64 = cwnd;
    // CUBIC only, all window sizes below are in packets
    let mut w_max: f64 = 0.0;
    let mut epoch_start: Option<Instant> = None;
    let mut cubic_k: f64 = 0.0;
    let mut w_est: f64 = 0.0;
    // Classifier state (the verdict is computed for every controller, only LossLens acts on it)
    let mut ll_q_hist: Vec<(f64, f64)> = Vec::new(); // (time in ms, queueing delay in ms), sampled every 5 ms
    let mut rand_streak: u32 = 0;                    // consecutive RANDOM verdicts that led to a cut
    let mut streak_q0: f64 = 0.0;                    // queueing delay when the streak began
    let mut v_cong: u64 = 0;                         // declared losses given the verdict CONGESTION
    let mut v_rand: u64 = 0;                         // declared losses given the verdict RANDOM
    let mut ev_cong: u64 = 0;                        // LossLens cuts decided as congestion
    let mut ev_rand: u64 = 0;                        // LossLens cuts decided as random
    let mut ll_safety: u64 = 0;                      // times the safety net forced a congestion cut

    // Step 6: log files for later analysis
    let mut cwnd_log = fs::File::create("cwnd_log.csv").expect("cannot create cwnd_log.csv");
    writeln!(cwnd_log, "time_ms,cwnd_bytes,ssthresh_bytes,in_flight_packets,srtt_ms").unwrap();
    let mut loss_log = fs::File::create("sender_losses.csv").expect("cannot create sender_losses.csv");
    writeln!(loss_log, "packet_number,time_ms,rule,verdict,q_ms,rise_ms").unwrap();
    let mut last_log = Instant::now();

    // Step 7: sender state. The pkt_ vectors are indexed by packet number
    let clock = Instant::now();
    let mut pkt_chunk: Vec<usize> = Vec::new();      // which chunk this packet carried
    let mut pkt_time: Vec<Instant> = Vec::new();     // when it was sent
    let mut pkt_state: Vec<u8> = Vec::new();         // 0 = in flight, 1 = acked, 2 = declared lost
    let mut chunk_acked = vec![false; total_chunks + 1];
    let mut chunks_acked_count: usize = 0;
    let mut next_new: usize = 0;
    let mut fin_sent = false;
    let mut retx_queue: VecDeque<usize> = VecDeque::new();
    let mut in_flight: usize = 0;
    let mut largest_acked: i64 = -1;
    let mut srtt: f64 = 0.0;
    let mut latest_rtt: f64 = 0.0;
    let mut min_rtt: f64 = f64::MAX;
    let mut have_rtt = false;
    let mut sent_total: u64 = 0;
    let mut retx_total: u64 = 0;
    let mut lost_declared: u64 = 0;
    let mut spurious: u64 = 0;
    let mut ce_echoed: u32 = 0;
    let mut last_ack_time = Instant::now();
    let mut last_rx = Instant::now();
    let mut buf = [0u8; 2000];

    // Step 8: main loop, runs until every chunk and the FIN are acknowledged
    while chunks_acked_count < total_chunks + 1 {

        // Step 8a: send while the window has room (fixed: 20 packets, the others: (in flight + 1) packets must fit in cwnd)
        loop {
            let allowed = if cc == "fixed" { in_flight < window } else { (in_flight as f64 + 1.0) * mss_f <= cwnd };
            if !allowed { break; }
            let mut is_retx = false;
            let mut chunk: Option<usize> = None;
            while let Some(c) = retx_queue.pop_front() {
                if !chunk_acked[c] { chunk = Some(c); is_retx = true; break; }
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

        // Step 8b: wait up to 10 ms for an ACK, keep its frame only if the CRC is good
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

        // Step 8c: process a valid ACK frame
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
                    if p >= pkt_state.len() || pkt_state[p] == 1 { continue; }
                    let was = pkt_state[p];
                    pkt_state[p] = 1;
                    if was == 0 { in_flight -= 1; }
                    if was == 2 { spurious += 1; }
                    let c = pkt_chunk[p];
                    if !chunk_acked[c] { chunk_acked[c] = true; chunks_acked_count += 1; }

                    // Step 8d: window growth, only for a packet that was in flight and was sent after the last cut
                    if (cc == "reno" || cc == "cubic" || cc == "losslens") && was == 0 && p >= recovery_pn {
                        if cwnd < ssthresh {
                            cwnd += mss_f;                          // slow start (all controllers): +1 packet per ACK
                        } else if cc == "cubic" {
                            // CUBIC congestion avoidance (RFC 9438 style)
                            let cw = cwnd / mss_f;
                            if epoch_start.is_none() {
                                epoch_start = Some(Instant::now());
                                if w_max < cw { w_max = cw; }
                                cubic_k = ((w_max - cw) / 0.4).max(0.0).cbrt();
                                w_est = cw;
                            }
                            let t = epoch_start.unwrap().elapsed().as_secs_f64() + srtt / 1000.0;
                            let w_cubic = 0.4 * (t - cubic_k).powi(3) + w_max;
                            w_est += 0.5294 / cw;
                            let target = w_cubic.max(w_est).min(1.5 * cw);
                            let inc = if target > cw { (target - cw) / cw } else { 0.01 / cw };
                            cwnd += inc * mss_f;
                        } else {
                            cwnd += mss_f * mss_f / cwnd;           // Reno and LossLens congestion avoidance: about +1 packet per RTT
                        }
                        if cwnd > max_cwnd { max_cwnd = cwnd; }
                    }

                    // Step 8e: RTT sample from the largest acknowledged packet
                    if pn == largest {
                        let rtt = pkt_time[p].elapsed().as_secs_f64() * 1000.0 - delay_us as f64 / 1000.0;
                        latest_rtt = rtt;
                        if !have_rtt { srtt = rtt; have_rtt = true; } else { srtt = 0.875 * srtt + 0.125 * rtt; }
                        if rtt < min_rtt { min_rtt = rtt; }
                    }
                }
            }
        }

        // Step 8f: loss detection over every packet still in flight
        let time_thresh_ms = if have_rtt { 1.125 * srtt.max(latest_rtt) } else { 500.0 };
        let rto_ms = if have_rtt { (4.0 * srtt).max(200.0) } else { 1000.0 };
        let timed_out = in_flight > 0 && last_ack_time.elapsed().as_secs_f64() * 1000.0 > rto_ms;
        for p in 0..pkt_state.len() {
            if pkt_state[p] != 0 { continue; }
            let age_ms = pkt_time[p].elapsed().as_secs_f64() * 1000.0;
            let by_packets = largest_acked >= p as i64 + 3;
            let by_time = largest_acked > p as i64 && age_ms > time_thresh_ms;
            let by_timeout = timed_out && age_ms > rto_ms;
            if by_packets || by_time || by_timeout {
                pkt_state[p] = 2;
                in_flight -= 1;
                lost_declared += 1;
                if !chunk_acked[pkt_chunk[p]] { retx_queue.push_back(pkt_chunk[p]); }
                let timeout_only = by_timeout && !by_packets && !by_time;

                // Step 8g: the classifier, read the two signals at the moment of the declaration
                // q_now = queueing delay now, q_old = queueing delay about one RTT ago, rise = how much it grew
                let q_now = if have_rtt { (srtt - min_rtt).max(0.0) } else { 0.0 };
                let now_ms = clock.elapsed().as_secs_f64() * 1000.0;
                let mut q_old = q_now;
                for &(t, q) in ll_q_hist.iter().rev() {
                    if t <= now_ms - srtt { q_old = q; break; }  // newest sample that is at least one RTT old
                }
                let rise = q_now - q_old;
                let verdict_cong = have_rtt && q_now > q_frac * min_rtt && rise >= slope_thr_ms;
                if verdict_cong { v_cong += 1; } else { v_rand += 1; }
                let rule = if by_packets { "packets" } else if by_time { "time" } else { "timeout" };
                writeln!(loss_log, "{},{:.1},{},{},{:.2},{:.2}", p, now_ms, rule, if verdict_cong { "CONGESTION" } else { "RANDOM" }, q_now, rise).unwrap();

                // Step 8h: Reno and CUBIC reaction, a loss starts a new event only if the packet was sent after the last cut
                if (cc == "reno" || cc == "cubic") && p >= recovery_pn {
                    let beta = if cc == "cubic" { 0.7 } else { 0.5 };   // fraction of cwnd kept after a loss
                    if cc == "cubic" {
                        let cw = cwnd / mss_f;
                        w_max = if cw < w_max { cw * (1.0 + 0.7) / 2.0 } else { cw };
                        epoch_start = None;
                    }
                    ssthresh = (cwnd * beta).max(2.0 * mss_f);
                    cwnd = if timeout_only { 2.0 * mss_f } else { ssthresh };
                    recovery_pn = pkt_state.len();
                    reductions += 1;
                }

                // Step 8i: LossLens reaction, same once-per-RTT rule, but the cut depends on the verdict
                if cc == "losslens" && p >= recovery_pn {
                    let mut cong = verdict_cong || timeout_only;       // a timeout is always treated as congestion
                    if cong {
                        rand_streak = 0;
                    } else {
                        rand_streak += 1;
                        if rand_streak == 1 { streak_q0 = q_now; }
                        if rand_streak >= 3 && q_now > streak_q0 + slope_thr_ms {
                            cong = true;                               // safety net: 3 RANDOM verdicts in a row while the delay keeps climbing
                            ll_safety += 1;
                            rand_streak = 0;
                        }
                    }
                    if cong {
                        ssthresh = (cwnd * 0.7).max(2.0 * mss_f);
                        cwnd = if timeout_only { 2.0 * mss_f } else { ssthresh };
                        ev_cong += 1;
                    } else {
                        cwnd = (cwnd * 0.9).max(2.0 * mss_f);          // mild cut, ssthresh untouched so slow start can continue
                        ev_rand += 1;
                    }
                    recovery_pn = pkt_state.len();
                    reductions += 1;
                }
            }
        }
        if timed_out { last_ack_time = Instant::now(); }

        // Step 8j: every 5 ms write one cwnd log row and remember the queueing delay for the classifier
        if last_log.elapsed() >= Duration::from_millis(5) {
            let ss = if ssthresh > 1e12 { -1.0 } else { ssthresh };
            let now_ms = clock.elapsed().as_secs_f64() * 1000.0;
            writeln!(cwnd_log, "{:.1},{:.0},{:.0},{},{:.2}", now_ms, cwnd, ss, in_flight, srtt).unwrap();
            let qs = if have_rtt { (srtt - min_rtt).max(0.0) } else { 0.0 };
            ll_q_hist.push((now_ms, qs));
            last_log = Instant::now();
        }

        // Step 8k: give up if nothing valid has arrived for 10 seconds
        if last_rx.elapsed().as_secs() >= 10 {
            println!("Stalled: no ACKs for 10 s. Is the receiver running? ({} of {} acked)", chunks_acked_count, total_chunks + 1);
            break;
        }
    }

    // Step 9: report
    let secs = clock.elapsed().as_secs_f64();
    println!("Done: {} bytes in {:.3} s, goodput {:.2} Mbps", data.len(), secs, data.len() as f64 * 8.0 / secs / 1e6);
    if have_rtt {
        println!("srtt {:.3} ms, min RTT {:.3} ms, CE marks echoed {}", srtt, min_rtt, ce_echoed);
    }
    println!("Packets sent {}, retransmissions {} ({:.2}%), losses declared {}, spurious {}",
        sent_total, retx_total, retx_total as f64 * 100.0 / sent_total as f64, lost_declared, spurious);
    println!("Controller {}: cwnd reductions {}, max cwnd {:.1} packets, final cwnd {:.1} packets",
        cc, reductions, max_cwnd / mss_f, cwnd / mss_f);
    println!("Classifier verdicts on declared losses: CONGESTION {}, RANDOM {} (queueing delay above {} x minRTT and risen by at least {} ms)",
        v_cong, v_rand, q_frac, slope_thr_ms);
    if cc == "losslens" {
        println!("LossLens cuts: congestion {}, random {}, safety net {}", ev_cong, ev_rand, ll_safety);
    }
}