# Week 4 technical reference

This implementation follows the six-week timeline's software simulation milestone
and Implementation Specification v1's physical interface, event loop, baselines,
C11–C16, S1–S5, metrics and verification gate. C1–C10/C17 remain covered by the
existing suites. Unspecified choices below are implementation defaults; the
referenced Formal Models were not available.

## Components and boundaries

`src/simulation/model.rs` defines plant truth, observation estimates, the
`PhysicalSafetyOracle` interface and compatible subsets. `config.rs` builds the
phase graphs, action definitions, policy variants and scripted plans. `engine.rs`
owns virtual events, the actuator adapter, observations and terminal commands.
`report.rs` exports experiments. `src/bin/sim.rs` provides the CLI.

`ActionRuntime` still privately owns all execution-related state. The simulator
uses its public update, lease, dispatch, result and clock APIs. It cannot directly
mark an action Completed. The baseline adapter exposes `on_evidence_update`,
`request_start`, `request_commit`, and `on_action_outcome` for every policy.
There is no unsafe bypass added to the assurance runtime.

`replace_pending_suffix` validates an entire candidate DAG before any mutation.
It retains started instances and their original attribution, preserves edges
between them, rejects new incoming dependencies on retained instances, cancels
unstarted old instances, and adds fresh identities. Historical cancelled instances
remain inspectable. An executing old-version action that still needs commitment
uses the existing precommit cancellation controller. Completed and committed
instances are never restarted or silently adopted into a new execution version.

The scripted planner receives `StateEstimate` and execution history. Presets have
fixed goals, so their action choices are deterministic; the estimate is available
at the interface and recorded with replanning, not interpreted by an AI model.

## Discrete building model

Three hazard and occupancy slots exist; the action scenarios target zone 2.
`s1`, `s2`, and `thermal` observe the same zone-2 hazard, independently subject to
observation faults. No fluid dynamics, fire propagation or calibrated sensor
noise model is claimed. Pressure is an integer with an illustrative minimum 50.

| Action ID | Normal duration | Phases / response policy |
| --- | --- | --- |
| alarm | 100ms | Start/outcome; preemptible |
| fan | 3s | Start/run/outcome; preemptible |
| damper | 2s | Start/outcome; completion-safe |
| door | 2.5s | Start/run/commit/outcome; completion-safe |
| arm | 1s | Start/outcome; reversible |
| discharge | 200ms | Start/commit/outcome; irreversible |
| light | 100ms | Additional C12 concurrency fixture |

Maximum duration is normal duration plus 2s. The lease cap is 1s. Correlated
outcome evidence has a 30s maximum age. Normal acknowledgement takes 10ms.
Door commitment is requested at start+500ms; discharge commitment at start+100ms.
Suppression start is preparation; only a committed completion discharges it.
Ground-truth occupancy constrains the irreversible commit, not preparation.
The fan's run contract uses health and pressure, not the original hazard reading.

The physical oracle consumes **only delivered observations** and concurrent action
reservations. Unknown/expired inputs represent uncertainty; this milestone does
not provide probabilistic uncertainty bounds. It checks fan pressure/damper
position, damper fan state, door egress/occupancy, and suppression occupancy.
Safety observations have at most a 1s horizon, further limited by their inputs.
Plant truth is used separately for release metrics and invariant checking.

The four monitored invariants are: no discharge with a zone-2 occupant; pressure
at least 50; no closing door across blocked egress; no fan HIGH with damper CLOSED.
Changing truth does not instantly update observations. Normal sensors sample every
500ms with a 2s TTL. Terminal `sensor` injections are deliberately capable of
misrepresenting truth; `fault` drop persists, while `sensor` is one-off.

Normal actuators apply commanded effects even for weak baselines. Local controller
responses remain correlated and can fail. The door completion controller reports
failure if the true egress path is blocked. These controllers are declared test
behaviours, not certifications of actual physical procedures.

## Policy variants

All variants share plans, sensor/fault schedules, plant transitions, result
correlation, duration constraints and outcome verification. Admission graphs vary;
B0 is unrestricted **admission**, not a removal of the shared execution harness.

| Variant | Admission and continuation |
| --- | --- |
| B0 | Start/commit depend on a stable internal valid token; no run monitoring |
| B1 | Static permission only; no physical or running checks |
| B2 | Conjunctive dependency policy: hazard requires all three sensors; start requirements remain running requirements; no alternative witness support |
| B3 | Every pending start/commit requires the entire configured critical sensor set; one Unknown/Invalid member blocks them all; no running monitor |
| B4 | Physical safety at initial start authorization only; no run contract or physical recheck for commitment |
| B5 | Threshold witnesses, separate start/run/commit/outcome obligations, fresh leases and controller policies |

B2 additionally requests a controller when a relevant dependency's status changes
during execution, including a change that leaves its aggregate root valid.
A same-status sensor refresh does not by itself cancel a running action. It still
invalidates an unused lease referencing the old version. This interpretation
allows healthy periodic sampling in S1; it is an explicit comparator default.
The static B1 permission is a policy input, not an implemented identity system.
B3 excludes expected-false initial facts such as `alarm_active` and
`suppression_armed` from its global critical set.

Every action writes its own named resource. Joint scheduling additionally excludes
fan/damper overlap and duplicate action classes. `supported_subset` chooses a
stable inclusion-maximal compatible subset in lexical order, with active actions
reserved. It does **not** solve a maximum-cardinality or weighted scheduling
problem. Tests enumerate all 128 subsets of the seven built-in action classes,
check pairwise feasibility, and check that no excluded candidate can be added.
Ordering, phase support, and binding exclusivity are still enforced at dispatch.
This is bounded verification for the built-in model, not a theorem for arbitrary
resource graphs or arbitrary planners.

