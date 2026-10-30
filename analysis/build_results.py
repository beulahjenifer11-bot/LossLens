import csv
import json
import math
import os
import random


def main():
    # Step 1: settings
    random.seed(0)
    n_boot = 2000
    mss, rtt, capacity = 1200, 0.040, 20.0
    mean = lambda v: sum(v) / len(v)
    os.makedirs("demo", exist_ok=True)

    # Step 2: read a data file (empty list if it does not exist) and a mean with a 95% bootstrap range over the runs
    def read(name):
        path = "docs/data/" + name
        if not os.path.exists(path):
            print("MISSING (skipped):", path)
            return []
        with open(path) as f:
            return list(csv.DictReader(f))

    def ci(vals):
        ms = sorted(mean([random.choice(vals) for _ in vals]) for _ in range(n_boot))
        return [round(mean(vals), 3), round(ms[int(0.025 * n_boot)], 3), round(ms[int(0.975 * n_boot) - 1], 3)]

    # Step 3: random loss sweep (S2): goodput per controller and loss rate, plus the textbook (Mathis) formula
    s2_rows = read("s2_runs.csv") + read("s2_cubic_runs.csv")
    groups = {}
    for r in s2_rows:
        if r["hash_ok"] == "True" and r["goodput_mbps"] != "":
            groups.setdefault((r["controller"], float(r["loss_pct"])), []).append(float(r["goodput_mbps"]))
    losses = sorted(set(k[1] for k in groups))
    s2 = {"loss": losses, "mathis": []}
    for p in losses:
        s2["mathis"].append(None if p == 0 else round(min(capacity, (mss * 8 / rtt / 1e6) * 1.22 / math.sqrt(p / 100.0)), 3))
    for cc in ("reno", "cubic", "fixed"):
        s2[cc] = {"mean": [], "lo": [], "hi": []}
        for p in losses:
            if (cc, p) in groups:
                m, lo, hi = ci(groups[(cc, p)])
            else:
                m = lo = hi = None
            s2[cc]["mean"].append(m)
            s2[cc]["lo"].append(lo)
            s2[cc]["hi"].append(hi)

    # Step 4: the five scenario files, one block per controller
    files = {"fast1": "final_fast1.csv", "cong1": "final_cong1.csv", "trace1": "final_trace1.csv", "mix1": "final_mix1.csv", "mix1ecn": "final_mix1ecn.csv"}
    scen = {}
    runs_total = len(s2_rows); trace_vals = [float(r["mbps"]) for r in read("trace_wifi.csv")]; trace_vals = [float(r["mbps"]) for r in read("trace_wifi.csv")]
    hash_total = sum(1 for r in s2_rows if r["hash_ok"] == "True")
    for key, fname in files.items():
        rows = read(fname)
        runs_total += len(rows)
        hash_total += sum(1 for r in rows if r["hash_ok"] == "True")
        ok = [r for r in rows if r["hash_ok"] == "True" and r["goodput_mbps"] != "" and r["mean_srtt_ms"] != "" and r["min_rtt_ms"] != ""]
        # the same overflow number in every run means the emulator label data is stale, so the label-based numbers are hidden
        labels_ok = len(set(r["emu_overflow"] for r in rows)) > 1
        ctrl = {}
        for cc in sorted(set(r["controller"] for r in ok)):
            g = [r for r in ok if r["controller"] == cc]
            tp = sum(int(r["tp"]) for r in g)
            fnn = sum(int(r["fn"]) for r in g)
            fp = sum(int(r["fp"]) for r in g)
            tn = sum(int(r["tn"]) for r in g)
            ovf = [float(r["emu_overflow"]) for r in g if r["emu_overflow"] != ""]
            ctrl[cc] = {
                "n": len(g),
                "goodput": ci([float(r["goodput_mbps"]) for r in g]),
                "qdelay": ci([float(r["mean_srtt_ms"]) - float(r["min_rtt_ms"]) for r in g]),
                "overflow": round(mean(ovf), 1) if labels_ok and ovf else None,
                "cong_rec": round(tp / (tp + fnn), 3) if labels_ok and tp + fnn > 0 else None,
                "rand_rec": round(tn / (tn + fp), 3) if labels_ok and tn + fp > 0 else None, "util": (ci([float(r["goodput_mbps"]) / max(0.1, mean([trace_vals[i % len(trace_vals)] for i in range(max(1, int(float(r["seconds"]) * 10)))])) for r in g]) if key == "trace1" and trace_vals else None), "util": (ci([float(r["goodput_mbps"]) / max(0.1, mean([trace_vals[i % len(trace_vals)] for i in range(max(1, int(float(r["seconds"]) * 10)))])) for r in g]) if key == "trace1" and trace_vals else None),
            }
        scen[key] = {"labels_ok": labels_ok, "ctrl": ctrl}

    # Step 5: the capacity trace, then write everything for the page and print a short summary
    trace = [float(r["mbps"]) for r in read("trace_wifi.csv")]
    out = {"s2": s2, "scen": scen, "trace": trace, "runs_total": runs_total, "hash_ok": hash_total}
    with open("demo/results.json", "w") as f:
        json.dump(out, f)
    print("Runs counted:", runs_total, "| files identical:", hash_total, "| trace intervals:", len(trace))
    for key, v in scen.items():
        print(key, "| labels ok:", v["labels_ok"], "| mean goodput Mbps:", {cc: round(c["goodput"][0], 2) for cc, c in v["ctrl"].items()})
    print("Saved demo/results.json")


main()
