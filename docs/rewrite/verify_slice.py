#!/usr/bin/env python3
"""Executable M2/M3 specification over captured implementation evidence.

Usage: python3 docs/rewrite/verify_slice.py EVIDENCE.json
This is a test evidence format, not a public authoring protocol. No evidence file
means failure; constructing expectations is never reported as application acceptance.
See acceptance.md for the required capture actions and native/runtime checks.
"""
import json
import math
from fractions import Fraction
from pathlib import Path
import sys

FIXTURE = Path(__file__).with_name("fixtures") / "animated-shape.json"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def close(actual, expected, path):
    if isinstance(expected, list):
        require(isinstance(actual, list) and len(actual) == len(expected), path)
        for index, (a, e) in enumerate(zip(actual, expected)):
            close(a, e, f"{path}[{index}]")
    else:
        require(type(actual) in (int, float) and math.isfinite(actual), path)
        require(abs(actual - float(expected)) <= 1e-9, f"{path}: {actual} != {expected}")


def verify_native(evidence):
    """Check captured native state only; this does not qualify the M2/M3 gate."""
    fixture = json.loads(FIXTURE.read_text())
    require(evidence["fixture"] == fixture["fixture"], "wrong fixture")
    require(evidence["protocol"] == 2, "old protocol cannot qualify")
    require(evidence["document_type"] == "fold.motion.construction", "old document")
    times = [Fraction(*time) for time in fixture["times"]]
    runs = evidence["seek_runs"]
    require(len(runs) == len(fixture["orders"]), "missing seek orders")
    identity = None
    for order, run in zip(fixture["orders"], runs):
        require(len(run) == len(order), "missing samples")
        for index, sample in zip(order, run):
            time = times[index]
            require(Fraction(*sample["time"]) == time, "wrong sample time")
            progress = min(Fraction(1), max(Fraction(0), time / Fraction(3, 5)))
            opacity = (max(Fraction(0), time * 2) if time <= Fraction(1, 2)
                       else max(Fraction(1, 2), Fraction(3, 2) - time))
            close(sample["position"], [0, 80 * (1 - progress)], "position")
            close(sample["opacity"], opacity, "opacity")
            close(sample["size"], [200, 100], "size")
            require(sample["element_ids"], "missing evaluated identity")
            if identity is None:
                identity = sample["element_ids"]
            require(sample["element_ids"] == identity, "seek changed identity")
    snapshots = evidence["snapshots"]
    baseline = snapshots["authored"]
    for key in ("canonical_reparse", "reopened", "redo"):
        require(snapshots[key] == baseline, f"{key} changed semantic/presentation state")
    require(snapshots["undo"] == snapshots["before_edit"], "undo lost state")
    require(snapshots["invalid_source"] == baseline, "invalid source mutated document")
    require(evidence["invalid_source_buffer"] != evidence["valid_source"], "invalid buffer lost")
    require(evidence["unknown_record_before"] == evidence["unknown_record_reopened"], "unknown operation lost")
    require(evidence["unknown_extensions_before"] == evidence["unknown_extensions_reopened"], "extensions lost")


def verify_errors(evidence):
    """Check rejection captures only; clock/transport provenance remains explicit."""
    required_errors = {
        "same_property": "conflict", "parent_lock_bypass": "locked",
        "override_lock_bypass": "locked", "stale_generation": "conflict",
        "expired_candidate": "expired", "cycle": "cycle",
        "recursive_asset": "cycle", "unknown_operation": "unsupported",
        "old_protocol": "version", "changed_retry": "idempotency_conflict",
    }
    for case, code in required_errors.items():
        require(evidence["errors"][case]["code"] == code, case)
        require(evidence["errors"][case]["before"] == evidence["errors"][case]["after"], case + " mutated state")


def verify(evidence):
    require(evidence["source_revision"] and evidence["build"], "missing provenance")
    verify_native(evidence)
    handoff = evidence["handoff"]
    for property_name in ("ramp", "artist_key", "count_lock", "graph_layout", "asset_overrides"):
        require(handoff["before"][property_name] == handoff["after"][property_name], property_name + " lost")
    require(handoff["before"]["field_direction"] != handoff["after"]["field_direction"], "agent edit absent")
    require(handoff["undo_groups_added"] == 1, "apply must commit one undo group")
    require(handoff["artist_clock_before"] == handoff["artist_clock_after"], "observation moved artist")
    require(handoff["artist_selection_before"] == handoff["artist_selection_after"], "observation changed selection")
    verify_errors(evidence)
    require(evidence["retry_undo_groups_added"] == 0, "retry applied twice")
    require(evidence["cli_schema"] == evidence["mcp_schema"], "transport schemas diverge")
    require(evidence["cli_handoff"] == evidence["mcp_handoff"], "transport semantics diverge")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    try:
        verify(json.loads(Path(sys.argv[1]).read_text()))
    except (KeyError, ValueError, TypeError, OSError) as error:
        raise SystemExit(f"FAIL: {error}")
    print("Captured M2/M3 slice evidence satisfies automated specification; native acceptance is separate.")
