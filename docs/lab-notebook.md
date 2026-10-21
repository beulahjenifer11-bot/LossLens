# Lab notebook

## 2026-10-20: Project started, tools installed (git, python, cargo).

## 2026-10-20: Git and GitHub set up. Rust toolchain works after installing C++ Build Tools (fixed link.exe error). protocol crate created.

## 2026-10-21: UDP echo tested on loopback and across two laptops over WiFi. Observed RTT range: 0.161 ms.

## 2026-10-21: UDP echo tested across two laptops over WiFi. Server received 4 of 5 pings. Replies to pings 1 and 2 arrived seconds late in a burst, so the echo client matched them to the wrong pings and printed false RTTs (304 ms, 0.161 ms). Lesson: unique packet numbers are needed so every ACK maps to exactly one transmission. Firewall rule for UDP 9000 worked.

## 2026-10-21: Wrote packet_demo: encodes and decodes the 37-byte header, CRC32 detects a single flipped bit. Layout matches docs/design.md.

## 2026-10-21: Wrote ack_demo: SACK ranges built from received packets, encoded to 66 bytes, decoded, packet threshold marks packet 4 lost while 7 and 8 stay pending.

## 2026-10-21: Wrote file_sender and file_receiver: fixed window of 20 packets, SACK ACKs, FIN with total length. 1 MB random file transferred over loopback, SHA256 of sent and received files identical.

## 2026-10-21: Wrote file_sender and file_receiver: fixed window of 20 packets, SACK ACKs, FIN with total length. 1 MB random file sent over loopback in 0.600 s (13.98 Mbps), matches window x MSS / srtt. srtt 15.2 ms vs min RTT 5.58 ms is inflated by debug-build processing time, so final experiments must use cargo build --release.

## 2026-10-21: Step 12 loss detection and retransmission: packet threshold 3, time threshold 9/8 RTT, timeout max(4 x srtt, 200 ms), new packet number per retransmission. Receiver has a temporary seeded random-drop switch that counts ground-truth drops. Results (fill in): 0% loss hash ___, 5% loss hash ___, dropped ___, losses declared ___, spurious ___, retransmission rate ___%.

## 2026-10-21: Step 13 repeatability test (experiments/verify_transfer.ps1): 4 loss rates x 3 seeds = 12 transfers of a 1 MB file, hash checked each time. Result: ___ passed, ___ failed. Goodput at 0% = ___ Mbps, at 10% = ___ Mbps.

## 2026-10-21: Step 14 emulator v1: UDP relay on port 9100 with fixed one-way delay, seeded Bernoulli loss and ground-truth drop log (emulator_drops.csv). Found that loopback RTT is too small to show the loss penalty (222 Mbps at 10% loss), so realistic delay is required before Reno. Results (fill in): 0% loss goodput ___ Mbps, 5% loss goodput ___ Mbps, emulator dropped ___, sender declared ___ lost.

## 2026-10-21: Step 15 emulator v2 results. Test 1 (20 Mbps, buffer 80): goodput 4.62 Mbps, srtt 41.3 ms, min RTT 40.6 ms, 0 drops, max queue 19 (start-up burst of 20 packets). Test 2 (2 Mbps, buffer 10): goodput 1.90 Mbps, srtt 87.3 ms vs min RTT 45.5 ms, 49 QUEUE_OVERFLOW drops, 0 RANDOM, sender declared 49 lost, spurious 0, hash identical. Hypothesis confirmed.

## 2026-10-21: Step 15 Test 3 (2 Mbps, buffer 10, 1% random loss): emulator dropped 12 RANDOM + 43 QUEUE_OVERFLOW, sender declared 55 lost, spurious 0, goodput 1.92 Mbps, srtt 89.4 ms vs min RTT 45.3 ms, hash identical. The sender detects every loss but cannot tell the cause: 22% of losses were random. This is the gap the LossLens classifier must close.

## 2026-10-21: Step 16 Reno results. Added cwnd, slow start, AIMD, one cut per RTT and timeout to 2 packets in file_sender, with the controller chosen by a command-line flag (reno or fixed), plus cwnd_log.csv and sender_losses.csv. Test A (2 Mbps, buffer 10, no random loss): 31 QUEUE_OVERFLOW drops versus 49 with the fixed window, 472 cwnd log rows, hash identical. Test B (20 Mbps, buffer 80, 1% random loss): goodput 3.53 Mbps (about 18 percent of capacity, Mathis estimate 2.9 Mbps), 11 RANDOM drops, 0 QUEUE_OVERFLOW, losses declared 11, spurious 0, 8 cwnd reductions, max cwnd 87 packets, final cwnd 8.9 packets, srtt 41.0 ms vs min RTT 40.3 ms, max queue depth 38 of 80 (slow start burst, no pacing yet), hash identical. Finding: all 8 cuts in Test B were unnecessary because the link was never congested. This is the gap LossLens must close.

## 2026-10-21: Step 17 plots. Python venv set up (.venv, analysis/requirements.txt). analysis/plot_cwnd.py draws cwnd and ssthresh, and joins sender_losses.csv with emulator_drops.csv by packet number to label each declared loss with its true cause. Results (fill in): Test A declared ___ losses, ___ QUEUE_OVERFLOW; Test B declared 11 losses, ___ RANDOM. Mathis estimate for Test B 2.63 Mbps vs measured 3.53 Mbps, ratio ___. Figures in docs/figures.

## 2026-10-21: Step 17 results. Figures in docs/figures, drawn by analysis/plot_cwnd.py, which joins sender_losses.csv with emulator_drops.csv by packet number. Test A (2 Mbps, buffer 10): classic Reno sawtooth. Slow start reached 40 packets at about 0.2 s, a cluster of overflow losses cut cwnd to about 20 then about 11, then steady cycles between about 10.5 and 21 packets every 0.9 s. Every declared loss was QUEUE_OVERFLOW. The peak of 21 matches the pipe (about 9.4 packets) plus the buffer (10), and halving gives about 10.5, close to the pipe size, so the link stays busy after each cut. Test B (20 Mbps, 1% random loss): slow start peaked at about 82 packets at 0.17 s, then three random losses in quick succession cut cwnd 82 to 43 to 22 to 11 within 0.15 s. After that cwnd stayed between 5 and 23 packets while the BDP is 83 packets, so at most about 28 percent of the pipe was used. Every declared loss was RANDOM, so every cut was unnecessary. By my calculation the Mathis estimate at the observed loss rate is 2.63 Mbps against 3.53 Mbps measured. The gap is likely due to the short 1 MB run and the start-up climb.
