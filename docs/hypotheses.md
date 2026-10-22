# Hypotheses (written BEFORE running each experiment)

## Step 15, written before running
Setup: rate 2 Mbps, one-way delay 20 ms, buffer 10 packets, fixed window 20, no random loss.
Prediction: BDP = 2 Mbps x 40 ms = 10,000 bytes, about 8 packets. Pipe 8 + buffer 10 = 18 < window 20, so the queue must overflow, max queue depth will reach 10, srtt will rise well above the 40 ms min RTT, and goodput will be capped near 1.9 Mbps.

### Step 15 outcome
All predictions held: max queue depth 10 of 10, srtt 87.3 ms vs min RTT 45.5 ms, goodput 1.90 Mbps, 49 QUEUE_OVERFLOW drops. Min RTT was above 40 ms because of 4.9 ms serialization delay at 2 Mbps, which I had not included in the prediction.

## Step 15 Test 3, written before running
Setup: 2 Mbps, 20 ms one-way delay, buffer 10, 1% random loss, fixed window 20.
Prediction: the label count shows both RANDOM (about 9 packets, 1% of 875) and QUEUE_OVERFLOW (about 45 to 55). The sender declares about the sum as lost, spurious stays near 0, and goodput stays near 1.8 Mbps, because the queue is full either way.

### Step 15 Test 3 outcome
Both causes appeared: 12 RANDOM and 43 QUEUE_OVERFLOW. Sender declared 55 lost, spurious 0. Prediction was partly off: RANDOM was 12 not about 9 (seed variation, expected 8.75 with spread about 3), QUEUE_OVERFLOW was 43 not 45 to 55 (random drops happen before the queue, so fewer packets reached it), goodput 1.92 Mbps was above 1.8 because the 2 Mbps link carries at most about 1.94 Mbps of file data.

## Step 16, written before running
Test A: 2 Mbps, 20 ms one-way, buffer 10, 0% random loss, Reno. Prediction: cwnd climbs in slow start until the first overflow, then oscillates (sawtooth). Fewer QUEUE_OVERFLOW drops than the fixed window's 49, srtt below the fixed window's 87 ms, goodput between 1.5 and 1.9 Mbps.
Test B: 20 Mbps, 20 ms one-way, buffer 80, 1% random loss, Reno. Prediction: goodput far below 20 Mbps, near the Mathis estimate of about 2.9 Mbps (range 2 to 4 Mbps), because every random loss halves cwnd. Cwnd reductions roughly equal to the number of random drops.

### Step 16 outcome
Test B (20 Mbps, buffer 80, 1% random loss, Reno): goodput 3.53 Mbps, about 18 percent of capacity, inside the predicted 2 to 4 Mbps range and close to the Mathis estimate of 2.9 Mbps. Prediction partly off: cwnd reductions were 8, not about equal to the 11 random drops, because losses within one RTT count as a single congestion event. Test A (2 Mbps, buffer 10): 31 QUEUE_OVERFLOW drops versus 49 for the fixed window, so Reno backs off as predicted. Remaining Test A predictions (srtt, goodput range) to be checked against sender output.

## Step 18 (scenario S2 random loss sweep), written before running
Setup: 20 Mbps, 40 ms RTT, buffer 80 packets, 5 MB file, controllers reno and fixed window of 20, loss 0, 0.1, 0.5, 1, 2, 5 percent.
Mathis estimates for Reno at 1200 byte MSS and 40 ms RTT: 9.3 Mbps at 0.1 percent, 4.1 at 0.5, 2.9 at 1, 2.1 at 2, 1.3 at 5.
Prediction 1 (H1): Reno goodput falls steeply with loss and is below 5 Mbps (25 percent of capacity) at 1 percent loss, within a factor of 2 of the Mathis estimates.
Prediction 2: the fixed window is capped near 4.6 Mbps at 0 percent loss and falls only mildly with loss, so Reno wins at 0 and 0.1 percent loss, the fixed window wins from 1 percent upward, and the crossover lies between 0.1 and 1 percent.
Prediction 3: every transfer passes the hash check, and losses declared match the emulator drop count with spurious near 0.

