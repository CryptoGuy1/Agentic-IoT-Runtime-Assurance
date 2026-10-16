# Model v1: first tested simulation core

Version: `evidra-model-v1.1`, 15 October 2026. This is the corrected, executable first-delivery specification following the two refinement reviews.

## What this delivery does

The simulated world knows the actual state. The estimator sees only delivered observations, issued commands, its initial assumptions and declared limits. It maintains a range of possible states. The checker asks whether all those possibilities satisfy the configured action constraints over a limited future window. An independent scorer evaluates an applied-now candidate using the actual simulated world.

The experiment runner performs scripted interventions. Its checker is not yet connected to the existing action-execution runtime or the browser. The saved comparisons are component diagnostics, not new B0–B6 results. Qwen3 and hardware are not needed to reproduce them.

## Run an example

From the repository root:

```sh
cargo run --offline --bin model_v1 -- --scenario short-gap --seed 42 --out target/model-short-gap
cargo run --offline --bin model_v1 -- --scenario long-gap --seed 42 --out target/model-long-gap
```

Choose a new output directory each time. The parent directory must exist. Each run writes `report.md` for reading and `results.json` for inspecting values and replaying the scenario with the same source version.

`--help` lists all 13 fixtures: normal, short-gap, long-gap, delayed-reading, delayed-command, lost-command, occupancy-entry, protected-room, fan-failure, door-jam, conflict, mismatch and pressure-limit.

The short-gap fixture omits pressure readings from 1 to 4 seconds and checks at 3 seconds. Its last pressure reading, taken at 0.5 seconds, has expired for direct use. Its earlier information is still carried forward through the physical model. With the door open, the resulting pressure range supports the proposed fan action. The long-gap fixture starts with a running fan, closed door and elevated pressure; the possibilities include unsafe pressure, so it returns Unknown. These are deliberately different controlled fixtures, not a claim that elapsed time alone explains their different results.

## Physical model and timing

The model uses the handoff's normalized equations and parameters: hazard decay 0.02, fan hazard removal 0.20, fan time constant 1.5 seconds, pressure decay 0.60, pressure coupling 0.55 and door speed 0.40. The active hazard source is 0.08. Disturbance limits per second are 0.01 for hazard/door and 0.02 for fan/pressure. These are experimental units, not calibrated building measurements.

Version 1.1 uses a fixed **100 ms discrete tick**. This is below the plan's 200 ms maximum and exactly divides the 500 ms sensor period. Physics is defined on this grid, not claimed as a verified continuous-time approximation. Between ticks there is no separately modeled trajectory. Network receipts retain their exact timestamps; the device and estimator process them on the next tick. Command delay bounds include this wait. Public model advancement and observation generation times must use the grid.

At each tick: advance physics using the previous inputs; apply due exogenous events and delivered commands; generate due observations; deliver sensor messages; update the estimate; record/check. Commands delivered at a boundary affect the following physical step. Alarm activation additionally takes 200 ms. Commands have monotonically increasing IDs; devices reject duplicated or superseded IDs.

Fan failure uses the handoff's faster decay, and a jammed door stays in place. The default model permits faults to begin at any step. Their possible modes remain represented after ordinary numeric observations; the estimator does not infer perfect health from a successful reading.

The nominal pressure dynamics can exceed the 0.80 limit with a running fan and closed door. The pressure-limit fixture intentionally retains that behavior. These checks make no claim that a short safe prediction implies indefinite safe operation.

## Observation and estimate rules

`Observation` contains source identity, variable, value, declared error, generation and receipt times, age limit, version and quality. Numeric readings require finite values and finite nonnegative error bounds. Sources and versions must be valid before any mutation occurs.

- Trusted measurements constrain the state **at generation time**. The estimator then replays the full retained history to the present using recorded commands and declared assumptions. Previous experiment decisions are never rewritten.
- An arriving reading with `now >= generated + max_age` is expired and has no new measurement effect. An already accepted reading can continue influencing a model prediction; expiry does not erase history or turn a predicted claim into a direct fresh measurement.
- Duplicates do not reapply constraints. Older source versions are ignored. A newer version with an older timestamp, a reused ID with conflicting payload, future input and malformed data are rejected without changing the estimate.
- Missing quality supplies no constraint. Contradictory trusted observations or explicit Inconsistent quality latch an inconsistency. The first release requires a fresh estimator/reset to clear it; later good samples do not silently clear it.
- If a constraint intersection is empty, retain the nonempty predicted set for diagnosis and mark it inconsistent. Such a set never supports authorization.
- Store an interval for each numeric variable and explicit alternatives for occupancy, alarm, health modes and hazard-response obligation. Each interval operation rounds outwards. Inputs are validated into bounded finite domains before prediction.

No-observation uncertainty can expand, contract or saturate according to dynamics and clipping. Tests check containment, not an incorrect requirement that every width grows monotonically.

The first implementation retains history in memory and limits one estimator run to one simulated hour. It is a correctness prototype, not yet an optimized bounded-memory live service.

## Occupancy and command assumptions

