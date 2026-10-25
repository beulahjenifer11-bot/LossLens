import csv
import os
import random

import matplotlib
matplotlib.use("Agg")  # draw to a file, no window needed
import matplotlib.pyplot as plt


def main():
    # Step 1: settings
    random.seed(0)    # makes every interval and p-value repeatable
    n_boot = 10000    # bootstrap resamples
    n_perm = 5000     # permutation shuffles
    controllers = ["reno", "cubic", "fixed", "losslens", "llcut09"]
    colours = {"reno": "tab:red", "cubic": "tab:green", "fixed": "tab:blue", "losslens": "tab:purple", "llcut09": "tab:gray"}
    files = {"fast1": "results/final_fast1.csv", "cong1": "results/final_cong1.csv", "trace1": "results/final_trace1.csv"}
    metrics = [("goodput", "goodput (Mbps)"), ("qdelay", "queueing delay (ms)"), ("overflow", "overflow drops per run")]
    mean = lambda v: sum(v) / len(v)
    os.makedirs("docs/figures", exist_ok=True)

    # Step 2: read both tables, runs[(scenario, controller)][seed] = that run's numbers
    runs = {}
    for sc, fn in files.items():
        with open(fn) as f:
            for r in csv.DictReader(f):
                if r["hash_ok"] != "True" or r["goodput_mbps"] == "" or r["mean_srtt_ms"] == "" or r["min_rtt_ms"] == "" or r["emu_overflow"] == "":
                    print("SKIPPED run:", sc, r["controller"], "seed", r["seed"])
                    continue
                runs.setdefault((sc, r["controller"]), {})[int(r["seed"])] = {
                    "goodput": float(r["goodput_mbps"]),
                    "qdelay": float(r["mean_srtt_ms"]) - float(r["min_rtt_ms"]),
                    "overflow": float(r["emu_overflow"]),
                }
    print("Runs read:", sum(len(v) for v in runs.values()), "(expected 100)")

    # Step 3: mean and 95% bootstrap CI over seeds, one block per scenario and metric
    stats = {}
    for sc in files:
        for metric, label in metrics:
            print()
            print(sc, "|", label, "| mean [95% CI]")
            for cc in controllers:
                d = runs.get((sc, cc), {})
                if not d:
                    continue
                vals = [d[s][metric] for s in sorted(d)]
                ms = sorted(mean([random.choice(vals) for _ in vals]) for _ in range(n_boot))  # resample the seeds with replacement
                lo = ms[int(0.025 * n_boot)]
                hi = ms[int(0.975 * n_boot) - 1]
                stats[(sc, cc, metric)] = (mean(vals), lo, hi)
                print(f"  {cc:<9} {mean(vals):>8.2f}  [{lo:>7.2f}, {hi:>7.2f}]  n={len(vals)}")

    # Step 4: paired comparisons on the same seeds (scenario, metric, A, B, label)
    comps = [
        ("fast1", "goodput", "losslens", "cubic", "H3 goodput clause, needs ratio of at least 1.5"),
        ("fast1", "goodput", "llcut09", "losslens", "control against LossLens, goodput"),
        ("cong1", "qdelay", "losslens", "cubic", "H3 delay clause, needs ratio of at most 1.10"),
        ("cong1", "qdelay", "losslens", "reno", "LossLens against Reno, queueing delay"),
        ("cong1", "qdelay", "llcut09", "losslens", "control against LossLens, queueing delay"),
        ("cong1", "overflow", "llcut09", "losslens", "control against LossLens, overflow drops"),
    ]
    print()
    print("Paired comparisons on the same seeds, ratio = mean of A / mean of B")
    for sc, metric, a, b, label in comps:
        da, db = runs.get((sc, a), {}), runs.get((sc, b), {})
        seeds = sorted(set(da) & set(db))
        if not seeds:
            continue
        va = [da[s][metric] for s in seeds]
        vb = [db[s][metric] for s in seeds]
        n = len(seeds)
        ratio = mean(va) / mean(vb)

        # Step 4a: bootstrap the ratio, the SAME resampled seeds are used for A and B
        rs = []
        for _ in range(n_boot):
            idx = [random.randrange(n) for _ in range(n)]
            rs.append(mean([va[i] for i in idx]) / max(1e-9, mean([vb[i] for i in idx])))
        rs.sort()
        lo, hi = rs[int(0.025 * n_boot)], rs[int(0.975 * n_boot) - 1]

        # Step 4b: Mann-Whitney U (pairs where A beats B, ties count half), effect size P(A > B), permutation p-value
        half = n * n / 2
        u = sum(1.0 if x > y else 0.5 if x == y else 0.0 for x in va for y in vb)
        pool = va + vb
        extreme = 0
        for _ in range(n_perm):
            random.shuffle(pool)
            up = sum(1.0 if x > y else 0.5 if x == y else 0.0 for x in pool[:n] for y in pool[n:])
            if abs(up - half) >= abs(u - half):
                extreme += 1
        print()
        print(label)
        print(f"  {a} / {b}: ratio {ratio:.3f}, 95% CI [{lo:.3f}, {hi:.3f}], n={n} seeds")
        print(f"  Mann-Whitney: P({a} > {b}) = {u / (n * n):.2f}, permutation p = {(extreme + 1) / (n_perm + 1):.4f}")
        if "H3 goodput" in label:
            print("  verdict:", "MET (whole CI above 1.5)" if lo >= 1.5 else "not shown by the CI")
        if "H3 delay" in label:
            if hi <= 1.10:
                print("  verdict: MET (whole CI at or below 1.10)")
            elif ratio <= 1.10:
                print("  verdict: point estimate within 10 percent, but the CI reaches above 1.10")
            else:
                print("  verdict: NOT MET")

    # Step 5: three panels, bars are means with 95% CI whiskers
    fig, axes = plt.subplots(1, 3, figsize=(13, 4.3))
    panels = [
        ("fast1", "goodput", "goodput (Mbps)", "fast1: 20 Mbps, 1% random loss"),
        ("cong1", "qdelay", "queueing delay (ms)", "cong1: 2 Mbps, no random loss"),
        ("cong1", "overflow", "overflow drops per run", "cong1: overflow drops (log scale)"),
    ]
    for ax, (sc, metric, ylab, title) in zip(axes, panels):
        xs = list(range(len(controllers)))
        ms_ = [stats[(sc, cc, metric)][0] for cc in controllers]
        below = [stats[(sc, cc, metric)][0] - stats[(sc, cc, metric)][1] for cc in controllers]
        above = [stats[(sc, cc, metric)][2] - stats[(sc, cc, metric)][0] for cc in controllers]
        ax.bar(xs, ms_, yerr=[below, above], capsize=4, color=[colours[cc] for cc in controllers])
        ax.set_xticks(xs)
        ax.set_xticklabels(controllers, rotation=30)
        ax.set_ylabel(ylab)
        ax.set_title(title, fontsize=10)
        ax.grid(axis="y", alpha=0.3)
        if metric == "overflow":
            ax.set_yscale("log")
    fig.tight_layout()
    fig.savefig("docs/figures/final_summary.png", dpi=150)
    print()
    print("Saved docs/figures/final_summary.png")


main()
