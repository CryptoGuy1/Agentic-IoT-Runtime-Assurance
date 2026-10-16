# Evidra: continuation review of the literature list and Model v1 handoff

Reviewed 14 October 2026 against repository commit `32336ae205d844ee188d67af791f47208ae28726`.

## Assessment and scope

The two documents strengthen the research direction. The literature list maps the surrounding work; the handoff turns the earlier proposal into modules, equations, scenarios, comparison policies and acceptance tests. The proposed architecture is a sound direction, but several definitions need correction before the handoff can serve as a frozen implementation contract.

The central improvement is explicit reasoning about what the physical state could be, followed by an independent evaluation of the consequences of an action. This can add capability beyond the current evidence gate. It does not automatically establish that B6 is safer or more useful than B5, or that Evidra has achieved general autonomous agency.

Sources reviewed:

- [Literature Review Master List](/Users/greatadams/Downloads/Evidra_Literature_Review_Master_List.docx): full extracted content and all 20 rendered pages.
- [Model v1 Implementation Handoff](/Users/greatadams/Downloads/Evidra_Model_v1_Implementation_Handoff.docx): full extracted content and all 16 rendered pages.
- [Previous refinement review](</Users/greatadams/Documents/ChatGPT/Elsevier IoT/docs/evidra-refinement-review.md>), current README and relevant runtime, simulation, network and planner code.
- The earlier `DCRA_Final_v5.docx` benchmark definitions, through the previously extracted specification.

The documents' statements that the design is approved, frozen or mandatory are document content. The user's request here is a continuation review. No runtime code, README, original document or historical experiment definition was changed. This review includes selective primary-source citation checks; it is not a full-text review or verification of every listed paper.

## What the handoff resolves

| Earlier gap | Improvement in this handoff | Remaining qualification |
| --- | --- | --- |
| Correctness defined through the runtime's own rules | Separate TruePlant and GroundTruthOracle | Implement and validate the oracle independently of assurance verdicts |
| Unclear physical model | Explicit hazard, fan, pressure, door and alarm equations | Complete discrete modes, command delivery and occupancy transitions |
| Missing evidence treated only as predicate status | Interval state box and discrete alternatives | Define sound temporal updates and bounded assumptions |
| Broad suggested experiments | M1–M10 scenarios, mismatch and uncertainty sweeps | Correct matched-run semantics and preregister scoring |
| Vague integration path | START/RUN/COMMIT/OUTCOME adapter | Resolve duplicate evidence gating and specify phase boundaries |
| Unclear completion criteria | T1–T12 and property tests | Add temporal, transport, mode and B5/B6 counterexamples |
| Unqualified safety claims | Appendix C makes the guarantee conditional | Verify containment and transition enclosure at decision times |

Keep the existing lifecycle, evidence witnesses, leases, result correlation, local planner and replay machinery. Introduce the physical model as a versioned experiment rather than replacing the existing simulator and relabelling its results.

## Corrections needed before implementation

### 1. Apply a measurement at its generation time

Sections 10–11 retain generation and receipt timestamps, but the pseudocode intersects a delivered measurement directly with the current predicted state whenever it is within its maximum age. A measurement that has not expired can still describe an earlier state.

Concrete counterexample using the handoff's nominal fan equation: the fan is stopped at time 0, its observation is `0 ± 0.03`, and a start command takes effect immediately. Two 0.2-second steps later the true fan fraction is approximately `0.248889`. If that original reading arrives at 0.4 seconds, it is still within the one-second age limit. Directly intersecting its `[0, 0.03]` range with the current estimate can reject a perfectly consistent history or exclude the true current state.

Specify either timestamped history correction followed by forward propagation, or a sound enclosure that transports the measurement constraint from its generation time to the present. Define deterministic processing for duplicates, reordered observations and source versions. TTL is an information-age rule; it does not make a historical measurement instantaneous.

Also define future timestamps, unavailable error bounds, range validation and clock uncertainty. Use integer simulation timestamps consistent with the existing `Duration` clock rather than relying on floating-point timestamp equality.

