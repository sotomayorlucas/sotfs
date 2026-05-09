#!/usr/bin/env python3
"""Coverage gate for sotFS.

Reads the line-coverage percentage from the JSON output of
``cargo llvm-cov report --json`` (NOT the human summary, which is
fragile across tool versions), and:

  1. fails if absolute coverage is below ``--threshold-min``;
  2. fails if it dropped more than ``--threshold-drop`` percentage
     points compared to the value stored in ``--baseline``;
  3. on first run, captures the current value as the baseline so
     subsequent runs have a delta to check against.

The baseline file is a single line: a JSON dict mapping ``layer``
to last-seen percentage. We use ``layer`` to keep the door open for
adding more layers later (e.g. fuzz coverage).
"""
from __future__ import annotations

import argparse
import json
import os
import sys


def parse_llvm_cov_json(path: str) -> float:
    """Return overall line coverage % from ``cargo llvm-cov report --json``.

    The schema we depend on (cargo-llvm-cov 0.6+):
      data: [
        {
          "totals": {
            "lines": { "count": int, "covered": int, "percent": float, ... }
          }
        }
      ]

    We compute percent ourselves from count/covered to avoid relying
    on the float field's stability across versions.
    """
    with open(path) as f:
        doc = json.load(f)
    totals = doc["data"][0]["totals"]
    lines = totals["lines"]
    count = int(lines["count"])
    covered = int(lines["covered"])
    if count == 0:
        return 100.0
    return 100.0 * covered / count


def load_baseline(path: str) -> dict[str, float]:
    if not os.path.exists(path):
        return {}
    with open(path) as f:
        return json.load(f)


def write_baseline(path: str, layers: dict[str, float]) -> None:
    with open(path, "w") as f:
        json.dump(layers, f, indent=2, sort_keys=True)
        f.write("\n")


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--cov-json", required=True,
                   help="cargo llvm-cov report --json output")
    p.add_argument("--baseline", required=True,
                   help="checked-in JSON baseline; created on first run")
    p.add_argument("--layer", default="workspace",
                   help="layer name in the baseline (default: workspace)")
    p.add_argument("--threshold-min", type=float, required=True,
                   help="absolute floor; fail if line% < this")
    p.add_argument("--threshold-drop", type=float, required=True,
                   help="max allowed drop in pp vs baseline")
    args = p.parse_args()

    pct = parse_llvm_cov_json(args.cov_json)
    print(f"[coverage] {args.layer} line coverage = {pct:.2f}%")

    baseline = load_baseline(args.baseline)
    fail = False

    if pct < args.threshold_min:
        print(f"FAIL: {args.layer} line coverage {pct:.2f}% "
              f"below floor {args.threshold_min}%")
        fail = True

    if args.layer in baseline:
        prev = baseline[args.layer]
        drop = prev - pct
        print(f"[coverage] baseline = {prev:.2f}% "
              f"(delta = {-drop:+.2f}pp, allowed drop = {args.threshold_drop}pp)")
        if drop > args.threshold_drop:
            print(f"FAIL: {args.layer} dropped {drop:.2f}pp; "
                  f"only {args.threshold_drop}pp allowed")
            fail = True
    else:
        print(f"[coverage] no baseline for {args.layer}; "
              "capturing current value")
        baseline[args.layer] = pct
        write_baseline(args.baseline, baseline)
        print(f"[coverage] wrote {args.baseline}")

    return 1 if fail else 0


if __name__ == "__main__":
    sys.exit(main())
