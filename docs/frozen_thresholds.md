# Frozen LossLens thresholds

Frozen on 2026-10-23 from the tuning grid (seeds 101 to 110, 1 MB file).

- q_frac = 0.2
- slope_ms = 1.0

Selection rule (written in docs/hypotheses.md before the run): balanced accuracy per scenario, mean of the two scenarios, pairs within 0.02 of the best ranked by mean goodput.

Score 0.68 against 0.7 for the default pair (0.1, 1).

Use on the command line: file_sender <file> <addr> losslens 0.2 1.0

No further threshold change is allowed. Test seeds 1 to 10 are used from now on.