### 2. Model command application uncertainty

Section 11 predicts from dispatched commands; Section 16 advances the plant using the dispatched command. The existing simulator distinguishes command transmission, device receipt, execution and feedback. Those distinctions must survive the extension.

When a start command is delayed or lost, Evidra cannot assume the fan has already begun accelerating. Conversely, an in-flight start can arrive after a cancellation request. Until the relevant information arrives, the estimate must cover the possible actuator inputs and application times. A received-command acknowledgement and a measured physical outcome also have different meanings.

Maintain pending/applied command hypotheses, command identity, order and bounded transport assumptions. TruePlant changes its input at the device's simulated application event. Estimator knowledge changes only through information legitimately available to the policy. Test delayed start, lost start, delayed stop, reordered commands and lost acknowledgements.

### 3. Give occupancy a transition model

Section 11.2 keeps occupancy at `{CLEAR}` while a trusted CLEAR reading is fresh. But an exact reading only establishes occupancy when sampled. Section 9 allows scenario-controlled occupancy changes without a declared restriction on when someone can enter.

An occupant can therefore enter after a CLEAR sample and before its one-second TTL expires. Keeping `{CLEAR}` until expiry would exclude the true state. The four-second DoorClose reachability horizon exposes the same problem: current clearance alone does not establish future clearance throughout closing.

Choose and declare an occupancy/interlock assumption, or include both occupancy possibilities whenever entry is physically possible. If entry is unrestricted and there is no protective mechanism, robust autonomous closure may remain UNKNOWN. That is an honest result. A rechecking policy needs a bound on sensing, delivery, detection and physical stopping, plus a model of how it protects the remaining closing trajectory.

M3 tests loss followed by an occupancy change after the prior evidence has aged. Add a case with occupancy changing inside the freshness window and another during door motion.

### 4. Include every relevant uncertainty in the represented transitions

TrueState contains `fan_failed` and `door_jammed`, but StateBox has no corresponding mode alternatives. The estimator's knowledge of the hazard source `s[k]` is also unspecified. Its bounds must cover these effects through explicit discrete hypotheses or demonstrably sufficient transition bounds.

The nominal fan disturbance of `±0.02/s` cannot by itself cover the failed-fan equation's decay of `1.25/s`. Assuming health until failure feedback arrives can break containment before the next observation. Include fault-onset assumptions and hidden source changes without giving the estimator the benchmark's true mode flags.

Specify initial state-set construction, parameter uncertainty, persistent actuator inputs, alarm actuation delay, and transitions between modes. Independent true parameters and estimator parameters are valuable, but nominal containment already requires complete input and mode assumptions.

### 5. Resolve what the model replaces and what it supplements

Section 15 defines `existing_evidence AND ModelSafe`. Under that formula, B6 cannot authorize a phase when B5's corresponding existing evidence expression is UNKNOWN or INVALID, regardless of the model result.

That is a defensible conservative extension, and it can prevent releases that B5 would allow. However, it cannot recover an action blocked by the same expired sensor leaf. The earlier example of continuing from a sound propagated estimate after a short freshness limit would be masked by that unchanged leaf.

Declare the intended capability:

- If B6 only adds a check, retain the AND formula and frame availability claims primarily against global fail-closed behavior.
- If B6 should substitute a sound physical estimate for certain direct measurement predicates, define a versioned physical claim producer and its certificate semantics. Retain permissions, identity, planner/version checks and genuine outcome requirements. Avoid requiring both the obsolete direct predicate and its proposed replacement.

Keep B5 unchanged as the reference. Add a controlled B5/B6 difference test, in addition to the proposed B3/B6 and B4/B6 tests. Include cases where B6 ties B5 and where it blocks more.

### 6. Separate safety, usefulness, physical success and confirmed outcome

Section 14.1 lists physical success and alarm-response objectives under safety invariants. The scorer's `safe` predicate and the subsequent usefulness label could therefore conflate safe failure to achieve a goal with an unsafe trajectory.

