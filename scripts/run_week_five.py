#!/usr/bin/env python3
"""Validate Week 5, execute a matched comparative pilot, replay manifests and report adverse results."""
import argparse
import csv
import hashlib
import json
import math
import re
import subprocess
import sys
import time
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FAULT_TESTS = {
    "Delay preserves original evidence metadata": "network_delay_preserves_source_time_version_and_expiry",
    "Late observations cannot become fresh": "late_expired_observation_is_not_rejuvenated_at_receipt",
    "Loss fails closed without fabricated observations": "complete_loss_leaves_evidence_unknown_and_actions_unstarted",
    "Duplicates are idempotent": "duplicate_packets_do_not_repeat_physical_effects_or_overwrite_results",
    "Reordering preserves monotonic observations": "reorder_rejects_older_versions_without_crashing",
    "Partition retains local obligations": "partition_drops_cloud_authority_but_local_fan_finishes",
    "Expired authority is rejected": "stale_leases_crossing_network_never_enqueue_actions",
    "TOCTOU evidence refresh is rejected": "selected_evidence_refresh_between_request_and_dispatch_rejects_lease",
    "Old plan authority is rejected": "old_plan_authority_in_flight_cannot_start_cancelled_instance",
    "Reaction deadline violations are reported": "network_controller_delay_reports_a_missed_reaction_deadline",
    "Cancellation must reach the device": "runtime_cancellation_does_not_teleport_to_the_actuator",
    "Reference and incremental modes agree under faults": "all_network_profiles_are_equivalent_in_full_and_incremental_modes",
    "Exogenous fault schedules match across baselines": "baseline_exogenous_sensor_fault_schedules_match",
    "Manifest replay is deterministic": "compound_session_and_manifest_reproduce_semantic_history",
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=ROOT / "target/week-five-pilot")
    parser.add_argument("--seeds", default="42,73")
    args = parser.parse_args()
    seeds = [int(v) for v in args.seeds.split(",")]
    if not seeds or any(v < 0 or v >= 2**64 for v in seeds) or len(set(seeds)) != len(seeds):
        parser.error("seeds must be distinct unsigned 64-bit integers")
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)

    def command(argv, log, stdin=None):
        result = subprocess.run(argv, cwd=ROOT, input=stdin, text=True, capture_output=True)
        (out / log).write_text(result.stdout + result.stderr)
        if result.returncode:
            raise RuntimeError(f"Command failed; inspect {out / log}")
        return result.stdout

    command([sys.executable, "scripts/verify_week_four.py", "--out", str(out / "verification")], "verification.log")
    gate = json.loads((out / "verification/gate-report.json").read_text())
    if not gate["passed"]:
        raise RuntimeError("Section 75 prerequisite failed")
    statuses = dict(re.findall(r"^test (\S+) \.\.\. (ok|FAILED|ignored)$", (out / "verification/tests.log").read_text(), re.M))
    faults = [{"requirement": title, "test": name, "passed": statuses.get(name) == "ok"} for title, name in FAULT_TESTS.items()]
    (out / "fault-validation.json").write_text(json.dumps({"section": 76, "passed": all(r["passed"] for r in faults), "rows": faults}, indent=2) + "\n")
    if not all(r["passed"] for r in faults):
        raise RuntimeError("Section 76 fault tests incomplete")
    print(f"Validation passed: {gate['passed_tests']} tests; {len(faults)} communication checks.", flush=True)
    command(["cargo", "build", "--release", "--offline", "--bins"], "release-build.log")
    binary = str(ROOT / "target/release/sim")
    (out / "runs").mkdir()
    rows = []
    signatures = {}
    keys = ["UnsafeActionReleaseRate", "FalseSafeAuthorizationRate", "UsefulActionRetention", "UnnecessaryRevocationRate", "WitnessSwitchCount", "LeaseRenewalCount", "ReplanCount", "FallbackCount", "DependencyPropagationLatency", "ExecutionRevalidationLatency", "LeaseIssuanceLatency", "SafetyReactionLatency", "ReactionDeadlineMisses", "TaskCompletionRate", "ActionCompletionRate"]

    def run_case(scenario, profile, baseline, seed, ablation="none", pre_fault=False):
        label = f"{scenario}-{profile}-{baseline}-{seed}-{ablation}" + ("-initial-loss" if pre_fault else "")
        directory = out / "runs" / label
        argv = [binary, "--scenario", scenario, "--network", profile, "--baseline", baseline, "--seed", str(seed), "--ablation", ablation, "--out", str(directory)]
        if pre_fault:
            argv += ["--interactive"]
        command(argv, f"runs/{label}.log", "fault s1 drop\nrun 10s\nquit\n" if pre_fault else None)
        manifest = json.loads((directory / "manifest.json").read_text())
        metrics = json.loads((directory / "metrics.json").read_text())
        if any(key not in metrics for key in keys):
            raise RuntimeError(f"Missing metrics in {label}")
        with (directory / "workload.csv").open() as f:
            work = list(csv.DictReader(f))
        if not work or any(not all(k in w for k in ["N", "M", "M_e", "nodes_recomputed", "justifications_recomputed", "propagation_ns"]) for w in work):
            raise RuntimeError(f"Missing workload measures in {label}")
        with (directory / "network.csv").open() as f:
            # Only exogenous sensor streams: actuator-caused observations must differ in closed loop.
            trace = [r for r in csv.DictReader(f) if any(r["key"].startswith(f"sensor:{id}:") for id in ["s1", "s2", "thermal", "occupancy_clear", "egress_clear", "pressure_safe", "operator_approved", "agent_available"])]
        signature = hashlib.sha256(json.dumps(trace, sort_keys=True).encode()).hexdigest()
        group = (scenario, profile, seed, pre_fault)
        if ablation == "none":
            if group in signatures and signatures[group] != signature:
                raise RuntimeError(f"Mismatched exogenous fault schedule: {label}")
            signatures[group] = signature
            # Plan definitions are deterministic by scenario; execution histories may diverge.
        row = {"run": label, "scenario": scenario, "network": profile, "baseline": baseline, "seed": seed, "ablation": ablation, "initial_sensor_loss": pre_fault, "exogenous_trace_sha256": signature}
        for key in keys:
            value = metrics[key]
            if isinstance(value, dict):
                for k, v in value.items():
                    row[f"{key}.{k}"] = v
            elif not isinstance(value, list):
                row[key] = value
        row["propagation_p95_ns"] = percentile([int(w["propagation_ns"]) for w in work], .95)
        row["reaction_totals_ns"] = json.dumps([r["total_ns"] for r in metrics["SafetyReactionLatency"]])
        rows.append(row)
        return directory

    # 180 matched runs with default seeds; none, delay and compound retain every baseline.
    for scenario in ["S1", "S2", "S3", "S4", "S5"]:
        for profile in ["none", "delay", "compound"]:
            for seed in seeds:
                for baseline in [f"B{i}" for i in range(6)]:
                    run_case(scenario, profile, baseline, seed)
        print(f"Matched comparisons complete: {scenario}", flush=True)
    for seed in seeds:
        for profile in ["loss", "duplicate", "reorder", "partition", "stale-lease"]:
            run_case("S1", profile, "B5", seed)
        for scenario, profile in [("C13", "none"), ("C14", "none"), ("C13", "slow-controller")]:
            run_case(scenario, profile, "B5", seed)
        for scenario, removal in [("S2", "no-alternatives"), ("C13", "no-run"), ("S3", "no-commit"), ("C11", "no-physical")]:
            run_case(scenario, "none", "B5", seed, removal)
        # The late-loss case above is intentionally allowed to be neutral.
        run_case("S2", "none", "B5", seed, "no-alternatives", True)
        run_case("S2", "none", "B5", seed, "none", True)
        run_case("C11", "none", "B5", seed)
    fields = list(dict.fromkeys(k for row in rows for k in row))
    with (out / "pilot.csv").open("w", newline="") as f:
        writer = csv.DictWriter(f, fields)
        writer.writeheader()
        writer.writerows(rows)

    replays = []
    (out / "replays").mkdir()
    for name in [f"S2-none-B5-{seeds[0]}-none", f"S3-none-B5-{seeds[0]}-none", f"S5-compound-B5-{seeds[0]}-none", f"C13-slow-controller-B5-{seeds[0]}-none"]:
        original = out / "runs" / name
        destination = out / "replays" / name
        command([binary, "--manifest", str(original / "manifest.json"), "--out", str(destination)], f"replays/{name}.log")
        equal = (original / "semantic.txt").read_bytes() == (destination / "semantic.txt").read_bytes()
        replays.append({"run": name, "identical": equal})
        if not equal:
            raise RuntimeError(f"Manifest replay differs: {name}")
    command([str(ROOT / "target/release/scale"), "--out", str(out / "scale"), "--sizes", "8,64,256", "--updates", "100"], "scale.log")
    with (out / "scale/workload.csv").open() as f:
        scale = list(csv.DictReader(f))
    grouped = defaultdict(list)
    for row in scale:
        grouped[(int(row["groups"]), row["mode"])].append(int(row["propagation_ns"]))
    scale_summary = [{"groups": g, "mode": mode, "samples": len(values), "p50_ns": percentile(values, .5), "p95_ns": percentile(values, .95), "p99_ns": percentile(values, .99), "max_ns": max(values)} for (g, mode), values in sorted(grouped.items())]
    local_max = max((r.get("DependencyPropagationLatency.max_ns") or 0) + (r.get("ExecutionRevalidationLatency.max_ns") or 0) + (r.get("LeaseIssuanceLatency.max_ns") or 0) for r in rows)
    scale_max = max(r["max_ns"] for r in scale_summary)
    feasible = max(local_max, scale_max) < 50_000_000  # declared pilot criterion: <10% of 500ms margin
    matched = [r for r in rows if r["ablation"] == "none" and not r["initial_sensor_loss"] and r["scenario"].startswith("S") and r["network"] in ["none", "delay", "compound"]]
    # Compare the same scenario/network/seed. Report wins, ties AND losses.
    lookup = {(r["scenario"], r["network"], r["seed"], r["baseline"]): r for r in matched}
    comparisons = []
    for r in matched:
        if r["baseline"] != "B5":
            continue
        for b in ["B0", "B1", "B2", "B3", "B4"]:
            other = lookup[(r["scenario"], r["network"], r["seed"], b)]
            safety_delta = (r["FalseSafeAuthorizationRate.value"] or 0) - (other["FalseSafeAuthorizationRate.value"] or 0)
            completion_delta = r["ActionCompletionRate.value"] - other["ActionCompletionRate.value"]
            comparisons.append({"scenario": r["scenario"], "network": r["network"], "seed": r["seed"], "against": b, "false_safe_rate_delta": safety_delta, "completion_delta": completion_delta, "b5_completed_actions": r["ActionCompletionRate.numerator"]})
    # Absence of releases is reported through counts; it is not evidence of useful success.
    distinction = any(c["b5_completed_actions"] > 0 and (c["false_safe_rate_delta"] < 0 or (c["false_safe_rate_delta"] <= 0 and c["completion_delta"] > 0)) for c in comparisons)
    neutral = [c for c in comparisons if c["false_safe_rate_delta"] == 0 and c["completion_delta"] == 0]
    adverse = [c for c in comparisons if c["false_safe_rate_delta"] > 0 or c["completion_delta"] < 0]
    source = hashlib.sha256()
    for path in sorted((ROOT / "src").rglob("*.rs")) + [Path(__file__).resolve(), ROOT / "Cargo.toml", ROOT / "Cargo.lock"]:
        source.update(str(path.relative_to(ROOT)).encode())
        source.update(path.read_bytes())
    report = {"scope": "bounded software pilot, not universal superiority or hard real-time certification", "section_75_passed": gate["passed"], "section_76_passed": True, "passed_tests": gate["passed_tests"], "runs": len(rows), "matched_runs": len(matched), "exogenous_schedules_matched": True, "manifest_replays": replays, "scale": scale_summary, "acceptance_83": {"semantic_correctness_tested": True, "behavioral_distinction_observed": distinction, "host_feasibility_within_pilot_threshold": feasible, "threshold_ns": 50_000_000, "largest_sum_of_local_operation_maxima_ns": local_max, "largest_scale_evaluation_ns": scale_max}, "neutral_comparisons": neutral, "adverse_comparisons": adverse, "comparisons": comparisons, "source_sha256": source.hexdigest(), "created_unix_seconds": int(time.time()), "seed_count": len(seeds)}
    (out / "pilot-report.json").write_text(json.dumps(report, indent=2) + "\n")
    (out / "pilot-report.md").write_text(markdown_report(report, matched))
    print(json.dumps({"runs": len(rows), "matched_runs": len(matched), "semantic_tests_passed": gate["passed_tests"], "neutral_comparisons": len(neutral), "adverse_comparisons": len(adverse), "acceptance_83": report["acceptance_83"], "report": str(out / "pilot-report.md")}, indent=2))
    return 0


