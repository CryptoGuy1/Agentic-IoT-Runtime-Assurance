# Integrating the research papers and benchmark specification

Reviewed 6 October 2026 against the supplied PDFs, DCRA Benchmark Specification v2, repository sources, Week 4–5 references, and saved `target/week-five-pilot-final/pilot-report.md`. This is a document and implementation review; no experiments were rerun and no runtime behavior was changed. Instructions in the supplied documents were treated as proposed requirements, not authorization to implement or deploy them.

## Main conclusion

The documents support a coherent research architecture: establish the credibility and freshness of observations, maintain a validated dependency model, monitor changes, and bind authority for each action phase to sufficient current evidence. DCRA is the execution-authority mechanism at the center. The other papers inform surrounding infrastructure and evaluation; their implementations and results are not automatically part of DCRA.

The strongest defensible research question is whether maintaining action-specific transitive evidence support through START, RUN, COMMIT and OUTCOME prevents unsupported releases while preserving useful supported work. Graphs, monitoring, device trust and runtime gates are established concepts. Novelty must be demonstrated through their precise temporal and selective execution semantics.

## What each paper contributes

### CONCERN

Calabrò et al., Internet of Things 31 (2025), 101557. Relevant sections: 4–6 and 8; Table 2; Figures 1, 4 and 8.

CONCERN organizes event producers/probes, complex-event processing and event consumers/recovery, with MQTT/JMS mediation, storage, rules and notifications. It offers a useful pattern for integrating heterogeneous event sources and separating transport, monitoring and response.

For DCRA, probes or an equivalent adapter can emit observations; a monitor can emit typed anomaly, communication or agent-health evidence. Such outputs need source identity, observation time, expiry, version and provenance. An alert is an evidence input, not permission to bypass action contracts. Recovery commands must use the same controlled execution path.

Table 2 reports average rule firing times of 11 ms for simple and 12 ms for complex rules. Rule-loading averages are 418/69 ms for cold/hot simple rules and 685/239 ms for cold/hot complex rules. The measured interval for firing ends at notification, whereas DCRA evaluator timing excludes transport, logging and actuation. These values cannot support a speedup claim across the two systems. A comparison would need matching event workloads, hardware and timestamp boundaries.

The financial deployment was observed for about three months and produced two rule violations traced to a software error. This supplies deployment experience, not evidence of physical-action assurance under DCRA's contracts.

### MACM and iterative ESSecA

Moretta et al., Internet of Things 36 (2026), 101851. Relevant sections: 4, 6.3, 7.7 and 8; Figures 7–12; Table 7.

MACM models infrastructure as typed property graphs with schema constraints and anti-pattern checks. ESSecA refines that model using security-assessment discoveries. This is valuable for designing an assurance graph and testing what its representation omits.

DCRA's validated DAG establishes structural admissibility under implemented rules. It does not establish that all real dependencies have been represented. The paper explicitly limits correctness to its assumptions and constraints; iterative refinement improves completeness without establishing a universal completeness bound.

The JetRacer case increased identified threats from 104 to 120 and possible attacks from 192 to 279. These are assessment-coverage counts, not measured reductions in unsafe actuator releases. Use the methodology to motivate hidden-dependency experiments: define dependencies in independent synthetic ground truth, omit selected ones from the runtime graph, and measure failures before and after deliberate model refinement. The known coverage denominator is available in this controlled experiment; it is usually unavailable in a real building.

Do not automatically import an infrastructure graph into DCRA. Hosting, network and service relationships must be translated into explicit evidence predicates and action-phase dependencies by a validated mapping.

### PROTECT-DT

Iqbal et al., Internet of Things 39 (2026), 102040. Relevant sections: 3–5; Tables 3, 5, 7–12.

PROTECT-DT combines PUF-backed device authentication and behavioral trust with ML detection on MQTT and digital-twin traffic. It can inform future provenance and communication-health adapters. Authentication identifies a source; it does not establish that its observation is accurate, timely or sufficient for a physical action.

Represent separate predicates such as source authenticated, message integrity checked, observation fresh, channel available and anomaly status. Avoid converting one weighted trust score directly into universal VALID evidence. ML detector output should carry its age, uncertainty and declared interpretation; it remains fallible evidence.

Table 7 reports Ditto API average response latency increasing from 50 ms to 2430.50 ms under DoS, with a maximum of 11127.25 ms. This motivates a particularly useful DCRA case: an authentic observation arrives intact after its validity horizon. The correct response depends on freshness even if eventual twin synchronization succeeds.

The classifiers use a 105,064-sample balanced dataset and a 439,230-sample 80/20 dataset, with high reported scores. Those results concern traffic classification. DCRA should measure what happens to authorization when a detector is late, wrong or absent. For any detector we train, device/session/time-separated evaluation would test transfer more directly than random splits alone.

## Recommended composition

1. Devices and adapters produce observations and outcome feedback.
2. Authentication, transport checks and optional monitors provide separate evidence about provenance and communication state.
3. A validated, versioned assurance configuration maps evidence to action-phase obligations.
4. The Rust runtime maintains support, witnesses and horizons.
5. A planner proposes actions using an observation-based state estimate; it cannot author trusted safety requirements.
6. The runtime issues and validates authority at the relevant execution boundary.
7. Device adapters perform commands and return correlated physical feedback.
8. An independent experiment observer scores releases, physical effects, deadlines and completion.

