# Model v1 measured results — first delivery

Validation date: 16 October 2026. Model: `evidra-model-v1.1`. Seed: `42` for the table. Source fingerprint: `fnv1a64:badf0f332fcea1d2`.

These are separate, scripted component experiments. They test the new estimator and action checker before connection to the execution runtime. They do not measure the complete agent, the historical B0–B6 policies, or the project’s 80% decision-quality target.

## What the results mean in everyday terms

- During the short pressure-sensor interruption, the model can still establish the configured constraints. Requiring a fresh pressure reading would return Unknown.
- If a person might enter while the door closes, the model returns Unknown. A single empty-room reading is insufficient.
- A safe attempt can still fail: the broken fan and jammed door do not reach their targets. Safety and successful completion are recorded separately.
- Incorrect model assumptions can invalidate the estimate without automatically triggering an inconsistency. The mismatch example deliberately exposes this limitation.

“Supported” means all represented possibilities meet the configured constraints within the checked time window, assuming the declared bounds are correct. “Unknown” means the check cannot establish that. Neither result is an execution permission in this delivery.

## Recorded runs

“State covered” counts recorded times when the actual simulated state lies inside the estimated possibilities. There are 101 records from 0 to 10 seconds. It is not a decision-accuracy percentage. “Candidate safe” is the independent scorer’s result for the configured constraints; “target reached” is its separate physical-outcome result. The candidate is applied immediately in a cloned world, so these outcomes are not feedback from the live device trace.

| Scenario | Range check | Fresh readings + model | Optimistic point | State covered | Candidate safe | Target reached | Explanation |
| --- | --- | --- | --- | --- | --- | --- | --- |
| [normal](../target/model-v1-validation-ndv89blv/normal/report.md) | Supported | Supported | Supported | 101/101 | Yes | Yes | Ordinary fan-start example. |
| [short-gap](../target/model-v1-validation-ndv89blv/short-gap/report.md) | Supported | Unknown | Supported | 101/101 | Yes | Yes | Pressure reading expired; prediction still supports the action. |
| [long-gap](../target/model-v1-validation-ndv89blv/long-gap/report.md) | Unknown | Unknown | Supported | 101/101 | No | Yes | Actual pressure is already unsafe at the check; the range includes safe and unsafe values. |
| [delayed-reading](../target/model-v1-validation-ndv89blv/delayed-reading/report.md) | Supported | Supported | Supported | 101/101 | Yes | Yes | A late reading constrains its generation time and is propagated forward. |
| [delayed-command](../target/model-v1-validation-ndv89blv/delayed-command/report.md) | Supported | Supported | Supported | 101/101 | Yes | Yes | Earlier command delay is included in the estimate. |
| [lost-command](../target/model-v1-validation-ndv89blv/lost-command/report.md) | Supported | Supported | Supported | 101/101 | Yes | Yes | Earlier command loss is included; the scored candidate is applied immediately. |
| [occupancy-entry](../target/model-v1-validation-ndv89blv/occupancy-entry/report.md) | Unknown | Unknown | Supported | 101/101 | No | Yes | Possible entry blocks support before the scripted person enters. |
| [protected-room](../target/model-v1-validation-ndv89blv/protected-room/report.md) | Supported | Supported | Supported | 101/101 | Yes | Yes | Support depends on the explicitly declared occupancy-stability interval. |
| [fan-failure](../target/model-v1-validation-ndv89blv/fan-failure/report.md) | Supported | Supported | Supported | 101/101 | Yes | No | The checked constraints hold, but the fan fails to reach its target. |
| [door-jam](../target/model-v1-validation-ndv89blv/door-jam/report.md) | Supported | Supported | Supported | 101/101 | Yes | No | The checked constraints hold, but the door fails to reach its target. |
| [conflict](../target/model-v1-validation-ndv89blv/conflict/report.md) | Unknown | Unknown | Unknown | 101/101 | Yes | Yes | Conflicting observations latch inconsistency for 91 of 101 recorded states. |
| [mismatch](../target/model-v1-validation-ndv89blv/mismatch/report.md) | Supported | Supported | Supported | 50/101 | Yes | Yes | Incorrect fan dynamics leave the true state outside the estimate; Supported is not justified by valid assumptions. |
| [pressure-limit](../target/model-v1-validation-ndv89blv/pressure-limit/report.md) | Supported | Supported | Supported | 101/101 | Yes | Yes | Alarm candidate meets the two-second check; pressure later exceeds the limit in the ten-second trace. |

The optimistic point comparator discards alternatives at every step. Its Supported results for long-gap and occupancy-entry illustrate the danger of that simplification; they do not establish performance against an engineered point-estimation baseline.

Short-gap and long-gap use different initial physical conditions as well as different interruptions. Their comparison does not isolate the effect of outage duration. In long-gap, the unsafe pressure exists before the candidate starts.

In mismatch, only 50/101 states are covered, including failure to cover the state at the decision. All three checks still return Supported and no inconsistency is detected. This run must not be counted as a successful assurance result merely because its particular candidate happens to satisfy the scorer’s constraints. Model validation and monitoring remain necessary.

In pressure-limit, the action is checked from 0 to 2 seconds. Actual pressure reaches approximately 0.914 at 10 seconds, above the 0.80 limit. A finite check cannot justify leaving a persistent effect running indefinitely.

## Reproduce and inspect

The [saved run index](../target/model-v1-validation-ndv89blv/summary.json) contains source/configuration fingerprints and exact results. Individual scenario directories also contain full `results.json` traces, commands and observations. These local artifacts are under `target/` and are removed by `cargo clean`; the table and reproduction instructions remain in this document.

```sh
cargo run --offline --bin model_v1 -- --scenario short-gap --seed 42 --out target/model-short-gap
cargo run --offline --bin model_v1 -- --scenario occupancy-entry --seed 42 --out target/model-occupancy-entry
cargo run --offline --bin model_v1 -- --scenario mismatch --seed 42 --out target/model-mismatch
```

Choose a new output directory for each run. The parent directory must exist. The [implementation guide](model-v1.md) lists all scenarios, assumptions and verification commands.

## Verification completed

- Rust: 110 unit/integration tests passed, plus one documentation test confirming that external code cannot import the private true-state implementation. One pre-existing manual trace-replay test remains ignored unless a trace is supplied.
- Python: 18 tests passed, including the existing web/planner adapter tests and three new command-line/report tests.
- Offline builds, Rust formatting and Clippy with warnings denied passed.
- All 13 scenarios replay identically in the automated checks. Additional coverage tests exercise seven scenarios across five noise seeds and all 16 signed disturbance extremes across 80 steps with hidden faults.
- All 123 original reading-list entries remain preserved and mapped in the research register, which contains 100 records including the added interval-analysis reference.

These checks establish the tested behavior and reproducibility of this component. They do not prove correctness for every possible input or physical installation.

## Next implementation boundary

Connect model-based claims to runtime phases only after adding candidate communication delay/loss, safe recovery, and checks for effects that persist after an action completes. Then expose the results in the browser and run matched policy comparisons. The current web interface continues to use its existing runtime behavior.