## Time, ordering and reactions

The event queue key is `(virtual_time, priority, monotonically increasing ID)`.
The outer loop also visits `ActionRuntime::next_deadline`, so a large clock jump
cannot delay a runtime expiry or timeout response. At each boundary:

1. The assurance/action runtime processes due expiry and lifecycle reactions.
2. Plant changes and fault starts precede observation deliveries.
3. Observation sampling/delivery and wakeups precede actuator events.
4. Actuator events precede commit/replan events; generated earlier-priority events
   at the same time are processed before further admission.
5. New admission sees the completed timestamp's updates and current reservations.

Expiry wins over a refresh arriving at exactly the same timestamp. This is a
conservative explicit default; the action may already be recovering when renewed
support arrives. Within an evidence-delivery batch, each ordered observation can
trigger running assurance; the simulator does not erase intervening support loss.
No real sleeps or host-clock values influence semantic decisions.

C13/C14 explicitly model support loss at 500ms, detection after 50ms, propagation
after 20ms, controller scheduling after 10ms, and actuation after 100ms or 650ms.
Total responses are therefore 180ms and 730ms. The margin is 500ms, and equality
counts as a miss. A deadline event records failure even if no controller responds.
Other controller requests use zero injected detection/propagation delay, 10ms
scheduling and 100ms actuation. Completion-safe controllers finish no earlier than
the original action duration and have an illustrative 3s response margin. Other
controllers have a 500ms margin. Records are correlated to action instances.
An unsuccessful physical controller result cannot count as a successful response.

## Metrics and reproducibility

Rates export numerator, denominator and value; zero denominators yield JSON null.
The definitions used by this milestone are:

| Metric | Definition |
| --- | --- |
| UnsafeActionReleaseRate | Start/commit releases violating ground-truth physical/joint checks divided by all start/commit releases |
| FalseSafeAuthorizationRate | Releases not supported by the independent ground-truth phase requirements, including physical/joint checks, divided by all releases |
| UsefulActionRetention | Distinct ground-truth-supported start opportunities admitted during this run divided by distinct supported opportunities encountered after predecessor completion |
| UnnecessaryRevocationRate | Unused-lease revocations or controller requests while the corresponding truth requirements remain satisfied, divided by those revocation/request events |
| WitnessSwitchCount | Existing witness changes its evidence set or justification; horizon-only refreshes excluded |
| LeaseRenewalCount | Additional issued lease for the same instance and phase |
| ReplanCount | Successful suffix replacements |
| FallbackCount | Controller commands delivered to the fake actuator |
| DependencyPropagationLatency | Measured evaluator host elapsed time, excluding diagnostic reachability and lifecycle reactions |
| ExecutionRevalidationLatency | Measured host elapsed time for lease request plus local dispatch |
| SafetyReactionLatency | Per-instance virtual detection, propagation, scheduling and actuation durations |
| ReactionDeadlineMisses | Recorded response deadlines missed or unsuccessfully handled |
| TaskCompletionRate | One experiment: 1/1 if every instance in the current plan completed, otherwise 0/1 |
| ActionCompletionRate | Additional diagnostic: completed current-plan instances divided by current-plan instances |

UsefulActionRetention is an opportunity measure within the observation window,
not a proof of optimal utility. Truth-avoidable revocations may still be justified
by missing evidence; the metric exposes that distinction. For fan controller
requests, truth support uses the run obligation (health/pressure), not start-only
hazard evidence. Superseded unstarted instances are excluded from the current-plan
completion denominator, but retained in the history.

Optional `AssuranceRuntime` performance instrumentation records each evaluator
invocation, including expiry batches and internally produced actuator observations:
N nodes, M rules, M_e structurally reachable rules, actual nodes/rules recomputed,
and host elapsed nanoseconds. The workload CSV preserves these records. Initial
graph construction is excluded. Instrumentation is off by default outside the
simulator, and its host timings are excluded from semantic comparisons.

The manifest records the commit, dirty status, compile-time source fingerprint,
configuration/graph fingerprints, scenario, seed, current version, host Rust
version, preset fault schedule, parameters and journal fingerprint. Fingerprints
use FNV-1a for reproducibility diagnostics; they provide no cryptographic integrity.
The source fingerprint covers model/runtime sources and Cargo configuration.
Untracked edits are represented by the compiled source fingerprint and dirty
status, not merely the last Git commit.

Seeded packet loss uses an explicitly deterministic xorshift generator; S5 drops
one in five draws after 500ms. Equal seed/configuration/commands with the same code
reproduce decisions, witnesses, leases, command history and final state. Host
measurements, manifest creation timestamps and local report paths are excluded.
Terminal mutations enter the replay journal; inspection and file-saving commands
do not. Output directories must be new to avoid overwriting experiment evidence.

## Verification and completion boundary

`tests/week_four.rs` covers C11–C16, S1–S5 across all six baselines, full/incremental
semantic equality, replay, missing acknowledgements, physical/observed separation,
invalid suffix rollback, workload inclusion, and jump-versus-step timing.
`scripts/verify_week_four.py` runs the entire Rust suite, format/lint checks,
batch/replay/interactive CLI smoke tests, and parses the resulting JSON/CSV.
Its requirement rows report actual named test outcomes; missing tests fail the gate.
It supplements the Section 75 rows with a local-completion-controller row.

Week 4 is a local discrete simulation milestone. Broad fault distributions,
distributed transports, real-time scheduling guarantees, real device validation,
calibrated building physics, authentication, post-quantum protocols and an AI
planner are outside this implementation. No simulation result is a physical
safety certification or a claim that novelty has been established.