def percentile(values, p):
    return sorted(values)[max(0, math.ceil(len(values) * p) - 1)]


def markdown_report(report, matched):
    lines = ["# Week 5 comparative pilot", "", f"{report['runs']} runs; {report['matched_runs']} matched baseline runs; {report['passed_tests']} tests passed. All four saved-manifest headline replays matched exactly.", "", "This is a small deterministic software pilot. No significance test or population-level superiority is claimed.", "", "| Network | Policy | False-safe releases / releases | Completed tasks / runs | Completed actions / actions |", "| --- | --- | --- | --- | --- |"]
    for profile in ["none", "delay", "compound"]:
        for baseline in [f"B{i}" for i in range(6)]:
            rows = [r for r in matched if r["network"] == profile and r["baseline"] == baseline]
            counts = []
            for metric in ["FalseSafeAuthorizationRate", "TaskCompletionRate", "ActionCompletionRate"]:
                counts.append(f"{sum(r[metric+'.numerator'] for r in rows)}/{sum(r[metric+'.denominator'] for r in rows)}")
            lines.append(f"| {profile} | {baseline} | " + " | ".join(counts) + " |")
    lines += ["", "## Neutral and adverse results", "", f"There are {len(report['neutral_comparisons'])} matched ties and {len(report['adverse_comparisons'])} comparisons where B5 has higher false-safe release rate or lower completion. The JSON retains every comparison, including unfavorable ones.", "", "Interpretation: nominal cases can tie; an expired lease can reduce availability while correctly blocking stale authority; awaiting acknowledgement can reduce completion when the physical effect already occurred. Completion alone cannot establish safety. Rates with no releases are null in raw metrics and must be read alongside their counts.", "", "The no-alternatives ablation is intentionally tested both before start and after start. A sensor loss after fan start can be neutral because its run contract does not require the start-only hazard. Removing run monitoring can increase completion by ignoring missing support; that is not evidence of a better assurance method.", "", "## Host processing and virtual deadlines", "", "| Graph groups | Evaluator | p50 µs | p95 µs | p99 µs | max µs |", "| --- | --- | --- | --- | --- | --- |"]
    for r in report["scale"]:
        lines.append(f"| {r['groups']} | {r['mode']} | {r['p50_ns']/1000:.2f} | {r['p95_ns']/1000:.2f} | {r['p99_ns']/1000:.2f} | {r['max_ns']/1000:.2f} |")
    a = report["acceptance_83"]
    lines += ["", f"The declared pilot host-cost threshold is 50ms, one tenth of the illustrative 500ms reaction margin. The largest sum of separate local-operation maxima was {a['largest_sum_of_local_operation_maxima_ns']/1e6:.3f}ms; the largest scale evaluation was {a['largest_scale_evaluation_ns']/1e6:.3f}ms. Threshold satisfied: **{a['host_feasibility_within_pilot_threshold']}**.", "", "These are measured host elapsed times in a release build. Initialization, logging and instrumentation traversal are excluded from evaluator timing; this is not an end-to-end deployment latency guarantee. Simulated communication delay is separate and can still miss a reaction deadline. The delayed-controller case explicitly demonstrates that runtime cancellation does not instantly stop an in-flight actuator command.", "", "## Acceptance decision", "", f"Semantic equivalence tested: **{a['semantic_correctness_tested']}**. Concrete behavioral distinction observed: **{a['behavioral_distinction_observed']}**. Host feasibility within the declared pilot threshold: **{a['host_feasibility_within_pilot_threshold']}**.", "", "These statements apply to these fixtures, seeds, machine and implementation. Closed-loop actuator observations naturally differ when policies take different actions; initial plans, exogenous observations, random fault draws and fault parameters are matched. The trusted runtime is unchanged by transport profiles; feature removals are explicitly labelled experimental variants.", ""]
    return "\n".join(lines)


if __name__ == "__main__":
    raise SystemExit(main())
