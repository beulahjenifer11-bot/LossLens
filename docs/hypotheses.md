# Hypotheses (written BEFORE running each experiment)

## Step 15, written before running
Setup: rate 2 Mbps, one-way delay 20 ms, buffer 10 packets, fixed window 20, no random loss.
Prediction: BDP = 2 Mbps x 40 ms = 10,000 bytes, about 8 packets. Pipe 8 + buffer 10 = 18 < window 20, so the queue must overflow, max queue depth will reach 10, srtt will rise well above the 40 ms min RTT, and goodput will be capped near 1.9 Mbps.

### Step 15 outcome
All predictions held: max queue depth 10 of 10, srtt 87.3 ms vs min RTT 45.5 ms, goodput 1.90 Mbps, 49 QUEUE_OVERFLOW drops. Min RTT was above 40 ms because of 4.9 ms serialization delay at 2 Mbps, which I had not included in the prediction.
