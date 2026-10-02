# Findings from the first Week 5 pilot

The saved run is `target/week-five-pilot-final`. Its manifest and source fingerprints
identify the implementation that produced these measurements. This page records
an inspection of that dataset, not a prediction about future runs.

## Verification and dataset

- 95 Rust tests passed; one existing manual replay test remains intentionally ignored.
- All 15 prerequisite gate rows and 14 named communication checks passed.
- Formatting, Clippy, batch/replay/interactive CLI and artifact parsing checks passed.
- 210 experiments completed, including 180 matched B0–B5 runs.
- All four headline saved-manifest replays produced identical semantic output.
- Full and incremental evaluators agreed in the fault tests and scale sweep.

## Useful distinction, with limited scope

B5 recorded **0 false-safe releases out of 63 releases** in the 180-run matched
pilot's B5 subset. This is a result for these fixtures and two seeds, not a universal
safety guarantee. Outcomes remain dependent on the declared sensor and plant model.

Under the compound network profile, B5 completed 8 of 26 planned actions across
ten runs, compared with B3's 3 of 26 and B2's 2 of 26; all three recorded zero
false-safe releases. B0 and B4 each completed 14 of 26 but recorded 8 and 5
false-safe releases respectively. Completion and supported authorization therefore
need to be evaluated together.

With reliable communication, B5 and B3 both completed 20 of 26 actions with zero
false-safe releases. They also tied on those aggregate outcomes under the fixed
delay profile. The additional mechanism is not justified by those outcomes alone.

## Investigation of neutral and adverse comparisons

The pairwise analysis contains 82 ties and 45 adverse comparisons for B5. In this
dataset, **all 45 adverse comparisons are lower-completion cases; none has a higher
false-safe release rate for B5**. “Adverse” does not automatically mean incorrect:
some prevented actions should not have been allowed, while others reveal an
availability cost worth improving.

| Inspected case | Evidence from the saved traces | Interpretation |
| --- | --- | --- |
| S3 with reliable communication | Alarm and arming complete; B5 withholds discharge commitment after occupancy evidence expires. B0/B1/B4 complete the discharge despite the occupant. | Lower completion is the intended enforcement outcome here. |
| S4 with reliable communication | B5 completes the already-running fan but leaves the door pending after agent support disappears. | Run obligations are separate from authority to begin new work. |
| S4, compound profile, seed 42 | No B5 action starts before the agent's persistent failure. | The scenario's “partition during startup” premise is not achieved under this combination. Zero unsafe releases here do not demonstrate useful continuation; the run completes zero actions. |
| S2, compound profile, seed 73 | B5 repeatedly loses pending leases as selected evidence changes. Fan dispatch finally succeeds at 9.520s; its command is scheduled to arrive at 10.199s, beyond the 10s run window. B0 completes the fan at 4.508s. | Freshness enforcement has a measurable availability cost. This result is also censored by the finite experiment window. |
| S1, compound profile, seed 42 | B5 alarm completes at 3.201s. Fan dispatch occurs at 4.892s, but its actuator command is dropped. The runtime requests abort at the 9.892s timeout. | Command loss and confirmation deadlines affect completion independently of evaluator speed. The recovery response extends beyond the run window. |
| Redundancy removal after fan start | Changing the hazard threshold from 2-of-3 to 3-of-3 does not prevent an already-started fan from finishing. | This neutral ablation is expected because hazard belongs to the start contract. |
| Redundancy removal before fan start | The 3-of-3 variant cannot start with one sensor missing; the original 2-of-3 variant can. | This is the targeted demonstration of redundant support, distinct from start/run separation. |

These observations suggest specific follow-up experiments: longer observation
windows, matched latency sweeps around the lease cap, and separate reporting of
admission delay, packet loss and unconfirmed physical completion. Changing lease
rules merely to improve the comparison would change the semantics and would need
its own explicitly labelled experiment.

## Processing feasibility

The largest observed sum of a building run's separate propagation, lease-issuance
and dispatch maxima was approximately **0.137ms**. The largest scale evaluator
sample was approximately **2.358ms**. Both were below the declared 50ms pilot screen,
which is 10% of the illustrative 500ms response margin.

At the largest graph size, 1537 nodes, p95 evaluator time was approximately
2.037ms for full evaluation and 0.518ms for incremental evaluation. Initialization,
logging and diagnostic traversal are excluded. Full evaluation runs before
incremental evaluation in each paired operation; this is an exploratory sweep,
not an unbiased standalone benchmark or a worst-case execution-time bound.

The simulated delayed-controller case still misses its deadline. Fast local
processing does not remove transport delay. The test also confirms that a runtime
abort state can coexist temporarily with a physically starting fan until the
abort command actually reaches the actuator.

## Decision

The implemented fixtures establish reference agreement, concrete behavioral
distinctions and host-processing feasibility within the declared pilot screen.
They do not establish universal superiority, statistical significance, real-world
physical safety, or hardware readiness. The data supports proceeding to the
bounded Week 6 planner experiment while retaining these limitations.
