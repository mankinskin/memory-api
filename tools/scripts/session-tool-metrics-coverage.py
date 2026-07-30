#!/usr/bin/env python3
"""Reproducible coverage measurement for session-api tool output sizes.

Computes the coverage ratio defined in spec 97f25cf8 R5:

    coverage = (tool calls with non-empty output_char_sizes)
               / (total tool calls in the captured session)

and reports a per-source breakdown and residual-gap diagnosis.
"""
import argparse
import json
import sys
from collections import Counter
from pathlib import Path


def load_json(path: Path):
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def coverage_from_metrics(metrics: dict) -> dict:
    tools = metrics.get("tools", {})
    total_calls = sum(t["call_count"] for t in tools.values())
    measured_calls = sum(
        len(t.get("output_char_sizes", [])) for t in tools.values()
    )
    pct = (measured_calls / total_calls * 100) if total_calls else 0.0

    source_counts = Counter()
    for t in tools.values():
        for src in t.get("output_source", []):
            source_counts[src] += 1

    per_tool = {}
    for name, t in sorted(tools.items()):
        calls = t["call_count"]
        measured = len(t.get("output_char_sizes", []))
        per_tool[name] = {
            "calls": calls,
            "measured": measured,
            "unmeasured": calls - measured,
        }

    return {
        "total_calls": total_calls,
        "measured_calls": measured_calls,
        "coverage_percent": round(pct, 2),
        "source_counts": dict(source_counts),
        "per_tool": per_tool,
    }


def diagnose(metrics_path: Path, events_path: Path) -> dict:
    if not events_path.exists():
        return {"error": f"events file not found: {events_path}"}

    events = load_json(events_path).get("events", [])
    terminal = [
        e
        for e in events
        if (e.get("event_type") or "").startswith("tool.execution_complete")
        or (e.get("event_type") or "").startswith("tool_execution_complete")
    ]

    empty_by_tool = Counter()
    cause_counts = Counter()
    for e in terminal:
        data = e.get("data_json", {})
        if data.get("output_chars") is not None:
            continue
        name = e.get("tool_name") or "unknown"
        empty_by_tool[name] += 1

        if e.get("tool_success") is False:
            cause_counts["failed_call_no_output_expected"] += 1
        elif data.get("has_spill"):
            cause_counts["spill_flag_true_but_no_output_chars"] += 1
        else:
            cause_counts["terminal_success_but_no_hook_payload_or_spill"] += 1

    metrics = load_json(metrics_path)
    tools = metrics.get("tools", {})
    reconciled = sum(
        t["call_count"] - len(t.get("output_char_sizes", []))
        for t in tools.values()
    )

    return {
        "terminal_events": len(terminal),
        "empty_terminal_events": sum(empty_by_tool.values()),
        "empty_by_tool": empty_by_tool.most_common(),
        "cause_counts": dict(cause_counts),
        "reconciled_unmeasured_from_metrics": reconciled,
    }


def resolve_metrics_path(session: str) -> Path:
    p = Path(session)
    if p.is_file():
        return p
    if p.is_dir() and (p / "tool-metrics.json").is_file():
        return p / "tool-metrics.json"
    candidate = Path(".session/sessions") / session / "tool-metrics.json"
    if candidate.is_file():
        return candidate
    raise SystemExit(f"cannot locate tool-metrics.json for {session}")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Compute tool output-size coverage for a captured session."
    )
    parser.add_argument(
        "session",
        help="Session UUID, or path to tool-metrics.json / session directory",
    )
    parser.add_argument(
        "--events",
        help="Path to events.json (default: sibling of tool-metrics.json)",
    )
    parser.add_argument(
        "--no-diagnose",
        action="store_true",
        help="Skip residual-gap diagnosis from events.json",
    )
    parser.add_argument("--json", action="store_true", help="Emit JSON output")
    args = parser.parse_args()

    metrics_path = resolve_metrics_path(args.session)
    events_path = (
        Path(args.events) if args.events else metrics_path.with_name("events.json")
    )

    metrics = load_json(metrics_path)
    result = {
        "session_id": metrics.get("session_id"),
        "metrics_path": str(metrics_path),
        "coverage": coverage_from_metrics(metrics),
    }
    if not args.no_diagnose:
        result["diagnosis"] = diagnose(metrics_path, events_path)

    if args.json:
        print(json.dumps(result, indent=2))
        return 0

    c = result["coverage"]
    print(f"session_id: {result['session_id']}")
    print(f"metrics_path: {result['metrics_path']}")
    print(f"total_calls: {c['total_calls']}")
    print(f"measured_calls: {c['measured_calls']}")
    print(f"coverage_percent: {c['coverage_percent']}%")
    print("per_source_counts:")
    for src, cnt in sorted(c["source_counts"].items()):
        print(f"  {src}: {cnt}")
    print("per_tool with unmeasured calls:")
    for name, info in c["per_tool"].items():
        if info["unmeasured"]:
            print(
                f"  {name}: calls={info['calls']} "
                f"measured={info['measured']} unmeasured={info['unmeasured']}"
            )

    d = result.get("diagnosis")
    if d:
        print("diagnosis:")
        if "error" in d:
            print(f"  error: {d['error']}")
        else:
            print(f"  terminal_events: {d['terminal_events']}")
            print(f"  empty_terminal_events: {d['empty_terminal_events']}")
            print(
                f"  reconciled_unmeasured_from_metrics: "
                f"{d['reconciled_unmeasured_from_metrics']}"
            )
            print("  cause_counts:")
            for cause, cnt in sorted(d["cause_counts"].items()):
                print(f"    {cause}: {cnt}")
            print("  empty_by_tool (top):")
            for name, cnt in d["empty_by_tool"][:20]:
                print(f"    {name}: {cnt}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