### Step 18 outcome (full run, 120 runs, seeds 1 to 10)
Prediction 1 supported: Reno at 1 percent loss reached 2.82 Mbps (95 percent CI 2.65 to 2.99), 14.1 percent of capacity, below the 25 percent limit. Reno was within a factor of 2 of the Mathis estimate at every loss rate (ratio 1.18, 1.02, 0.96, 0.91, 0.85 at 0.1, 0.5, 1, 2, 5 percent). Ratio below 1 at higher loss is probably from timeouts, which Mathis ignores, and above 1 at 0.1 percent from the slow start share of a 5 MB run.
Prediction 2 mostly supported: the fixed window stayed between 4.37 and 4.66 Mbps at all loss rates, and Reno won at 0 and 0.1 percent (18.26 and 10.97 Mbps against 4.65 and 4.66). The crossover is earlier than predicted: at 0.5 percent the fixed window was already ahead (4.64 against 4.24, intervals do not overlap), so it lies between 0.1 and 0.5 percent, not at 1 percent.
Prediction 3: spurious losses were 0.00 in all 12 settings (n=10 each). Hash and drop count checks to be confirmed from the script output.
Unexpected finding: Reno retransmitted 3.58 percent of packets at 0 percent random loss and 3.00 percent at 0.1 percent, so at low random loss most of its losses are congestion drops from slow start overshoot (no pacing yet). The fixed window retransmission rate matched the random loss rate (0.10, 0.53, 1.07, 2.05, 5.11 percent) because 20 packets never overflow the buffer.

## Step 22 (CUBIC), written before running
Test A: 2 Mbps, 20 ms one-way, buffer 10, no random loss, 1 MB file, cubic.
Prediction 1: cwnd falls to about 70 percent of its peak (about 14 to 15 packets from a peak near 21), which is above the pipe size of about 9.4 packets, so the buffer never fully drains. srtt should therefore be about 70 to 90 ms against a min RTT near 45 ms, and goodput should stay near 1.9 Mbps.
Prediction 2: at this small bandwidth delay product the sender runs in the TCP-friendly region, so the cwnd plot looks like a shallower sawtooth with a roughly straight ramp, not the cubic curve. The cubic shape only shows at large bandwidth delay products.
Prediction 3 (checked in the next step with the full runner): at 1 percent random loss, cubic goodput is within 20 percent of Reno (2.3 to 3.4 Mbps), because the TCP-friendly region is designed to match Reno at small windows.

### Step 22 outcome (CUBIC Test A, single run)
Prediction 1 partly confirmed: cwnd troughs sat at about 14.4 packets after peaks of about 20.5, which is 70 percent and well above the pipe size of about 9.4 packets, so the buffer never drained. Goodput and srtt were not checked yet and will come from the full runner.
Prediction 2 confirmed: the steady ramps are nearly straight, so the sender stayed in the TCP-friendly region. Reading the graphs, CUBIC grew at about 5.6 packets per second against about 13 for Reno (ratio about 0.43), near the 0.53 packets per RTT given by the RFC 9438 formula 3 x (1 - beta) / (1 + beta).
Ground truth join: 35 packets dropped by the emulator, 35 declared lost by the sender, all QUEUE_OVERFLOW. Rules that fired: packets 33, time 2, no timeout.
Observation: CUBIC needed three cuts during the start-up overshoot (about 39 to 29 to 20.5 to 14.7 packets) because keeping 70 percent after the first loss still left cwnd above the pipe plus buffer of about 19 packets.

### Step 22 outcome (CUBIC Test A, single run)
Prediction 1 partly confirmed: cwnd troughs sat at about 14.4 packets after peaks of about 20.5, which is 70 percent and well above the pipe size of about 9.4 packets, so the buffer never drained. Goodput and srtt were not checked yet and will come from the full runner.
Prediction 2 confirmed: the steady ramps are nearly straight, so the sender stayed in the TCP-friendly region. Reading the graphs, CUBIC grew at about 5.6 packets per second against about 13 for Reno (ratio about 0.43), near the 0.53 packets per RTT given by the RFC 9438 formula 3 x (1 - beta) / (1 + beta).
Ground truth join: 35 packets dropped by the emulator, 35 declared lost by the sender, all QUEUE_OVERFLOW. Rules that fired: packets 33, time 2, no timeout.
Observation: CUBIC needed three cuts during the start-up overshoot (about 39 to 29 to 20.5 to 14.7 packets) because keeping 70 percent after the first loss still left cwnd above the pipe plus buffer of about 19 packets.
