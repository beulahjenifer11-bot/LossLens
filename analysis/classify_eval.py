import csv
import sys


def main():
    # Step 1: the run names to score come from the command line, e.g. mixed_losslens
    names = sys.argv[1:]
    if not names:
        print("usage: python analysis/classify_eval.py <run name> [<run name> ...]")
        return
    ratio = lambda a, b: a / b if b else float("nan")

    for name in names:
        # Step 2: ground truth from the emulator, packet number -> true cause
        truth = {}
        with open("results/" + name + "_drops.csv") as f:
            for row in csv.DictReader(f):
                truth[int(row["packet_number"])] = row["cause"]

        # Step 3: join every loss the sender declared with its true cause, count (true cause, verdict) pairs
        counts = {}
        with open("results/" + name + "_losses.csv") as f:
            for row in csv.DictReader(f):
                pn = int(row["packet_number"])
                key = (truth.get(pn, "UNLABELLED"), row["verdict"])
                counts[key] = counts.get(key, 0) + 1
        c = lambda t, v: counts.get((t, v), 0)

        # Step 4: confusion matrix, rows are the TRUE cause, columns are the sender's verdict
        print("=" * 62)
        print(name)
        print("true cause         verdict CONGESTION    verdict RANDOM")
        for t in ("QUEUE_OVERFLOW", "RANDOM", "UNLABELLED"):
            print(f"{t:<18} {c(t, 'CONGESTION'):>17} {c(t, 'RANDOM'):>17}")

        # Step 5: metrics. TP = overflow drop called CONGESTION, FP = random drop called CONGESTION,
        # FN = overflow drop called RANDOM, TN = random drop called RANDOM
        tp = c("QUEUE_OVERFLOW", "CONGESTION")
        fn = c("QUEUE_OVERFLOW", "RANDOM")
        fp = c("RANDOM", "CONGESTION")
        tn = c("RANDOM", "RANDOM")
        prec_c, rec_c = ratio(tp, tp + fp), ratio(tp, tp + fn)
        prec_r, rec_r = ratio(tn, tn + fn), ratio(tn, tn + fp)
        f1_c = ratio(2 * prec_c * rec_c, prec_c + rec_c)
        f1_r = ratio(2 * prec_r * rec_r, prec_r + rec_r)
        total = tp + fn + fp + tn
        unlabelled = c("UNLABELLED", "CONGESTION") + c("UNLABELLED", "RANDOM")
        print(f"congestion class: precision {prec_c:.2f}, recall {rec_c:.2f}, F1 {f1_c:.2f}")
        print(f"random class:     precision {prec_r:.2f}, recall {rec_r:.2f}, F1 {f1_r:.2f}")
        print(f"accuracy over labelled drops: {ratio(tp + tn, total):.2f} ({total} labelled, {unlabelled} unlabelled)")


main()