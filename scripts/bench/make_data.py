#!/usr/bin/env python3
"""Write the benchmark's data files, deterministically (seed 7):
DIR/data/big.txt     100 MiB of plain ASCII lines of 3-18 words
DIR/data/colors.txt  200,000 lines with SGR 30-37, bold and 256-colour runs
Usage: make_data.py DIR"""
import os, random, sys

d = os.path.join(sys.argv[1], "data")
os.makedirs(d, exist_ok=True)
rng = random.Random(7)
vocab = ["".join(rng.choice("abcdefghijklmnopqrstuvwxyz") for _ in range(rng.randint(2, 10))) for _ in range(2000)]

with open(os.path.join(d, "big.txt"), "w", newline="\n") as f:
    size, target = 0, 100 * 1024 * 1024
    while size < target:
        line = " ".join(rng.choices(vocab, k=rng.randint(3, 18))) + "\n"
        f.write(line)
        size += len(line)

with open(os.path.join(d, "colors.txt"), "w", newline="\n") as f:
    for _ in range(200_000):
        parts = []
        for _ in range(rng.randint(2, 5)):
            kind = rng.randint(0, 2)
            if kind == 0:
                sgr = f"\x1b[{rng.randint(30, 37)}m"
            elif kind == 1:
                sgr = f"\x1b[1;{rng.randint(30, 37)}m"
            else:
                sgr = f"\x1b[38;5;{rng.randint(0, 255)}m"
            parts.append(sgr + " ".join(rng.choices(vocab, k=rng.randint(1, 3))) + "\x1b[0m")
        f.write(" ".join(parts) + "\n")
