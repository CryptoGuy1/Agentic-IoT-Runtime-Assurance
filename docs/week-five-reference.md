# Week 5 technical reference

Scope: the six-week timeline's communication-fault and comparative-pilot milestone,
Implementation Specification v1 sections 38–44, 65–69, 71–74, 76 and 79–83.
The existing Section 75 gate passed before this extension. The pilot reruns it
against the extended implementation. No LLM, container network, MQTT service,
physical device or post-quantum protocol is introduced.

## Transport boundaries

`simulation/network.rs` defines a deterministic transport model. `engine.rs`
connects it at four distinct boundaries:

1. Source observations to the evidence runtime.
2. Requests carrying issued leases to the local execution gateway.
3. Queued commands to the fake actuator.
4. Correlated actuator results back to the action runtime.

`Link::Cloud` distinguishes agent heartbeats from local sensor traffic. The
partition profile cuts cloud and authority traffic, leaving sensor/actuator/result
links available. A custom configuration can partition all links. A packet is
lost if its link is partitioned either at send or at scheduled receipt. There is
no implicit queue across a partition and no transport retry policy. Later periodic
sensor samples and explicitly renewed leases provide new attempts.

Source versions are allocated when an observation is created, before transport.
Its source identity, observed time, expiry and payload remain unchanged in every
copy. Receipt rejects versions less than or equal to the current accepted version.
An already-expired late observation remains Unknown. Invalid evidence retains
Week 1's explicit semantics; invalid observations are not silently expired into
valid evidence.

Lease issuance and dispatch are separate events. Only one authority request per
instance/phase is pending at a time. Arrival calls the existing `dispatch` method,
which checks the recorded versions, plan version, lifecycle, contract, nonce,
expiry and single-use status. Dropped requests eventually expire; old copies cannot
remove a newer pending request. No network profile disables gateway checks.

The fake actuator uses command IDs for idempotence and recognizes obsolete commands
relative to commands it has actually received for that instance. It does not read
a runtime cancellation as immediate device knowledge. Thus a previously dispatched
start may physically arrive before its delayed abort. Local finish events are
superseded only after the newer command reaches the device. Late responses then
encounter the runtime's normal correlation/sequence checks. This is a declared
software adapter protocol, not authentication or hardware replay protection.

`take_command` means the command left the runtime's local queue; `Execute` means it
reached the simulated actuator. Normal action duration starts on physical receipt.
Runtime maximum durations still start at local dispatch, so communication can use
up the available execution budget. Physical completion and confirmed completion
can diverge if responses are lost.

## Deterministic faults

All durations below are simulated milliseconds. Delay/jitter/reordering apply to
all transported message types; authority and actuator extra delays apply only to
the indicated link. None of these values represent measurements of a real network.

| Profile | Base delay | Jitter | Loss / 1000 | Duplicate / 1000 | Selected extra reorder delay | Extra authority delay | Extra actuator delay | Partition [start,end) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| none | 0 | 0 | 0 | 0 | 0 | 0 | 0 | none |
| delay | 40 | 0 | 0 | 0 | 0 | 80 | 0 | none |
| loss | 0 | 0 | 200 | 0 | 0 | 0 | 0 | none |
| duplicate | 0 | 0 | 0 | 1000 | 0 | 0 | 0 | none |
| reorder | 0 | 0 | 0 | 0 | 750 | 0 | 0 | none |
| partition | 0 | 0 | 0 | 0 | 0 | 0 | 0 | [500,3500) |
| compound | 20 | 0–80 | 100 | 200 | 650 | 50 | 0 | [1200,2200) |
| stale-lease | 0 | 0 | 0 | 0 | 0 | 1100 | 0 | none |
| slow-controller | 0 | 0 | 0 | 0 | 0 | 0 | 600 | none |

Duplicate copies arrive 5ms apart. A keyed deterministic hash of seed, link,
message key and independent draw label decides loss, jitter, duplication and
reordering. Reordering selects approximately half of the keys. These are seeded
fault fixtures, not statistically validated channel models or cryptographic RNGs.

Exogenous sensor keys include sensor identity and sample time. They do not use a
global random counter that could shift when a policy produces more commands.
Command/result keys use command identity and result sequence. Repeated same-sensor
sends at the same simulated time share a fault draw. This declared correlation is
kept in the manifest/source version and transport trace.

The Week 4 S5 sensor-drop fixture retains its existing deterministic schedule.
The new transport faults are an additional independent experimental factor.
The S4 scenario's persistent agent failure is distinct from the recoverable
partition network profile.

Physics observations are cached against their own observed inputs and active
reservations. An unrelated event does not manufacture a new physical-safety
observation version just because virtual time moved. Expiry or changed relevant
inputs still triggers reevaluation.

## Matched comparisons and feature removals

B0–B5 retain the same common execution/result-correlation harness. In particular,
all share protocol-level stale-authority rejection; B0 means unrestricted admission
requirements, not a deliberately broken transport protocol.

