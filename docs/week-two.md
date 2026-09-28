# Understanding Week 2

Week 1 built the reason-checking system. Week 2 checks its answers more thoroughly
and reduces how much work it repeats when an observation changes.

We now have two ways to evaluate the same graph:

| Evaluator | How it works |
| --- | --- |
| **Full** | Checks every evidence node, rule and conclusion again. |
| **Incremental** | Starts at the changed evidence and follows the connections that could change other answers. |

Both must produce the same conclusions, explanations and audit history.
The incremental evaluator is now the default. The full evaluator remains available
as a separate reference for testing and debugging.

## 1. First, independently check the explanations

Remember that a **witness** is the evidence selected to explain a valid conclusion,
and its **horizon** is the deadline for that support.

Suppose a 2-of-3 rule has valid observations expiring at 5, 8 and 12 seconds.
A checker can try every possible supporting group:

| Group | Supports the rule? | Horizon |
| --- | --- | --- |
| Sensor 1 alone | No | — |
| Sensors 1 and 2 | Yes | 5 seconds |
| Sensors 1 and 3 | Yes | 5 seconds |
| Sensors 2 and 3 | Yes | 8 seconds |
| All three sensors | Yes | 5 seconds |

The best available horizon is 8 seconds. Our runtime should select support with
that horizon, and its explanation must actually satisfy the rule.

We built an **exhaustive checker** that tries every subset of usable evidence on
small graphs. This checker does not call either production evaluator to calculate
support. It also does not reuse their witness-selection code.

Trying every subset grows expensive quickly, so this code is used only in tests,
with a limit of **15 total graph nodes**. We test that limit explicitly. It is not
part of normal runtime evaluation.

This checks the longest available support horizon. It does not claim the runtime
always finds the smallest possible evidence set across every tied combination.
The deterministic selection rules documented in Week 1 are retained.

## 2. Then, stop repeating unrelated work

Imagine these connections:

```text
Smoke sensors → hazard confirmed → fan and alarm start conditions
Occupancy sensor                → door start conditions
```

When occupancy becomes unknown, the fan and alarm do not depend on that reading.
There is no reason to evaluate those branches again.

The incremental evaluator:

1. Checks the changed evidence.
2. Checks conclusions that depend on it, with inputs processed before outputs.
3. Continues downstream if a result or its supporting witness changes.
4. Stops along a branch when its result and witness are unchanged.

A conclusion can stay valid while its witness changes. We still propagate that
change so downstream explanations remain correct. A changed horizon also matters.

A newer observation with the same status and deadline is recorded in the audit
log. If it changes no assurance value or witness, propagation stops at that leaf.
Version-bound execution permission is later work; this optimization does not
introduce leases or device dispatch.

## 3. Keep the existing expiry behavior

Evidence still expires at its deadline without needing a new sensor message.
When several observations expire at the same instant, all their expiry events
are processed before evaluating their effects together.

Advancing from time 0 directly to time 10 still processes intermediate deadlines
in order. Old deadlines cannot expire replacement observations. Rejected updates
still leave the evidence, time, epoch and audit history unchanged.

## 4. The four required scenarios

| Scenario | What our checks demonstrate |
| --- | --- |
| **C1: one sensor becomes unknown** | A 2-of-3 quorum stays valid if the other two still support it. |
| **C2: no new sensor message arrives** | Evidence expires on time without changing its observation version. |
| **C3: the selected evidence changes** | An alternative witness replaces lost support, and `WitnessChanged` records it. No planner is involved. |
| **C4: unrelated evidence changes** | Occupancy loss affects the door branch; fan and alarm results remain unchanged and their branches are not evaluated. |

## 5. Compare the evaluators across many experiments

Hand-written examples are useful, but cannot cover all combinations. We also
generate graphs and sequences of observation updates and clock advances.

A **seed** is the number used to reproduce the same generated experiment.

| Experiment set | Seeds | Operations per seed | Total operations |
| --- | --- | --- | --- |
| Small graphs, 2–10 nodes | 128 | 32 | 4,096 |
| Larger graphs, 16–100 nodes | 128 | 128 | 16,384 |

