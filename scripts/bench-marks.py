#!/usr/bin/env python3
"""Subtract engine mark timestamps; shell/log delivery time is never measured."""
import argparse
import json
import re
from pathlib import Path

MARK = re.compile(r"benchmark-mark: pid=(\d+) seq=(\d+) ns=(\d+) label=([A-Za-z0-9_.-]+)(?:\s|$)")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log", type=Path)
    parser.add_argument("labels", nargs="*", default=["in_menu", "in_hanoi", "in_underpass"])
    parser.add_argument("--json", action="store_true", help="emit machine-readable intervals")
    args = parser.parse_args()
    try:
        marks = [tuple(match.groups()) for match in MARK.finditer(args.log.read_text())]
        if not marks:
            raise ValueError("no engine markers found")
        if len({row[0] for row in marks}) != 1:
            raise ValueError("multiple processes in log; provide one run")
        # stdout and the runtime log may both contain the same event.
        unique = list(dict.fromkeys(marks))
        if any(int(b[1]) <= int(a[1]) or int(b[2]) < int(a[2]) for a, b in zip(unique, unique[1:])):
            raise ValueError("markers are not in monotonic sequence order")
        selected = []
        for label in args.labels:
            matches = [row for row in unique if row[3] == label]
            if len(matches) != 1:
                raise ValueError(f"{label}: expected one marker, found {len(matches)}")
            selected.append(matches[0])
        if len(selected) < 2:
            raise ValueError("at least two labels are required")
        intervals = []
        for start, end in zip(selected, selected[1:]):
            if int(end[1]) <= int(start[1]):
                raise ValueError(f"{end[3]} does not follow {start[3]}")
            intervals.append({"from": start[3], "to": end[3], "elapsed_ns": int(end[2]) - int(start[2])})
    except (OSError, ValueError) as error:
        parser.exit(1, f"bench-marks: {error}\n")
    if args.json:
        print(json.dumps({"pid": int(marks[0][0]), "intervals": intervals}, indent=2))
    else:
        for interval in intervals:
            print(f"{interval['from']} -> {interval['to']}: {interval['elapsed_ns'] / 1e9:.6f} s")


if __name__ == "__main__":
    main()
