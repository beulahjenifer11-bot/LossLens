# Lab notebook

## 2026-10-20: Project started, tools installed (git, python, cargo).

## 2026-10-20: Git and GitHub set up. Rust toolchain works after installing C++ Build Tools (fixed link.exe error). protocol crate created.

## 2026-10-21: UDP echo tested on loopback and across two laptops over WiFi. Observed RTT range: 0.161 ms.

## 2026-10-21: UDP echo tested across two laptops over WiFi. Server received 4 of 5 pings. Replies to pings 1 and 2 arrived seconds late in a burst, so the echo client matched them to the wrong pings and printed false RTTs (304 ms, 0.161 ms). Lesson: unique packet numbers are needed so every ACK maps to exactly one transmission. Firewall rule for UDP 9000 worked.

## 2026-10-21: Wrote packet_demo: encodes and decodes the 37-byte header, CRC32 detects a single flipped bit. Layout matches docs/design.md.

## 2026-10-21: Wrote ack_demo: SACK ranges built from received packets, encoded to 66 bytes, decoded, packet threshold marks packet 4 lost while 7 and 8 stay pending.
