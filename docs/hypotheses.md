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
