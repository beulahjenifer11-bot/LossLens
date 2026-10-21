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