After every operation, tests compare the full and incremental results: statuses,
horizons, complete witnesses, evidence records, epochs and audit entries.
Small graphs also go through the independent exhaustive checker.

Additional runtime pairs step through intermediate expiry deadlines. Their
results are compared at every batch, and their complete histories must match
the runtimes that advance directly to the final time.

The sequences include valid, invalid and unknown observations, newer versions,
already-expired arrivals, stale versions and future observations. Fixed cases
cover simultaneous expiry, replacement, shared supporting evidence, tie-breaking,
restoration and changes to horizons or witnesses without losing validity.

We also check that unrelated roots keep their answers and explanations, and that
incremental evaluation work stays inside the potentially affected region.

Passing these tests is evidence about implementation correctness. It is not a
formal proof or a guarantee that a physical sensor is truthful.

## 6. Run the demonstration

From the project directory:

```sh
cargo run --offline --example week_two
```

For the occupancy-loss scenario, the example shows:

```text
Full:        8 nodes and 4 rules evaluated
Incremental: 2 nodes and 1 rule evaluated
Identical conclusions, witnesses and audit history
```

These are counts of evaluation work, not measured execution-time guarantees.
Finding which observations are due to expire still scans the current evidence
store. We have optimized graph evaluation, not every runtime operation.

Run the complete tests and code checks with:

```sh
cargo test --offline
cargo fmt --check
cargo clippy --offline --all-targets -- -D warnings
```

## 7. Choosing a mode in Rust

Existing calls now use incremental evaluation:

```rust
let runtime = AssuranceRuntime::new(graph);
```

For the full reference implementation:

```rust
use dcra::runtime::{AssuranceRuntime, EvaluationMode};

let runtime = AssuranceRuntime::with_mode(graph, EvaluationMode::Full);
```

`runtime.mode()` reports the choice. Other observation, clock, evaluation and
audit methods retain their existing meaning.

Two new read-only counters help inspect the work:

| Method | Meaning |
| --- | --- |
| `last_evaluation_stats()` | Nodes and rules evaluated in the last batch of the most recent accepted operation. |
| `operation_stats()` | Total evaluation work across every batch in that operation. |

Both return `EvaluationStats`, with `nodes_evaluated` and
`justifications_evaluated`. Initialization is excluded. An accepted operation
with no evaluation returns zero counts. Rejected operations preserve the
previous counters.

Internally, the incremental engine keeps a queue ordered by each node's position
in the dependency graph and updates cached results in place. A reached conclusion
rechecks all its incoming rules. It does not secretly run the full evaluator on
every update or copy the entire result map just to log changes.

## 8. Replaying an experiment that fails

If a generated experiment fails, the test prints the seed and operation index
and writes a trace under `target/week-two-failures/`. The trace contains the graph
and the entire event sequence, not just a seed.

Replay a trace using:

```sh
DCRA_REPLAY=tests/fixtures/event-boundaries.trace cargo test --offline --test week_two replay_saved_trace -- --ignored --nocapture
```

Replace the path with the failure trace to investigate. The included fixture
exercises deadlines, replacements, rejected updates and restoration, so replay
is tested even when generated comparisons find no failures.

The manual replay test is intentionally ignored during a normal test run because
it requires a file path. The included fixture also runs automatically in its own
test. After fixing a real generated failure, keep its trace in `tests/fixtures/`
and add a regular replay test to prevent recurrence.

Technical details: traces start with `DCRA_TRACE_V1`; strings are encoded as UTF-8
hex, time records preserve seconds and nanoseconds, and each record identifies a
node, rule, observation or clock advance. The test generator uses fixed SplitMix64
arithmetic, with small seeds 0–127 and larger seeds 10000–10127. No new dependencies
were added. These are test utilities, not a production file-ingestion API.

## 9. What comes next

Week 2 completes the planned witness checks and incremental evaluation. The
three-valued logic and other documented Week 1 assumptions still apply; the
referenced Formal Models were not among the supplied documents.

[Week 3 is now implemented](week-three.md): action contracts, leases and lifecycle handling. The
Python AI planner, physical simulator and hardware integration remain later work.
