import random

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt


def main():
    # Step 1: settings. One value per 100 ms interval, 600 intervals = 60 s, the emulator replays it in a loop
    random.seed(7)
    n = 600
    states = {"good": 12.0, "medium": 6.0, "poor": 2.5}   # typical capacity in Mbps of each WiFi condition
    stay = 0.97                                            # chance of keeping the same condition each interval (about 3 s per condition)

    # Step 2: walk through the intervals: pick a condition, add noise, sometimes add a short deep fade
    cond = "good"
    fade_left = 0
    mbps = []
    for i in range(n):
        if random.random() > stay:
            cond = random.choice([c for c in states if c != cond])   # switch to a different condition
        rate = states[cond] * random.uniform(0.85, 1.15)             # noise around the condition's capacity
        if fade_left == 0 and random.random() < 0.01:
            fade_left = random.randint(3, 8)                         # a deep fade lasts 0.3 to 0.8 s
        if fade_left > 0:
            rate = 0.8
            fade_left -= 1
        mbps.append(round(max(0.5, rate), 2))

    # Step 3: save the trace (one number per line under a header) and print a summary
    with open("results/trace_wifi.csv", "w") as f:
        f.write("mbps\n")
        for r in mbps:
            f.write(str(r) + "\n")
    mean = sum(mbps) / len(mbps)
    print("Intervals:", len(mbps), "| mean capacity:", round(mean, 2), "Mbps | min", min(mbps), "| max", max(mbps))

    # Step 4: draw it for the report
    fig, ax = plt.subplots(figsize=(10, 3.2))
    ax.plot([i * 0.1 for i in range(n)], mbps, color="tab:blue", linewidth=1)
    ax.axhline(mean, color="gray", linestyle="--", label="mean " + str(round(mean, 1)) + " Mbps")
    ax.set_xlabel("time (s)")
    ax.set_ylabel("link capacity (Mbps)")
    ax.set_title("Synthetic WiFi-like capacity trace (replayed in a loop)")
    ax.legend()
    ax.grid(alpha=0.3)
    fig.tight_layout()
    fig.savefig("docs/figures/trace_wifi.png", dpi=150)
    print("Saved results/trace_wifi.csv and docs/figures/trace_wifi.png")


main()