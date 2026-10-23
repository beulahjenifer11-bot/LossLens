import csv
import datetime
import random


def main():
    # Step 1: read the tuning table and check it
    random.seed(0)
    with open("results/tuning_grid.csv") as f:
        rows = list(csv.DictReader(f))
    bad = [r for r in rows if r["hash_ok"] != "True"]
    unlabelled = sum(int(r["unlabelled"]) for r in rows)
    print("Rows:", len(rows), "| hash failures:", len(bad), "| unlabelled declared losses:", unlabelled)

    # Step 2: organise by (scenario, q_frac, slope, seed) -> (tp, fn, fp, tn, goodput)
    scenarios = ["mixed", "fast"]
    qs = sorted(set(float(r["q_frac"]) for r in rows))
    slopes = sorted(set(float(r["slope_ms"]) for r in rows))
    seeds = sorted(set(int(r["seed"]) for r in rows))
    data = {}
    for r in rows:
        key = (r["scenario"], float(r["q_frac"]), float(r["slope_ms"]), int(r["seed"]))
        data[key] = (int(r["tp"]), int(r["fn"]), int(r["fp"]), int(r["tn"]), float(r["goodput_mbps"]))
    print("Distinct runs found:", len(data), "(expected", len(scenarios) * len(qs) * len(slopes) * len(seeds), ")")

    # Step 3: pooled counts [tp, fn, fp, tn] over a list of seeds, and balanced accuracy from those counts
    # tp = overflow drop called CONGESTION, fn = overflow called RANDOM, fp = random called CONGESTION, tn = random called RANDOM
    pooled = lambda sc, q, s, sl: [sum(data[(sc, q, s, sd)][i] for sd in sl) for i in range(4)]
    bal = lambda c: 0.5 * (c[0] / max(1, c[0] + c[1]) + c[3] / max(1, c[3] + c[2]))

    # Step 4: one table per scenario, pooled over all seeds
    stats = {}
    for sc in scenarios:
        print()
        print(sc, "scenario, pooled over", len(seeds), "seeds")
        print("q_frac slope  cong_rec rand_rec  bal   Mbps")
        for q in qs:
            for s in slopes:
                c = pooled(sc, q, s, seeds)
                cr = c[0] / max(1, c[0] + c[1])
                rr = c[3] / max(1, c[3] + c[2])
                gp = sum(data[(sc, q, s, sd)][4] for sd in seeds) / len(seeds)
                stats[(sc, q, s)] = (cr, rr, bal(c), gp)
                print(f"{q:>6g} {s:>5g}  {cr:>8.2f} {rr:>8.2f} {bal(c):>5.2f} {gp:>6.2f}")

    # Step 5: score every pair (mean of the two scenario balanced accuracies) and print them, best first
    pair = {}
    for q in qs:
        for s in slopes:
            score = (stats[("mixed", q, s)][2] + stats[("fast", q, s)][2]) / 2
            mean_gp = (stats[("mixed", q, s)][3] + stats[("fast", q, s)][3]) / 2
            pair[(q, s)] = (score, mean_gp)
    print()
    print("Pair scores, best first")
    print("q_frac slope  mixed  fast  score  mean Mbps")
    for (q, s), (score, mgp) in sorted(pair.items(), key=lambda kv: -kv[1][0]):
        print(f"{q:>6g} {s:>5g}  {stats[('mixed', q, s)][2]:>5.2f} {stats[('fast', q, s)][2]:>5.2f} {score:>6.3f} {mgp:>8.2f}")

    # Step 6: the selection rule: pairs within 0.02 of the best score are ranked by mean goodput
    best_score = max(v[0] for v in pair.values())
    candidates = [k for k, v in pair.items() if v[0] >= best_score - 0.02]
    winner = max(candidates, key=lambda k: pair[k][1])
    default = (0.1, 1.0)
    print()
    print("Best score:", round(best_score, 3), "| candidates within 0.02:", candidates)
    print("WINNER: q_frac", winner[0], "slope", winner[1], "ms (score", round(pair[winner][0], 3), ")")
    print("Default pair (0.1, 1) score:", round(pair[default][0], 3))

    # Step 7: bootstrap over seeds: resample the 10 seeds with replacement 2000 times, score winner and default each time
    diffs, wins = [], []
    for _ in range(2000):
        rs = [random.choice(seeds) for _ in seeds]
        w = sum(bal(pooled(sc, winner[0], winner[1], rs)) for sc in scenarios) / 2
        d = sum(bal(pooled(sc, default[0], default[1], rs)) for sc in scenarios) / 2
        wins.append(w)
        diffs.append(w - d)
    wins.sort()
    diffs.sort()
    print("Winner score 95% CI over seeds: [", round(wins[50], 3), ",", round(wins[1949], 3), "]")
    print("Winner minus default, 95% CI over seeds: [", round(diffs[50], 3), ",", round(diffs[1949], 3), "]")

    # Step 8: Prediction 6, within each q_frac and scenario compare the lowest and highest slope
    print()
    print("Prediction 6 (higher slope raises random recall and lowers congestion recall)")
    held = 0
    for sc in scenarios:
        for q in qs:
            lo = stats[(sc, q, slopes[0])]
            hi = stats[(sc, q, slopes[-1])]
            ok = hi[1] > lo[1] and hi[0] < lo[0]
            held += 1 if ok else 0
            print(sc, "q", q, ": cong_rec", round(lo[0], 2), "->", round(hi[0], 2), ", rand_rec", round(lo[1], 2), "->", round(hi[1], 2), "", "HOLDS" if ok else "DOES NOT HOLD")
    print("Held in", held, "of", len(scenarios) * len(qs), "cases")

    # Step 9: Predictions 7 and 8
    print()
    gain = pair[winner][0] - pair[default][0]
    print("Prediction 7: gain over default", round(gain, 3), "(limit 0.10):", "OK" if gain <= 0.10 else "EXCEEDED")
    print("Prediction 7: winner mixed balanced accuracy", round(stats[("mixed", winner[0], winner[1])][2], 3), "(must stay below 0.85):", "OK" if stats[("mixed", winner[0], winner[1])][2] < 0.85 else "EXCEEDED")
    fast_gps = [data[("fast", winner[0], winner[1], sd)][4] for sd in seeds]
    print("Prediction 8: winner fast goodput mean", round(sum(fast_gps) / len(fast_gps), 2), "Mbps, minimum", round(min(fast_gps), 2), "(floor 5):", "OK" if min(fast_gps) > 5 else "BELOW FLOOR")

    # Step 10: write the frozen thresholds file
    with open("docs/frozen_thresholds.md", "w") as f:
        f.write("# Frozen LossLens thresholds\n\n")
        f.write("Frozen on " + str(datetime.date.today()) + " from the tuning grid (seeds " + str(seeds[0]) + " to " + str(seeds[-1]) + ", 1 MB file).\n\n")
        f.write("- q_frac = " + str(winner[0]) + "\n")
        f.write("- slope_ms = " + str(winner[1]) + "\n\n")
        f.write("Selection rule (written in docs/hypotheses.md before the run): balanced accuracy per scenario, mean of the two scenarios, pairs within 0.02 of the best ranked by mean goodput.\n\n")
        f.write("Score " + str(round(pair[winner][0], 3)) + " against " + str(round(pair[default][0], 3)) + " for the default pair (0.1, 1).\n\n")
        f.write("Use on the command line: file_sender <file> <addr> losslens " + str(winner[0]) + " " + str(winner[1]) + "\n\n")
        f.write("No further threshold change is allowed. Test seeds 1 to 10 are used from now on.\n")
    print()
    print("Saved docs/frozen_thresholds.md")


main()