Record separate components: invariant violation, objective usefulness, achieved physical outcome, deadline satisfaction and justified outcome confirmation. Decide explicitly which deadlines are hard safety obligations and which are task requirements. A jammed door can fail to close without creating the same hazard as a moving door striking an occupant.

Define FanStop's numeric stopping threshold and deadline, DoorOpen's physical safety conditions, hazard hysteresis and each scenario's utility rule. Explain whether occupancy at a closing command with no actual motion is a forbidden command or a physical motion violation. Define COMMIT separately from successful outcome, or declare why a particular action has no distinct physical commit boundary.

UNKNOWN observation quality also needs an explicit branch. The `CONTRADICTED` branch currently emits an inconsistency event and continues, potentially leaving a valid-looking result. Specify whether inconsistency is latched, which claims it affects, and what evidence or reset procedure clears it. An accurately observed false predicate must not be confused with a faulty or contradictory sensor.

### 7. Define the trajectory guarantee and the safe response

Section 12 should require checking the initial state and all represented intermediate states, not only the terminal reachable box. Define the candidate's control schedule, concurrent commands, remaining action duration and response to future evidence changes.

A fixed look-ahead does not guarantee indefinite safe continuation. The proposed nominal pressure equation has equilibrium `0.55/0.60 ≈ 0.916667` when fan and door fractions are both 1, above the specified `0.80` limit. Starting at pressure 0 with those fractions fixed, the specified discrete update gives approximately `0.798108` at 3.2 seconds and `0.812335` at 3.4 seconds.

This calculation is a check of the supplied equations, not a complete scenario run. It shows that eventual unsafe pressure is possible even without model mismatch. Do not silently tune the model to make a preferred policy win. Specify continued monitoring and a feasible recovery, or a terminal condition that supports safe continuation beyond the checked horizon.

Stopping the fan is not automatically the safe response if an active hazard requires ventilation. Recovery may involve opening the door, another protective action or escalation. Evaluate the recovery trajectory and distinguish an abort request from an applied and confirmed physical stop.

### 8. Make the time and numerical semantics executable

The nominal 0.2-second tick does not exactly align with the 0.5-second sensor periods. Define exact event scheduling: for example, event-aware substeps no longer than 0.2 seconds, with an explicit final partial step. Alternatively, change the declared discrete timing grid consistently and version it.

State whether the equations define the benchmark's discrete plant or approximate continuous dynamics. Checking grid states establishes only the stated discrete guarantee unless intermediate continuous evolution is also bounded.

