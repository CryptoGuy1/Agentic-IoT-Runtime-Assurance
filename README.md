# Dependency-Closed Runtime Assurance

This project is building a **general runtime assurance system** that uses current
evidence to assess whether decisions are justified, monitors execution, and checks
outcomes. Its research goal is to make appropriate decisions reliably under
uncertainty and demonstrate improvement over the existing approaches represented
by its benchmarks.

The target is **at least 80% correct decisions within a defined benchmark scope**.
This is a development and evaluation target, not an achieved result or a guarantee
that every individual decision has an 80% probability of being right. Establishing
that target requires an independent definition of the right choice, representative
scenarios, and reporting of measurement uncertainty.

Fans, doors, alarms and other building actions are the **first demonstration of
the assurance idea**. They provide a controlled environment for testing the
runtime; they do not define the intended limits of the system. We are building
towards an assurance core whose evidence, decision and execution mechanisms can
be applied through domain-specific models and adapters, with broader applicability
validated through additional scenarios and domains.

## Decision quality and benchmarking

Every decision the runtime makes should be recorded and benchmarked against an
independently defined expected response. Evaluation must cover decisions to allow
or withhold an action, continue or abort execution, permit commitment, and accept
an outcome. The expected response must account for the scenario's true state,
objectives, constraints and timing; the runtime's own rules cannot serve as the
sole judge of correctness.

The benchmark documents guide this evaluation. As implementation progresses, we
need to:

- Define the decision categories, expected responses and scoring denominators,
  including how repeated decisions and decisions to wait are counted.
- Measure incorrect actions and missed useful actions together, so withholding
  every action cannot satisfy the decision-quality target.
- Compare the runtime with relevant baselines under matched scenarios and fault
  conditions, reporting both aggregate results and results by decision category
  and scenario.
- Preserve decision traces, evidence, outcomes and replay instructions so results
  can be inspected and reproduced.
- Evaluate the 80% target with uncertainty estimates and broaden the benchmark
  beyond the initial building demonstration.

**Supported evidence is not proof of a correct decision.** Sensors, declared
dependencies or objectives can be wrong even when the runtime follows its rules.
Evidence assurance and independently measured decision quality are therefore
separate parts of the research.

## Current implementation and evidence

This Rust library implements the **Weeks 1–5 evidence assurance, action execution,
building simulation and communication-fault experiments** from the Agentic IoT
Six Week Research Timeline, using Implementation Specification v1 and the
timeline's counterexample tests. It models declared evidence support; it does not
establish sensor truth or physical safety, or control physical devices. Commands
are queued for a deterministic fake actuator. Week 4 adds an observation-driven
safety interface, a discrete building model and terminal experiments.

The existing pilot checks reference agreement, false-safe releases, action
completion and processing costs. These provide evidence about the implemented
mechanisms within the simulation, but **do not yet establish 80% decision
correctness or general superiority over real-world systems**. See the
[Week 5 pilot findings](docs/week-five-findings.md) for measured results and their
limitations.

A validated dependency graph expresses AND, OR and k-of-n justifications. Each
valid conclusion has a preferred witness identifying its supporting observations
and a horizon showing when that support expires. A deterministic runtime accepts
versioned observations, processes explicit expiry events, and records transitions.

## Run and inspect

The planned hardware testbed uses our Raspberry Pi 5 for the assurance runtime,
benchmark orchestration and dashboard, and our Raspberry Pi 4 for sensor inputs
and the local actuator/gateway adapter. Start by running the existing software,
then add the distributed communication and physical I/O adapters. See the
[Raspberry Pi testbed and provisional shopping list](docs/raspberry-pi-testbed.md)
for staged purchases, implementation work and benchmark requirements. The current
implementation still uses simulated observations and a fake actuator.

For the local browser demonstration, run `python3 scripts/run_web_demo.py` and open
`http://127.0.0.1:8765`. See [the web demo guide](docs/web-demo.md) for the two guided
meeting examples, controls and presenter script. The interface runs the existing
Rust simulator; no external packages or real devices are required.

With a Rust toolchain supporting edition 2024 (no external dependencies):

```sh
cargo run --offline --example week_one
cargo run --offline --example week_two
cargo run --offline --example week_three
cargo run --offline --bin sim -- --interactive
cargo test --offline
cargo fmt --check
cargo clippy --offline --all-targets -- -D warnings
```

The `week_one` example demonstrates:

- **C1:** a 2-of-3 hazard quorum stays valid after one sensor becomes unknown,
  with the supporting witness changing to the remaining two sensors.
- **C2:** evidence version 10 expires at simulated time 2 without a new sensor
  message. At time 3 its status is unknown, its version is still 10, and the
  explicit expiry event is recorded at time 2.

Read [Understanding Week 1](docs/week-one.md) for the foundation and
[Understanding Week 2](docs/week-two.md) for witness verification, incremental
updates and reproducible experiments. Read [Understanding Week 3](docs/week-three.md)
for contracts, leases, lifecycle handling and verified outcomes. The [Week 1 technical reference](docs/week-one-reference.md)
records the underlying semantics and assumptions.

The `week_two` example compares both evaluators through C1–C4. When occupancy
becomes unknown, the incremental evaluator checks only 2 nodes and 1 rule;
the full evaluator checks all 8 nodes and 4 rules. Their results and audit
histories match. These counts measure evaluation work, not a timing guarantee.