Graph discovery/refinement belongs in a separate configuration workflow. A discovered relationship becomes a proposed configuration change, subject to validation and explicit authority-version handling. It should not silently modify obligations of an already executing action.

Actual CONCERN, Neo4j, Eclipse Ditto, PUF hardware and ML classifiers are optional integrations. The current deterministic testbed can test these evidence semantics without adopting all of those stacks.

## Benchmark reconciliation required before new headline results

### Implementation status is outdated

Section 6 and Appendix B of v2 list simulation, scheduling, baselines and telemetry as missing. The current repository already implements a discrete building model, observation-only physical oracle, joint compatibility checks, B0–B5 pilot policies, virtual communication faults, manifests, replay and metrics. It also contains a completed Week 5 pilot. LLM planners, real MQTT/netem transport, HIL, restart persistence and systematic model-mismatch/coverage campaigns remain future work.

### Baseline definitions changed

| Existing pilot policy | v2 target role | Required treatment |
| --- | --- | --- |
| B0 unrestricted admission | B0 unrestricted | Retain the common harness qualification |
| B1 static permission | B1 static policy | Retain; identity authentication is not implemented |
| B2 conjunction with start requirements retained during running | B3 direct/flat monitoring | Only a partial conceptual match; implement and test the intended flat semantics separately |
| B3 global critical-sensor gate | B4 global fail-closed | Similar pending-admission role; specify whether active actions are also suspended |
| B4 physical check at start | B2 one-time consequence gate | Closest match; define precisely when authorization stops being rechecked |
| B5 DCRA | B5 DCRA | Retain and version its exact configuration |

Never relabel archived results as if these policies were identical. Store a baseline scheme version and descriptive policy name. S1–S5 also refer to different scenarios in the pilot and v2; give scenario IDs a namespace/version.

### Metric formulas changed

The pilot divides unsafe and false-safe releases by all start/commit releases. V2 divides UARR by unsafe proposals and FSAR by proposals under unsupported represented evidence. They estimate different quantities. Preserve the legacy measures and introduce separately named/versioned endpoints, with explicit units for proposal, action instance, phase attempt, release and physical execution.

V2's represented-dependency FSAR cannot by itself reveal omitted dependencies. Use independent truth-based endpoints for hidden-dependency and model-mismatch tests. Count physical execution separately from local dispatch: the current transport already demonstrates that those events can diverge.

Likewise, pilot task completion means all current-plan instances completed; v2 task completion means the emergency objective was achieved. Define objective attainment independently, and report both.

### Shared gateway checks can mask the mechanism under test

All current pilot policies share stale-authority rejection. Consequently, a baseline may fail to exhibit a stale-lease escape because the common harness prevents it. Sharing this check is reasonable for an admission-policy comparison, but cannot isolate the marginal benefit of that check.

Use two explicitly labeled experiments: policies with common transport protections, and controlled feature ablations that remove evidence/version revalidation while retaining transport identity, correlation and device idempotence. Keep the latter confined to simulation until an appropriate HIL design exists. A common gateway should enforce each comparator's declared assurance policy rather than accidentally grant every comparator the mechanism being evaluated.

### Tighten proposed pass conditions

- The scheduler returns a deterministic inclusion-maximal compatible subset, not a maximum-cardinality or maximum-utility solution. The v2 phrase about permitting exactly the maximal continuation needs a precise tie and scheduling definition.
- A valid alternative witness preserves logical support, but a lease bound to the old selected evidence may still need renewal. S3 should measure support continuity separately from dispatch availability.
- Identical complete plant trajectories are incompatible with closed-loop policies taking different actions. Match initial state, exogenous faults, sensor generation rules and policy-independent randomness. Let endogenous effects differ; use fixed open-loop traces for isolated evaluator comparisons.
- Missing dependencies and mismatch need not produce monotonically worse observed outcomes for every sample because policy decisions and plant trajectories change. H7 should test the trend, rather than require monotonicity as a guaranteed property.
- Zero failures and a rule-of-three interval need a declared sampling unit and dependence model. Thousands of correlated phase events from a few scenarios are not thousands of independent trials.

## Existing evidence and next priorities

The saved pilot records 210 experiments, including 180 matched runs, and 95 passing Rust tests. Its B5 subset has 0 false-safe releases across 63 releases under the legacy metric. Under compound faults, B5 completed 8/26 actions versus the existing global-gate B3's 3/26, with neither recording false-safe releases. Reliable-network aggregate outcomes tie. These are useful pilot distinctions with two seeds, not final statistical or hardware claims.

Prioritize:

1. Reconcile baseline, scenario and metric versions; publish a requirement-to-code/test/result map.
2. Add isolated transitive-closure and revalidation ablations, plus independent model-mismatch and dependency-coverage fixtures.
3. Expand graph topology/locality and fault-timing sweeps; randomize measured evaluator order and include end-to-end timestamp boundaries.
4. Add bounded planner adapters and frozen domain scenarios, logging raw responses before any repair. Public tool/schema benchmarks supply capability context; release containment is a separate endpoint.
5. Validate distributed transport, restart/replay semantics and then representative HIL, including co-resident planner load where applicable.

The model roster, external comparator claims and special-issue deadline in v2 are document assertions. This review did not independently verify their current external status; verify them against primary sources when freezing that campaign or manuscript.