People may enter immediately after an empty-room sample. The default estimator therefore admits both occupancy possibilities on the next step, even while the last sample is fresh. A door-close check remains Unknown if that future possibility matters.

The protected-room fixture explicitly supplies an occupancy-stability interval to both the experiment and the estimator. It is a declared test assumption, not a clearance certificate inferred from the simulator's secret event schedule. A physical installation would need a justified protective mechanism before using an equivalent assumption.

An issued command supplies a maximum application delay and whether it can be lost. Before guaranteed delivery, prediction covers the old and new actuator inputs. When loss is possible, both remain possible after the nominal delay. A later guaranteed newer command excludes obsolete older commands. Numeric feedback constrains physical state without turning dispatch into acknowledgement or success.

## Action contracts and result interpretation

| Action | Prediction window | Physical target / deadline |
| --- | --- | --- |
| AlarmOn | 2 seconds | Alarm on within 2 seconds |
| FanStart | 5 seconds | Fan fraction at least 0.90 within 4 seconds |
| FanStop | 6 seconds | Fan fraction at most 0.05 within 6 seconds |
| DoorClose | 4 seconds | Door fraction at least 0.95 within 4 seconds |
| DoorOpen | 4 seconds | Door fraction at most 0.05 within 4 seconds |

FanStop's previously unspecified near-zero threshold and three-second check were replaced by an explicit 0.05 target and six-second window. This permits the nominal modeled ramp-down to settle. Delayed commands and their application times are recorded separately. Candidate scoring in this release starts from immediate application at the check time.

Every checked state, including the initial state, must satisfy `abs(pressure) <= 0.80`. DoorClose additionally requires occupancy clearance throughout the window. FanStop must not violate an active hazard-response obligation: that obligation activates at hazard 0.60 and clears at 0.30. DoorOpen has the pressure constraint and separately measured outcome; jams do not automatically mean physical danger. These are deliberately small benchmark contracts.

`Supported` means the represented trajectory enclosure satisfies these constraints. `Contradicted` means a constraint is certainly violated at a checked boundary. Other cases are `Unknown`. This is a sufficient check: it may return Unknown for a physically safe action. Prediction over a nonempty box is essential, and an inconsistent estimate always returns Unknown.

`CheckResult` records the action, check time, horizon, observation references, pressure enclosure and reasons. **It is not an execution lease.** The applied-now counterfactual check does not authorize a command that may arrive later. Runtime integration must add candidate transit/application uncertainty and current phase/lease checks.

The independent scalar scorer records configured-constraint safety, physical safety, usefulness, physical success, deadline satisfaction and an initial-pressure-violation flag. Its physical safety check detects pressure violations and occupied closing motion. Its clearance contract is deliberately stricter than motion safety. A jam can fail the target while remaining physically safe. Alarm deadlines are recorded as task outcomes in this release, not mixed into the pressure/motion safety metric.

Counterfactual scoring clones the plant and follows the scenario's future events and keyed disturbances. It neither mutates the live world nor reads the interval checker. All built-in fixtures have no additional future scripted command after the check. `outcome_confirmed` is always false in these component results: simulated counterfactual success is not actuator feedback.

## Reproducibility and comparisons

JSON includes model/contract versions, source and configuration fingerprints, seed, actual and estimated state at every tick, containment at the decision, command events, delivered observations and the independent result. FNV-1a fingerprints identify configurations and source bytes; they are not cryptographic integrity guarantees.

The diagnostic comparators are clearly named:

- **Range-based model:** the new check.
- **Fresh readings plus model:** the same check with an additional direct-freshness requirement; this isolates what the age gate costs in the short-gap fixture.
- **Optimistic point projection:** repeatedly collapses ranges to midpoints and chooses false for unknown Boolean values. It illustrates the risk of discarding possibilities, not a fully engineered point estimator.

These names do not reuse historical B3/B5 meanings. No Unsafe Release Rate is reported because this runner does not authorize policy-selected actions. Containment counts are separate from inconsistency counts. The mismatch fixture intentionally sets a faster true fan than the estimator expects and suppresses fan feedback, exposing failed assumptions rather than hiding them.

## Verification and next boundary

```sh
cargo test --offline
cargo fmt --all -- --check
cargo clippy --offline --all-targets -- -D warnings
cargo build --offline --bins
python3 -m unittest discover -s scripts -p 'test_*.py'
```

Tests cover arithmetic enclosures, hand calculations, delayed history updates, invalid-input atomicity, missing/contradictory observations, occupancy, delayed/lost/reordered commands, extreme bounded disturbances, hidden faults, independent scoring, JSON validity, replay and output-directory protection. Existing runtime and web/AI-adapter tests remain part of regression checking.

See the [measured results and limitations](model-v1-results.md) for the completed 13-scenario seed-42 run and verification totals.

The next delivery connects model-backed physical claims to the runtime, retains separate permissions/outcome evidence, and checks feasible recovery and persistent device effects. It must add candidate-delivery windows and phase-specific guarantees before these component verdicts can control actions. Full policy comparisons, the browser display, autonomous planning and Pi I/O follow that work.
