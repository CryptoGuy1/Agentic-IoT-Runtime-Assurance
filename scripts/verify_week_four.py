#!/usr/bin/env python3
"""Run the Section 75 software gate and save evidence, using only Python stdlib."""
import argparse
import csv
import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GATES = {
    "Three-valued logic": ["and_or_truth_tables_cover_all_pairs", "c17_unknown_requests_evidence_invalid_requests_replanning"],
    "Explicit expiry": ["c2_expiry_without_version_change_is_explicit_and_propagates"],
    "Threshold support": ["two_of_three_truth_table_including_unknown_and_invalid"],
    "Alternative witnesses": ["c1_through_c4_preserve_support_log_substitution_and_isolate_unrelated_roots"],
    "Witness horizon oracle": ["generated_small_graphs_agree_with_full_and_exhaustive_oracles", "fifteen_node_boundary_is_exhaustive"],
    "Incremental equivalence": ["generated_large_graphs_agree_after_every_operation_and_expiry_batch", "full_and_incremental_lifecycle_scenarios_agree"],
    "Lease validation": ["c5_stale_lease_requires_fresh_authority_and_unrelated_epoch_is_allowed", "lease_deadline_wrong_nonce_phase_and_instance_never_dispatch"],
    "Plan versions": ["c6_old_plan_cancels_unstarted_actions_and_blocks_old_authority"],
    "Separate start/run contracts": ["c7_running_expiry_aborts_at_intermediate_deadline_and_old_command_is_superseded", "c8_start_loss_does_not_abort_and_alternative_running_support_refreshes_subscriptions"],
    "Commit contracts": ["c9_irreversible_commit_needs_fresh_valid_commit_evidence", "s3_expired_occupancy_blocks_commit_and_ground_truth_exposes_b0"],
    "Verified outcomes": ["c10_missing_acknowledgement_cannot_complete_or_unlock_successor", "missing_acknowledgement_cannot_unlock_successor"],
    "Concurrency": ["c11_joint_unsafe_actions_do_not_both_execute", "c12_safe_concurrency_has_equal_start_times", "supported_continuation_matches_exhaustive_feasibility"],
    "Partial replanning": ["c16_replanning_preserves_completed_and_committed_history", "invalid_suffix_replacement_is_atomic_and_cannot_reorder_executed_work"],
    "Reaction budget": ["c13_reaction_stages_fit_margin", "c14_deadline_miss_is_explicit", "clock_jump_and_fine_steps_deliver_reactions_at_the_same_boundary"],
    "Local completion controller": ["c15_committed_door_completes_locally_after_partition"],
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=ROOT / "target/week-four-verification")
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)

    def run(name, command, input_text=None):
        result = subprocess.run(command, cwd=ROOT, text=True, input=input_text, capture_output=True)
        (out / f"{name}.log").write_text(result.stdout + result.stderr)
        return result

    tests = run("tests", ["cargo", "test", "--offline", "--color", "never"])
    statuses = dict(re.findall(r"^test (\S+) \.\.\. (ok|FAILED|ignored)$", tests.stdout, re.M))
    gates = [{"requirement": title, "tests": names,
              "passed": tests.returncode == 0 and all(statuses.get(n) == "ok" for n in names)}
             for title, names in GATES.items()]
    checks = {}
    for name, command in [
        ("format", ["cargo", "fmt", "--all", "--check"]),
        ("clippy", ["cargo", "clippy", "--offline", "--all-targets", "--", "-D", "warnings"]),
    ]:
        checks[name] = run(name, command).returncode == 0
    base = ["cargo", "run", "--offline", "--bin", "sim", "--"]
    batch = out / "s3-b5"
    checks["batch_cli"] = run("batch", base + ["--scenario", "S3", "--out", str(batch)]).returncode == 0
    replay = out / "s3-replay"
    checks["replay_cli"] = run("replay", base + ["--replay", str(batch / "session.commands"), "--out", str(replay)]).returncode == 0
    interactive = out / "interactive"
    commands = "help\nadvance 0ms\nstatus\nexplain fan.start\nfault s1 drop\nadvance 1s\nplant occupancy occupied\nevents 3\nrun 6s\nsave " + str(interactive) + "\nquit\n"
    checks["interactive_cli"] = run("interactive", base + ["--interactive"], commands).returncode == 0 and (interactive / "manifest.json").is_file()
    try:
        for directory in [batch, replay, interactive]:
            manifest = json.loads((directory / "manifest.json").read_text())
            metrics = json.loads((directory / "metrics.json").read_text())
            events = [json.loads(line) for line in (directory / "events.jsonl").read_text().splitlines()]
            with (directory / "workload.csv").open() as stream:
                work = list(csv.DictReader(stream))
            assert manifest["source_hash"] and isinstance(manifest["fault_schedule"], list)
            assert metrics["TaskCompletionRate"]["denominator"] == 1
            assert events and work and "propagation_ns" in work[0]
            assert all(e["sequence"] == i + 1 for i, e in enumerate(events))
        assert (batch / "semantic.txt").read_bytes() == (replay / "semantic.txt").read_bytes()
        checks["artifact_validation"] = True
    except (OSError, ValueError, AssertionError, KeyError) as error:
        checks["artifact_validation"] = False
        (out / "artifact-error.txt").write_text(repr(error))
    report = {
        "scope": "Week 4 discrete software model; no physical certification or distributed experiments",
        "test_exit_code": tests.returncode,
        "passed_tests": sum(v == "ok" for v in statuses.values()),
        "ignored_tests": [n for n, v in statuses.items() if v == "ignored"],
        "gates": gates, "checks": checks,
        "passed": all(g["passed"] for g in gates) and all(checks.values()),
    }
    (out / "gate-report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"passed": report["passed"], "passed_tests": report["passed_tests"],
                      "gates_passed": sum(g["passed"] for g in gates), "report": str(out / "gate-report.json")}, indent=2))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