## Implementation

| Module | Responsibility |
| --- | --- |
| `evidence` | Three-valued status, assurance values, complete evidence metadata |
| `graph` | Validated immutable DAG, topological order, premise/conclusion indexes |
| `evaluator` | Full evaluation of every rule and node, preferred witnesses and horizons |
| `runtime` | Mode selection, versioned updates, epochs, expiry, audit history and work counts |
| `incremental` (internal) | Cached evaluation with propagation through affected dependencies |
| `actions` | Validated plans/contracts, leases, local command dispatch, running assurance and actuator results |
| `simulation` | Building truth/observations, B0–B5 admission policies, virtual events, replanning, terminal controls and experiment reports |
| `clock` | Monotonic virtual clock; no real sleeping or wall-clock decisions |

Use `AssuranceRuntime` for evidence updates and expiry processing. `AssuranceStatus`
is the single status enum used by evidence and derived assurance values.
`AssuranceRuntime::new(graph)` now defaults to incremental evaluation. Use
`AssuranceRuntime::with_mode(graph, EvaluationMode::Full)` for the independent
full reference evaluator. Both expose the same evidence and audit API.

## Week 1 completion gate

`tests/week_one.rs` verifies hand-worked truth tables and horizons, C1/C2,
malformed graph rejection, nested support, deterministic ties and replay,
explicit expiry at the deadline, simultaneous expiries, replacement observations,
and atomic rejection of malformed or stale updates. No test uses real sleeping.

## Week 2 completion gate

- A test-only exhaustive checker verifies supporting subsets and maximum horizons
  independently on small graphs, including the 15-node boundary.
- C1–C4, witness substitution, horizon changes, shared evidence, tie-breaking and
  locality checks pass for the full/incremental implementations.
- 128 small seeds × 32 operations and 128 larger seeds × 128 operations compare
  results and audit histories, including intermediate expiry batches.
- Failures save replayable traces under `target/week-two-failures/`. The included
  event-boundary fixture tests replay even when no generated failure occurs.

For a manual replay:

```sh
DCRA_REPLAY=tests/fixtures/event-boundaries.trace cargo test --offline --test week_two replay_saved_trace -- --ignored --nocapture
```

The manual replay test is the one intentionally ignored test in a normal run;
the fixture also has an automatic test. See the Week 2 guide for the trace format
and counter semantics. Expiry discovery still scans the evidence store, and the
exhaustive oracle is never part of production evaluation.

## Week 3 completion gate

`ActionRuntime` owns its assurance runtime and execution state. It validates
single-use start/commit leases against time, plan and selected evidence versions;
monitors separate running contracts; and requires correlated outcome evidence
before completion. Its default evaluator is incremental; full mode remains available.

The Week 3 example shows a rejected stale lease, fresh authorization, an unknown
outcome blocking a successor, and a safe-abort request at the precise evidence
expiry boundary. `tests/week_three.rs` covers C5–C10/C17 plus result ordering,
controller failures, deadlines, output resets and cascading running obligations.

See the [technical reference](docs/week-three-reference.md) for exact lifecycle
and policy defaults. The fake controller is a test adapter, not physical safety
certification. Commands and logs are in-memory and non-durable.

## Week 4 completion gate

Read [Understanding Week 4](docs/week-four.md) to run and interact with the building.
The [technical reference](docs/week-four-reference.md) documents comparison
policies, modelling assumptions, event ordering and metric denominators.

```sh
cargo run --offline --bin sim -- --scenario S3 --baseline B5 --out target/s3-run
cargo run --offline --bin sim -- --replay target/s3-run/session.commands --out target/s3-replay
python3 scripts/verify_week_four.py
```

The verification script saves a machine-readable gate report under
`target/week-four-verification/`; use `--out` with a new directory for later runs.
It covers the complete test suite and batch, replay and interactive CLI paths.
All software simulation remains Rust. Python orchestrates verification and remains
the planned language for the later AI adapter and analysis. Week 4 uses a scripted
planner; no AI account or external service is required.


## Week 5 communication-fault pilot

Read [Understanding Week 5](docs/week-five.md) for the end-to-end explanation and
[the technical reference](docs/week-five-reference.md) for transport and measurement details.

```sh
cargo run --offline --bin sim -- --scenario S1 --network compound --interactive
python3 scripts/run_week_five.py
```

The pilot runs the prerequisite gate, matched B0–B5 experiments, targeted fault
checks, feature removals, saved-manifest replays and a full/incremental scale sweep.
Results default to `target/week-five-pilot/`; supply `--out` with a new directory
for subsequent runs. Read `pilot-report.md` first. Network transit uses virtual
time; measured host processing time is reported separately. Week 6's AI planner
is not included.


The [first Week 5 pilot findings](docs/week-five-findings.md) document the completed
210-run dataset, measured processing costs, and inspected neutral/adverse results.

The web demo also supports **AI · Ollama Qwen3 1.7B** planning using the local `qwen3:1.7b` model. Select AI mode, generate a plan, then advance virtual time. The model proposes actions; Rust/DCRA retains all execution checks. See [web demo instructions](docs/web-demo.md) for constraints and saved planner traces.
