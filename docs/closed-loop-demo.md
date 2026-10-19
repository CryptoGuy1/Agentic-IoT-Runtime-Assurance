# End-to-end simulation demonstration

Delivery: 16 October 2026. Experiment schema: `model-closed-loop-v1`.

The model-backed runtime now participates in a complete software experiment:

```text
Hidden simulated plant
  → sampled sensor messages with timing/error metadata
  → estimator and versioned physical claims
  → permissions, leases, action/recovery checks
  → physical command release and delayed device receipt
  → actual simulated motion
  → separate acknowledgement and measured completion reports
  → continued assurance monitoring
```

An independent scalar scorer evaluates the recorded actual trajectory after the loop finishes. It never supplies labels to the runtime. This execution path uses `ModelRuntime::with_claims`, not the older browser simulation or the isolated applied-now counterfactual runner.

## Run and present

```sh
cargo build --offline --bins
python3 scripts/run_model_demo.py --out target/NEW_DEMO_DIRECTORY
```

Choose a new directory whose parent exists. The command saves all seven runs and an index `report.md` with a manifest of seeds and source/configuration fingerprints. Existing directories are never overwritten. For one scenario:

```sh
cargo run --offline --bin model_demo -- --scenario short-gap --seed 42 --out target/NEW_SHORT_GAP_DIRECTORY
```

Start the client walkthrough with normal, short-gap and recovery, then show lost-feedback. Read the scenario's `report.md`; use `events.jsonl` to inspect the linked messages and decisions. `results.json` includes configuration, actual states, estimates, containment, certificates and post-run scores. Events are exported in groups at each model boundary; their timestamps identify source/decision times rather than an independent wall-clock performance measurement.

The delivered browser interface has not yet been connected to this path. This delivery is a command-line demonstration with inspectable reports.

## Scenarios

| Scenario | Purpose |
| --- | --- |
| `normal` | A permitted fan start changes the simulated world; a separate device sensor observes the target before the runtime completes. |
| `short-gap` | Pressure messages are absent from 500 to 4000 ms. At the 1000 ms proposal the direct predicate has expired, but the propagated claim supports the action. |
| `recovery` | A declared door fraction of 0.9 creates a pressure-sensitive ventilation test. Continued prediction requests and checks fan-stop recovery before actual pressure exceeds its limit. |
| `uncertain` | Initial truth is pressure 0.79, while trusted information admits 0.76–0.82. The initial proposal is withheld. Only one proposal is scheduled: becoming Ready later does not constitute an automatic retry. |
| `lost-feedback` | Correlated acknowledgement/completion messages are dropped while ordinary sensors continue. Physical success remains distinct from task confirmation. |
| `delayed-command` | A 600 ms command transit is applied literally. Reduced recovery headroom can cause an early checked stop before the original target. |
| `delayed-reading` | Sensor messages arrive 300 ms after generation and constrain the estimate at their source times. |

The fixed goal is to bring the fan fraction to at least 0.90 for a ventilation test. Hazard starts at zero and the true source is zero, so configured fan-stop recovery does not conflict with an active ventilation obligation. This is not a fire-response mission. A hazard-active mission needs a different justified recovery policy.

The nominal physical equations are unchanged. The recovery fixture's initial geometry is explicit in both the public prior and hidden setup; it does not change the pressure limit or tune model coefficients. Actual states and priors are constructed separately. Model fault alternatives remain conservative even though these fixtures inject no device faults.

## Timing and adapter boundary

Physics uses the existing 100 ms discrete model. Each step evolves with the previous actuator inputs. Due commands then reach the simulated device and change the next step's inputs. Normal command transit is 100 ms; delayed-command uses 600 ms. Both are represented in candidate and recovery checks.

The simulated adapter emits a correlated acknowledgement when the device receives the command. A separate exact simulated device sensor tests its physical target every tick and emits a completion report only when the target is measured. Feedback takes a further 100 ms to arrive. Normal task completion still requires the runtime's correlated outcome contract; recovery confirmation terminates recovery without claiming the original fan-start objective succeeded. Generated feedback does not read checker or scorer verdicts.

Ordinary observations use the existing declared error bounds but zero measurement error in these fixtures. Plant disturbances use independently keyed seeded bounded samples. This isolates the integration and timing behavior; it is not a broad measurement-noise campaign. Pressure's direct TTL is 500 ms. Full observation metadata and model assumptions remain inspectable in the reports.

## Independent execution scoring

`execution_score` imports scalar truth records and action definitions, not the estimator/checker/runtime. It scans the recorded trajectory; it does not clone the plant and pretend a candidate was applied immediately.

For each physically released command it records actual application, first physical target within the action window, deadline satisfaction from release including command transit, target usefulness at release, observed constraint/physical safety and accepted correlated confirmation. A late or missing target is distinct from a missing confirmation. No application gives no command-target credit. No reached target with an unfinished deadline is `null` (censored), not a fabricated failure. Partial safety windows are explicitly flagged.

The run score records initial pressure violation separately, pressure-violation ticks, occupied closing motion, false runtime completion claims at their observation timestamps, unconfirmed physical targets and the final ventilation-test objective. An achieved target is an observed effect, not proof of exclusive causal attribution to a particular command. Initial/exogenous violations are not automatically described as harm caused by a released action.

Truth-based usefulness and outcomes do not by themselves establish that a decision was justified by the available information. An independently specified information-sufficiency assessor, scoring denominators for repeated/wait decisions, and the 80% milestone remain future benchmark work.

## Reproducibility and limits

The CLI writes `report.md`, `results.json` and `events.jsonl` to a newly reserved directory. JSON includes source/configuration fingerprints, seed, priors, true/model dynamics, timing, objective and contract/scorer revisions. Certificate observation IDs resolve to delivered observation records, and command IDs resolve to release/receipt events. Fingerprints use FNV-1a for identifying source/configuration changes, not cryptographic integrity.

Tests cover actual receipt/response/confirmation ordering, measured recovery, missing feedback, delayed readings/commands, conservative withholding, deterministic replay, JSON validity, reference linkage, output protection and scorer failures/censoring. Existing model/runtime and claim-layer regression tests remain required.

Remaining demonstration work is browser presentation and a guided live control/replay view. The larger research work still includes versioned B5/B6 matched comparisons, independent information-justification labels, multiple recovery alternatives, autonomous replanning and Pi I/O. These software runs are not real-device measurements, a general safety proof or a policy-superiority result.


## Recorded validation

The [seed-42 results](closed-loop-results.md) summarize the seven saved runs.
Verification completed on 16 October 2026: 150 Rust unit/integration tests plus one
compile-fail documentation test passed, including seven new closed-loop tests and
three independent-scorer tests. One existing manual replay test remained ignored.
All 21 Python tests passed, including the three new CLI/artifact tests; formatting,
Clippy with warnings denied, offline binary/example builds and whitespace checks
passed. Scenario replay was byte-identical in the automated checks.
