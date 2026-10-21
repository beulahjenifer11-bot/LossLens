import csv
import math
import os

import matplotlib
matplotlib.use("Agg")  # draw to files, no window needed
import matplotlib.pyplot as plt


def main():
    # Step 1: settings. Each run is (file prefix in results/, graph title)
    mss = 1200
    runs = [
        ("testA_reno", "Test A: Reno, 2 Mbps link, 10-packet buffer, no random loss"),
                ("testB_reno", "Test B: Reno, 20 Mbps link, 1% random loss"),
        ("testA_cubic", "Test A: CUBIC, 2 Mbps link, 10-packet buffer, no random loss"),
    ]
    os.makedirs("docs/figures", exist_ok=True)

    for name, title in runs:
        print("=" * 60)
        print(title)

        # Step 2: read the cwnd log (time in seconds, cwnd and ssthresh in packets)
        t, cwnd, ssthresh = [], [], []
        with open("results/" + name + "_cwnd.csv") as f:
            for row in csv.DictReader(f):
                t.append(float(row["time_ms"]) / 1000.0)
                cwnd.append(float(row["cwnd_bytes"]) / mss)
                s = float(row["ssthresh_bytes"])
                ssthresh.append(s / mss if s > 0 else float("nan"))  # -1 means "still infinite", skip it

        # Step 3: read the emulator's ground truth, packet number -> true cause
        truth = {}
        with open("results/" + name + "_drops.csv") as f:
            for row in csv.DictReader(f):
                truth[int(row["packet_number"])] = row["cause"]

        # Step 4: read the sender's declared losses and JOIN each one with its true cause
        losses = []  # (time in seconds, true cause, rule that fired)
        with open("results/" + name + "_losses.csv") as f:
            for row in csv.DictReader(f):
                pn = int(row["packet_number"])
                cause = truth.get(pn, "NOT_DROPPED (spurious)")  # declared lost but the emulator never dropped it
                losses.append((float(row["time_ms"]) / 1000.0, cause, row["rule"]))

        # Step 5: count the join result
        by_cause, by_rule = {}, {}
        for _, cause, rule in losses:
            by_cause[cause] = by_cause.get(cause, 0) + 1
            by_rule[rule] = by_rule.get(rule, 0) + 1
        print("Emulator dropped (ground truth):", len(truth), "packets")
        print("Sender declared", len(losses), "losses. By true cause:", by_cause)
        print("Rules that fired:", by_rule)
        print("cwnd peaked at", round(max(cwnd), 1), "packets, ended at", round(cwnd[-1], 1), "packets")

        # Step 6: draw cwnd over time, ssthresh dashed, one vertical line per declared loss coloured by TRUE cause
        colours = {"RANDOM": "tab:red", "QUEUE_OVERFLOW": "tab:blue", "NOT_DROPPED (spurious)": "gray"}
        fig, ax = plt.subplots(figsize=(11, 4.5))
        ax.plot(t, cwnd, color="black", linewidth=1.3, label="cwnd (packets)")
        ax.plot(t, ssthresh, color="green", linestyle="--", linewidth=1, label="ssthresh (packets)")
        shown = set()
        for lt, cause, _ in losses:
            lab = "loss, true cause " + cause if cause not in shown else None
            ax.axvline(lt, color=colours.get(cause, "gray"), alpha=0.35, linewidth=1, label=lab)
            shown.add(cause)
        ax.set_xlabel("time (s)")
        ax.set_ylabel("packets")
        ax.set_title(title)
        ax.legend(loc="upper right", fontsize=8)
        ax.grid(alpha=0.3)
        fig.tight_layout()
        out = "docs/figures/" + name + "_cwnd.png"
        fig.savefig(out, dpi=150)
        plt.close(fig)
        print("Saved", out)

    # Step 7: Mathis check for Test B. Numbers come from your Step 16 output
    rtt = 0.040             # seconds, the emulator's 40 ms path
    goodput_b = 3.53        # Mbps measured by the sender in Test B
    packets_sent_b = 886    # packets the sender sent in Test B
    drops_b = 11            # packets the emulator dropped in Test B
    p = drops_b / packets_sent_b
    mathis_bps = (mss * 8 / rtt) * (1.22 / math.sqrt(p))
    print("=" * 60)
    print("Mathis estimate for Test B at observed loss rate", round(p * 100, 2), "%:", round(mathis_bps / 1e6, 2), "Mbps")
    print("Measured goodput:", goodput_b, "Mbps, ratio measured / Mathis:", round(goodput_b / (mathis_bps / 1e6), 2))


main()