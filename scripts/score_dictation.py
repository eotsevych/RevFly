#!/usr/bin/env python3
"""Scores a dictation benchmark: word error rate, dropped phrases and wait per app and clip.

  python3 scripts/score_dictation.py bench/        # Markdown tables, plus bench/results.csv
  python3 scripts/score_dictation.py --test        # self-check

Folder layout:
  bench/refs/<clip>.txt              hand-checked reference transcript of each clip
  bench/runs/<app>_<clip>_<run>.txt  what an app pasted (app without underscores; run = 1, 2, 3)
  bench/waits.csv                    optional: app,clip,run,wait_ms (stop key to first pasted
                                     character, read from the screen recording)

Before comparing, both texts are lowercased, punctuation is dropped, and number style is evened out
("1" = "one", "1st" = "first", "%" = "percent"), so formatting choices don't count as errors.
"""
import csv
import os
import re
import statistics
import sys
from collections import defaultdict

ONES = "zero one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen".split()
TENS = "_ _ twenty thirty forty fifty sixty seventy eighty ninety".split()
ORDINALS = {"one": "first", "two": "second", "three": "third", "five": "fifth", "eight": "eighth", "nine": "ninth", "twelve": "twelfth"}
SAME = {"alright": ["all", "right"], "ok": ["okay"], "gonna": ["going", "to"], "wanna": ["want", "to"], "percent": ["percent"]}