The default pilot uses two seeds, five scenarios, three profiles and all six
policies: 180 matched runs. Additional targeted fault runs, ablations and controls
bring the total to 210. Initial plans, source models, exogenous observations,
random fault schedules, durations and controller behaviour are matched. The script
checks exogenous transport traces for equality across policies. Endogenous state
and actuator-caused observations necessarily differ when policies choose different
actions; claiming identical entire closed-loop traces would be incorrect.

Four explicit B5 feature removals are available through `--ablation`:

| Removal | Exact change | Interpretation |
| --- | --- | --- |
| no-alternatives | Hazard threshold changes from 2-of-3 to 3-of-3 | Removes redundant threshold support; it is not a general removal of every possible alternative rule |
| no-run | Action definitions omit run-contract monitoring | Tests continuous running assurance |
| no-commit | Commit roots depend on the internal valid token | Removes commit evidence checks while preserving the explicit commit lifecycle |
| no-physical | Physical roots depend on the internal valid token and the B5 joint subset filter is bypassed | Tests physical/joint admission in this model |

These are named experimental graph/scheduler variants. They do not introduce an
unsafe bypass API into `ActionRuntime`. Ablations cannot be silently combined
with another baseline. Existing outcome correlation remains enabled in all these
variants; an outcome-removal ablation is not implemented in this milestone.

Both pre-start and post-start sensor loss are included for redundancy. The
post-start case can be neutral because the fan's run contract excludes hazard.
The report preserves neutral and adverse baseline comparisons instead of selecting
only successful demonstrations.

## Timing and scale

Existing Section 71 metrics and Section 72 workload records remain available.
`ExecutionRevalidationLatency` now measures local dispatch only; separate
`LeaseIssuanceLatency` records request processing. Their separation matters because
virtual communication occurs between them. Neither includes simulated transit.
`DependencyPropagationLatency` remains measured evaluator host elapsed time,
excluding diagnostic traversal, logging and lifecycle reactions.

C13/C14 retain their explicit support-loss origin and four virtual reaction stages.
Outbound command transit adds to the scheduling stage before actuator receipt.
For ordinary recovery requests, the existing reaction origin is when the runtime
observes the unsupported state or timeout; zero injected detection/propagation
latency does not imply immediate detection of a real-world sensor/network fault.
Transport timestamps and evidence expiry remain separately inspectable. A packet
drop does not instantly invalidate still-live earlier evidence.

`scale` builds independent graph groups with three evidence leaves, a threshold
hazard, a start root and a run root, plus a shared policy leaf. It varies groups
8, 64 and 256 (49, 385 and 1537 nodes), with 100 timed updates per size and
intermediate expiry batches. Local changes alternate with high-fan-out policy
changes. Full and incremental results and semantic audit histories are compared
after every operation. Initialization is deliberately excluded from the samples.
This graph-size sweep is separate from the fixed-size closed-loop building pilot.

The report gives p50/p95/p99/max evaluator elapsed times. Its declared feasibility
screen is less than 50ms—10% of the illustrative 500ms response margin—for the
largest scale evaluation and the largest sum of a run's separate propagation,
lease issuance and dispatch maxima. The latter is a conservative sum of observed
operation maxima, not a directly measured end-to-end transaction. It is not a
hard real-time bound, process-scheduling benchmark, memory scalability claim or
physical safety certification. It is legitimate for virtual deadlines to fail
while this local host-cost screen passes.

## Reports and replay

Each run adds `network.csv` and `network-metrics.json` to Week 4's artifact set.
They record attempts, scheduled arrival copies, send losses, receipt partition
losses, observation/authority/result rejections and device duplicate handling.
The event log records application of individual deliveries. The manifest contains
the full transport configuration, ablation and embedded replay session. Replay
format version 2 also preserves evaluation mode; version 1 sessions remain readable.
The CLI can replay either a session file or a current-format manifest.

`--manifest` reads the replay field produced by this program; it is not a general
JSON experiment import API or an authenticated archive. Exact semantic replay
requires the same source. Fingerprints allow drift to be noticed but do not assert
cryptographic integrity. The pilot adds a SHA-256 source fingerprint, retains raw
manifests, and compares semantic output bytes for its four headline replays.

The pilot produces:

- A Section 76 report tied to actual named test outcomes.
- A complete per-run CSV with all Section 71 metrics; raw manifests/logs/workload
  records remain under each run directory.
- All matched baseline deltas, including ties and lower-completion/higher-false-safe
  comparisons for B5. A lower false-safe rate must be read with release counts.
- Release-build scale measurements and exact-reference agreement.
- A bounded Section 83 acceptance assessment and explicit limitations.

Zero-release rates remain null in raw metrics. For pairwise descriptive deltas,
the report uses zero as the numerical rate when no release occurred, while
retaining the counts and requiring at least one completed B5 action before such
a comparison can count as behavioral distinction. There are only two default
seeds; no statistical significance or universal superiority is inferred.

## What remains for Week 6 and later

The planner remains scripted. Week 6 introduces the agreed Python AI adapter,
structured proposal validation, recorded model inputs/outputs and the adversarial
metadata scenario only after the preceding gates pass. Separate processes,
containers, network sockets, calibrated building dynamics, real sensors, secure
transport and hardware controllers remain follow-on engineering.