Conservative mathematical interval formulas implemented with ordinary `f64` arithmetic still need control of rounding error. Specify outward rounding or another justified numerical enclosure, especially near thresholds. This is a standard interval-analysis requirement, discussed in [SIAM's Introduction to Interval Analysis](https://epubs.siam.org/doi/pdf/10.1137/1.9780898717716.fm).

Revise T4: without measurements, uncertainty must evolve according to the declared model; interval width need not increase every tick. Stable dynamics, physical-domain clipping and existing broad uncertainty can produce contraction or saturation despite nonzero disturbances.

## Benchmark corrections

### Matched stimuli and policy-dependent outcomes

Scientific invariant 8 requires identical true trajectories and observations across policies. That cannot generally hold in a closed-loop physical experiment: differing commands change fan/door states, pressure, hazard and subsequent observations. The earlier final benchmark already explicitly allowed this divergence.

Use two lanes with separate interpretations:

1. Recorded evidence replay for comparing decisions on identical inputs.
2. Closed-loop runs with matched initial conditions, exogenous events, source schedules, noise/fault processes, candidate proposals and termination rules, allowing endogenous trajectories to diverge.

Key noise and fault draws by stable source/event identifiers so divergent event counts do not accidentally change the external stimulus. Counterfactual oracle evaluation must not consume or mutate the live run's random stream or plant. It also needs a defined treatment of future exogenous events and disturbance traces.

### Version policies and metric names

The B0–B6 names in this handoff are a new schema. In particular, B3 has had several incompatible meanings:

| Document or implementation | Meaning of B3 |
| --- | --- |
| Current pilot | Global critical-sensor gate for pending admission |
| Earlier final benchmark v2 | Direct/flat dependency monitor |
| Previous research refinement | One-time model gate |
| New Model v1 handoff | Point-estimate model |

Use explicit schema/version identifiers in configurations, manifests, plots and reports. Define whether the point-estimate comparator has the same phase checks and horizons as B6 so the comparison isolates uncertainty representation. Keep a separate one-time-versus-continuous ablation. Specify B4's response to degradation after an action has started; its current pending-only definition is incomplete as a safety comparator.

URR now means Unsafe Release Rate, while the earlier benchmark used it for Unnecessary Revocation Rate. Store unambiguous metric keys rather than reusing the acronym in merged results.

### Complete scoring and mismatch definitions

Freeze the unit of scoring: candidate request, authorization attempt, action instance or phase. Define repeated proposals, wait, rejection, timeout, cancellation and end-of-run pending work. Preserve raw counts and denominators; report an undefined rate when its denominator is zero. Score each candidate against the true state at the specified boundary without feeding that label back into the policy.

A truth-safe action blocked under inadequate information is a missed physical opportunity, but may be the justified decision. UBR alone does not distinguish those cases. Retain an independently specified information-sufficiency assessment or report that limitation explicitly. Preserve the README's 80% independently scored decision-correctness milestone with its own declared scope and uncertainty reporting.

Report containment at authorization/revalidation boundaries and over checked trajectories, alongside average SSCR. A high average coverage rate can hide a failure at the exact moment an unsafe action is released. Distinguish assumption-respecting runs from deliberately out-of-model runs; failures in the latter quantify robustness rather than disprove a conditional guarantee under the former.

Positive-only mismatch scales miss important adverse directions, such as slower door motion or weaker hazard removal. Include signed perturbations with physically valid parameter ranges and explicit mode/timing mismatch. Thresholds may be tuned during a documented pilot, but freeze the confirmatory configuration before collecting headline results.

## Literature review: useful map, unfinished bibliography

The document has 123 numbered reading entries: 67 Priority A, 39 Priority B and 17 Priority C. These are not 123 distinct verified papers. Seventeen entries explicitly identify themselves as cross-references, several grouped entries refer to multiple works, and additional papers recur as complete citations. Examples include LLM-Planner (31/47), CRYSTAL (77/92), the digital-twins-as-a-service paper (78/91), and GUIDE2FR (96/104).

Its strongest feature is the connection between literature themes and specific Evidra questions. Preserve that structure, but create one canonical record per work, tagged with multiple themes. Distinguish bibliographic verification from reading depth and from reproduction of a reported result.

Selective primary-source checks found:

| Entry | Finding and correction |
| --- | --- |
| ShieldAgent, 3 | Title and ICML 2025/PMLR 267:8313–8344 metadata agree with the [publisher record](https://proceedings.mlr.press/v267/chen25ae.html) |
| AgentSpec, 6 | Replace vague conference wording with ICSE 2026, pages 2938–2950, DOI `10.1145/3744916.3764546`, as listed in the [primary record](https://arxiv.org/abs/2503.18666) |
| PlanFence, 8 | The current v2 title is *Fresh Memory, Stale Plans: Derivation Currency for Distributed LLM-Agent Memory*. Update the citation or explicitly identify the older version being used. [Primary record](https://arxiv.org/abs/2609.03340) |
| Mission-Level Runtime Assurance, 9 | The listed authors do not match the current v3 record: Nikolaos Kekatos, Michael Ioannou, Panagiotis Katsaros, Alexios Lekidis, Theodoros Nestoridis, Tom Nianios and Dimitrios Nikou. Record the exact reviewed version. [Primary record](https://arxiv.org/abs/2607.23532) |
| Edge Intelligence, 33 | Citation text gives two titles and mixes venue information; resolve to a single exact publisher record before use |
| Surveys, 38/39 | Incomplete author/year/identifier information; treat as discovery leads until resolved |
| Simplex, 49 | “Sha et al. / Seto et al.” leaves authorship unresolved; replace with authoritative metadata |

Do not infer that an unresolved citation is fabricated. The list itself says it is a working discovery map. The corrections above support that interpretation, and prevent its peer-review labels from being treated as verified wholesale.

The new implementation particularly needs bounded-error/set-membership estimation, delayed-measurement updates, interval enclosure and hybrid fault-mode estimation foundations. Add or elevate those beside the existing Simplex, ModelPlex, safety-filter, partial-observability and freshness literature. The older work *Recursive State Estimation: Unknown but Bounded Errors and System Inputs* (DOI `10.1109/TAC.1968.1098790`) is a relevant discovery lead; its full text was not reviewed here. SIAM's interval-analysis text supplies a concrete numerical foundation.

Prioritize a focused comparison matrix over reading every item before development. For each closest work record the information available, physical model, uncertainty representation, freshness semantics, enforcement phases, outcome verification, trust assumptions, hardware evidence and limitations. Use `not established from reviewed material` where appropriate rather than treating an unmentioned feature as absent.

A defensible candidate contribution is the integration of distributed evidence and provenance, explicit physical-state uncertainty, phase-specific authorization and measured outcomes under communication faults. Neither interval estimation nor a runtime guardrail is new by itself. Whether this integration is sufficiently distinct remains a hypothesis until the closest papers are read in depth and controlled comparisons demonstrate its effects.

## Fit with the broader Evidra objectives

Qwen3 remains a proposal generator. Model v1 improves the runtime's basis for checking proposals and the benchmark's basis for evaluating them. Autonomous goal pursuit still requires an event-driven planning/replanning loop, progress assessment and bounded recovery. The current web adapter is manually triggered and preserves an alarm→fan→door chain; FanStop/DoorOpen and other new contracts need an explicit versioned planner mapping.

The current server holds its simulation lock during inference. That works for a paused virtual demonstration, but a physical runtime must continue observations, expiry and enforcement independently of slow model requests. Hardware I/O should follow software acceptance, as the handoff proposes.

| Objective | What Model v1 helps establish | Additional work |
| --- | --- | --- |
| Security | Restricted authority and no hidden-truth access | Source authentication, replay protection, compromised-source cases and actuator enforcement |
| Privacy | Local planning and explicit data channels | Data minimization, retention and access controls for observations and logs |
| Reliability | Coverage, timing, fault and outcome measurements | Feasible recovery, command uncertainty and real-time hardware behavior |
| Explainability | Plausible intervals, dependencies and failed contracts | Plain explanations tied to actual decision records rather than LLM narrative alone |
| Interoperability | Separate observation and command interfaces | Units, schemas, source clocks and tested device adapters |
| Software quality | Modular design, replay and acceptance tests | Correct numeric/timing semantics, regression protection and experiment versioning |
| Societal accountability | Traceable configured authority and escalation | Ownership of thresholds, operator roles, override rules and reviewable incidents |

One normalized building experiment can validate a first domain. It does not establish building-code compliance, complete security/privacy, or general applicability across IoT domains.

## Recommended next sequence

1. Produce a corrected handoff revision covering time, input/mode uncertainty, occupancy transitions, physical claim integration, scoring and schema versions. Keep the overall architecture.
2. Implement the isolated plant, observation channel and estimator first. Prove containment on declared bounded traces before wiring the model into authorization.
3. Add tests for delayed observations, unknown quality, contradiction recovery, within-TTL occupancy changes, command delay/reordering, hidden fault onset, numerical boundaries and safe recovery.
4. Validate oracle safety, utility, outcome and information labels independently. Check that counterfactual evaluation cannot mutate live simulation state.
5. Integrate B5/B6 with explicitly versioned contracts. Demonstrate the new inference capability and its conservative costs on controlled fixtures.
6. Run a documented pilot, then freeze the confirmatory benchmark and complete the closest-work comparison before headline claims.
7. Move accepted interfaces onto the Pi testbed, separating the fast assurance loop from planning inference.

The most useful immediate deliverable is the corrected implementation contract and its counterexamples. More bibliography entries or a more capable LLM will not resolve these model semantics.