def number_words(n):
    """1234 → ["one", "thousand", "two", "hundred", "thirty", "four"]."""
    if n < 20:
        return [ONES[n]]
    if n < 100:
        return [TENS[n // 10]] + (number_words(n % 10) if n % 10 else [])
    if n < 1000:
        return number_words(n // 100) + ["hundred"] + (number_words(n % 100) if n % 100 else [])
    for size, name in ((10**9, "billion"), (10**6, "million"), (1000, "thousand")):
        if n >= size:
            return number_words(n // size) + [name] + (number_words(n % size) if n % size else [])


def ordinal(words):
    last = words[-1]
    last = ORDINALS.get(last) or (last[:-1] + "ieth" if last.endswith("y") else last + "th")
    return words[:-1] + [last]


def normalize(text):
    """Words to compare. shortcut: "2024" reads as "two thousand twenty four", not "twenty twenty
    four"; write years in the reference as the apps write them if that matters."""
    text = text.lower().replace("%", " percent").replace("$", " dollars ")
    words = []
    for token in re.findall(r"[a-z0-9]+(?:['’][a-z]+)?", text):
        token = token.replace("’", "'")
        number = re.fullmatch(r"(\d+)(st|nd|rd|th)?", token)
        if number and len(number.group(1)) <= 12:
            spoken = number_words(int(number.group(1)))
            words += ordinal(spoken) if number.group(2) else spoken
        else:
            words += SAME.get(token, [token])
    return words


def align(ref, hyp):
    """Word error rate of `hyp` against `ref`, and the reference words that were left out, as runs."""
    rows, cols = len(ref) + 1, len(hyp) + 1
    cost = [[0] * cols for _ in range(rows)]
    for i in range(rows):
        cost[i][0] = i
    for j in range(cols):
        cost[0][j] = j
    for i in range(1, rows):
        for j in range(1, cols):
            cost[i][j] = min(
                cost[i - 1][j] + 1,  # dropped
                cost[i][j - 1] + 1,  # added
                cost[i - 1][j - 1] + (ref[i - 1] != hyp[j - 1]),  # kept or misheard
            )
    dropped, run = [], []
    i, j = len(ref), len(hyp)
    while i > 0:
        if j > 0 and cost[i][j] == cost[i - 1][j - 1] + (ref[i - 1] != hyp[j - 1]):
            i, j = i - 1, j - 1
            run = flush(run, dropped)
        elif cost[i][j] == cost[i - 1][j] + 1:
            run.insert(0, ref[i - 1])
            i -= 1
        else:
            j -= 1
            run = flush(run, dropped)
    flush(run, dropped)
    wer = cost[-1][-1] / max(len(ref), 1)
    return wer, dropped[::-1]


def flush(run, dropped):
    """Keeps a run of 2+ dropped words as a lost phrase; single words count only in the WER."""
    if len(run) >= 2:
        dropped.append(" ".join(run))
    return []


def read(path):
    with open(path, encoding="utf-8") as f:
        return f.read()


def load_waits(path):
    waits = defaultdict(list)
    if os.path.exists(path):
        with open(path, newline="") as f:
            for row in csv.DictReader(f):
                waits[(row["app"], row["clip"])].append(float(row["wait_ms"]))
    return waits


def score(bench):
    refs = {name[:-4]: normalize(read(os.path.join(bench, "refs", name))) for name in os.listdir(os.path.join(bench, "refs")) if name.endswith(".txt")}
    runs = defaultdict(list)
    for name in sorted(os.listdir(os.path.join(bench, "runs"))):
        parts = name[:-4].split("_")
        if not name.endswith(".txt") or len(parts) < 3:
            continue
        app, clip = parts[0], "_".join(parts[1:-1])
        if clip not in refs:
            print(f"skipped {name}: no refs/{clip}.txt", file=sys.stderr)
            continue
        runs[(app, clip)].append(align(refs[clip], normalize(read(os.path.join(bench, "runs", name)))))
    waits = load_waits(os.path.join(bench, "waits.csv"))
    results = []
    for (app, clip), scored in sorted(runs.items()):
        wait = waits.get((app, clip))
        # Phrases lost in most runs, so one bad run doesn't define the app.
        counts = defaultdict(int)
        for _, dropped in scored:
            for phrase in set(dropped):
                counts[phrase] += 1
        results.append({
            "app": app,
            "clip": clip,
            "runs": len(scored),
            "wer": statistics.median(w for w, _ in scored),
            "wait_ms": statistics.median(wait) if wait else None,
            "dropped": [p for p, n in counts.items() if n * 2 > len(scored)],
        })
    return results


def report(bench, results):
    print("| App | Clip | Runs | WER (median) | Wait (median) | Phrases lost in most runs |")
    print("| --- | --- | --- | --- | --- | --- |")
    for r in results:
        wait = f"{r['wait_ms']:.0f} ms" if r["wait_ms"] is not None else ""
        lost = "; ".join(f'"{p}"' for p in r["dropped"])
        print(f"| {r['app']} | {r['clip']} | {r['runs']} | {r['wer']:.1%} | {wait} | {lost} |")

    print("\n| App | Mean WER over clips | Median wait | Phrases lost |")
    print("| --- | --- | --- | --- |")
    by_app = defaultdict(list)
    for r in results:
        by_app[r["app"]].append(r)
    for app, rows in sorted(by_app.items(), key=lambda kv: statistics.mean(r["wer"] for r in kv[1])):
        waits = [r["wait_ms"] for r in rows if r["wait_ms"] is not None]
        wait = f"{statistics.median(waits):.0f} ms" if waits else ""
        print(f"| {app} | {statistics.mean(r['wer'] for r in rows):.1%} | {wait} | {sum(len(r['dropped']) for r in rows)} |")

    with open(os.path.join(bench, "results.csv"), "w", newline="") as f:
        out = csv.writer(f)
        out.writerow(["app", "clip", "runs", "wer", "wait_ms", "dropped"])
        for r in results:
            out.writerow([r["app"], r["clip"], r["runs"], round(r["wer"], 4), r["wait_ms"], " | ".join(r["dropped"])])


def self_test():
    assert normalize("I'm going to become the 1st... look just like 1! 50%") == normalize("i'm going to become the first look just like one fifty percent")
    assert normalize("Alright, OK") == ["all", "right", "okay"]
    assert align(["a", "b"], ["a", "b"]) == (0.0, [])
    ref = normalize("Me too. I want to become a great Hokage. Oh no, you couldn't be more wrong.")
    hyp = normalize("Me too. Great Hokage. You couldn't be more wrong.")
    wer, dropped = align(ref, hyp)
    assert dropped == ["i want to become a", "oh no"], dropped
    assert abs(wer - 7 / len(ref)) < 1e-9, wer
    print("ok")


if __name__ == "__main__":
    if sys.argv[1:] == ["--test"]:
        self_test()
    elif len(sys.argv) == 2:
        report(sys.argv[1], score(sys.argv[1]))
    else:
        print(__doc__)
        sys.exit(1)
