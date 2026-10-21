import csv
import math
import os
import random

import matplotlib
matplotlib.use("Agg")  # draw to a file, no window needed
import matplotlib.pyplot as plt


def main():
    # Step 1: settings (must match experiments/run_s2.ps1)
    mss = 1200        # payload bytes per packet
    rtt = 0.040       # seconds, 2 x 20 ms one-way delay
    capacity = 20.0   # Mbps
    n_boot = 10000    # bootstrap resamples
    random.seed(0)    # makes the confidence intervals repeatable
    os.makedirs("docs/figures", exist_ok=True)

    # Step 2: read the runs table and count problems
    with open("results/mvp_s2_runs.csv") as f:
        rows = list(csv.DictReader(f))
    bad_hash = [r for r in rows if r["hash_ok"] != "True"]
    good = [r for r in rows if r["goodput_mbps"] != "" and r["emu_random"] != ""]
    print("Rows read:", len(rows), "| hash failures:", len(bad_hash), "| usable runs:", len(good))

    # Step 3: group the usable runs by (controller, loss percent)
    groups = {}
    for r in good:
        groups.setdefault((r["controller"], float(r["loss_pct"])), []).append(r)
    losses = sorted(set(k[1] for k in groups))

    # Step 4: mean, bootstrap CI and extra columns for every group, then print the table
    stats = {}
    print()
    print("ctrl   loss%   n    mean Mbps   95% CI            util%  retx%  spurious  cuts")
    for cc in ("reno", "fixed"):
        for p in losses:
            if (cc, p) not in groups:
                continue
            g = groups[(cc, p)]
            vals = [float(r["goodput_mbps"]) for r in g]
            n = len(vals)
            mean = sum(vals) / n
            boots = []
            for _ in range(n_boot):
                resample = [random.choice(vals) for _ in range(n)]  # n draws with replacement
                boots.append(sum(resample) / n)
            boots.sort()
            lo = boots[int(0.025 * n_boot)]
            hi = boots[int(0.975 * n_boot) - 1]
            retx_pct = 100.0 * sum(int(r["retx"]) for r in g) / sum(int(r["sent"]) for r in g)
            spur = sum(int(r["spurious"]) for r in g) / n
            cuts = sum(int(r["reductions"]) for r in g) / n
            stats[(cc, p)] = (mean, lo, hi, n, spur)
            print(f"{cc:<6} {p:>5g} {n:>3} {mean:>10.2f}   [{lo:>6.2f}, {hi:>6.2f}]  {100 * mean / capacity:>6.1f} {retx_pct:>6.2f} {spur:>9.2f} {cuts:>6.1f}")

    # Step 5: Mathis estimate for Reno at each nonzero loss rate, capped at the link capacity
    mathis = {}
    for p in losses:
        if p > 0:
            mathis[p] = min(capacity, (mss * 8 / rtt / 1e6) * 1.22 / math.sqrt(p / 100.0))

    # Step 6: verdicts on the written predictions
    print()
    print("Prediction checks")
    if ("reno", 1.0) in stats:
        m = stats[("reno", 1.0)][0]
        verdict = "SUPPORTED" if m < 0.25 * capacity else "NOT SUPPORTED"
        print("H1: Reno at 1% loss =", round(m, 2), "Mbps =", round(100 * m / capacity, 1), "% of capacity ->", verdict)
    for p in losses:
        if p > 0 and ("reno", p) in stats:
            ratio = stats[("reno", p)][0] / mathis[p]
            where = "within factor 2" if 0.5 <= ratio <= 2 else "OUTSIDE factor 2"
            print("Reno / Mathis at", p, "% loss:", round(ratio, 2), "(Mathis", round(mathis[p], 2), "Mbps)", where)
    for p in losses:
        if ("reno", p) in stats and ("fixed", p) in stats:
            r_m = stats[("reno", p)][0]
            f_m = stats[("fixed", p)][0]
            print("Winner at", p, "% loss:", "reno" if r_m > f_m else "fixed", "(reno", round(r_m, 2), "vs fixed", round(f_m, 2), ")")
    diffs = [int(r["declared"]) - (int(r["emu_random"]) + int(r["emu_overflow"])) for r in good]
    print("Losses declared minus emulator drops, per run: min", min(diffs), "max", max(diffs), "mean", round(sum(diffs) / len(diffs), 2))

    # Step 7: draw goodput against loss rate (x positions are evenly spaced, labels are the loss rates)
    fig, ax = plt.subplots(figsize=(8, 5))
    for cc, colour in (("reno", "tab:red"), ("fixed", "tab:blue")):
        xs, ys, below, above = [], [], [], []
        for i, p in enumerate(losses):
            if (cc, p) in stats:
                m, lo, hi = stats[(cc, p)][:3]
                xs.append(i)
                ys.append(m)
                below.append(max(0.0, m - lo))
                above.append(max(0.0, hi - m))
        ax.errorbar(xs, ys, yerr=[below, above], marker="o", capsize=4, color=colour, label=cc + " (mean, 95% bootstrap CI)")
    mx = [i for i, p in enumerate(losses) if p > 0]
    my = [mathis[p] for p in losses if p > 0]
    ax.plot(mx, my, color="black", linestyle=":", marker="x", label="Mathis estimate for Reno")
    ax.axhline(0.25 * capacity, color="gray", linestyle="--", linewidth=1, label="25% of capacity (H1 limit)")
    ax.set_xticks(range(len(losses)))
    ax.set_xticklabels([str(p) + "%" for p in losses])
    ax.set_xlabel("random loss rate")
    ax.set_ylabel("goodput (Mbps)")
    ax.set_ylim(0, capacity)
    ax.set_title("S2: goodput vs random loss (20 Mbps, 40 ms RTT, 5 MB file)")
    ax.legend(fontsize=8)
    ax.grid(alpha=0.3)
    fig.tight_layout()
    fig.savefig("docs/figures/s2_goodput_vs_loss.png", dpi=150)
    print()
    print("Saved docs/figures/s2_goodput_vs_loss.png")


main()