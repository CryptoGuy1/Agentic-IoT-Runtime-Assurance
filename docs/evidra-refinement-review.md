# Evidra refinement review and recommended research sequence

Reviewed on 13 October 2026 against repository commit `32336ae205d844ee188d67af791f47208ae28726`.

## Assessment

The supplied technical review identifies the right immediate scientific problem: Evidra needs independently defined experimental correctness. Its proposed evidence-to-state reasoning is a worthwhile extension, but it is a separate engineering and research contribution, not something obtained merely by adding an independent scorer. Preserve the existing runtime and pilot as a reference, validate the new scorer first, and test a small uncertainty-aware gate before committing to the full proposed B6 campaign.

The source is [the supplied technical review](/Users/greatadams/Downloads/Evidra_Technical_Review_and_Research_Refinement.docx), rendered and read across all 13 pages. Recommendations inside it are proposals, not implementation instructions or measured results. This review does not change runtime code, the README's 80% target, the original document, or historical benchmark definitions. It is an assessment and a proposed execution order.

The intended general assurance system remains the broader project goal. The proposed building study can validate a first domain-specific implementation. It cannot establish general applicability without further domains, or establish full autonomous agency while planning remains manually triggered.

## What the document gets right

The separation of experimental plant truth, imperfect observations, and action-effect scoring is essential. A source can be fresh yet wrong; a runtime can faithfully enforce its rules yet select an inappropriate action; and a command can be acknowledged without its intended effect occurring. A useful evaluation must expose those distinctions.

The document appropriately treats safety and useful operation as separate outcomes, keeps the planner outside the safety authority, proposes model-mismatch experiments, and retains outcome verification. Its recommendation to validate scientific definitions before final headline collection is sound. Routine hardware setup can proceed in parallel; experimental configuration should be frozen only after pilot validation, before confirmatory data collection.

The emphasis on hypothesis-labelled results is also appropriate. The qualitative ordering in Section 14 is a proposed pattern, not a ranking we should tune fixtures to achieve. Ties and adverse results must remain reportable outcomes.

## Repository audit

| Refinement | Current implementation | Actual remaining work |
| --- | --- | --- |
| Hidden plant versus observations | `BuildingState` and `StateEstimate` are already separate; observation delivery is scheduled independently | Make the boundary explicit in experimental interfaces and prevent scoring information entering decisions |
| Independent ground-truth scorer | `truth_support()` builds a truth-based estimate but calls the same `BuildingOracle` used for admission | Implement and validate a separate scorer using hidden state, action effects, trajectories and objectives |
| Imperfect observations | Virtual delay, loss, reordering, duplication, expiry and drop/restore already exist | Add controlled numeric measurement error, conflicting observations, common-source failures and declared trust assumptions |
| Plausible physical-state set | No state-set estimator; `StateEstimate` is a map of three-valued predicates and deadlines | Implement bounded state uncertainty and transition reasoning without hidden-truth access |
| Phase-aware assurance | START/RUN/COMMIT/OUTCOME, leases, recovery and result correlation already exist | Connect uncertainty certificates to the existing phases and test the added mechanism independently |
| Alternative support | Thresholds, witnesses and witness horizons already exist | Define when alternative observations legitimately narrow uncertainty; model correlated faults |
| Local AI planning | Ollama/Qwen3 integration, restricted sequential plans and saved proposals already exist | Add frozen-output replay and event-triggered, bounded planning outside the real-time assurance path |
| Comparison policies | Current repository B0–B5 exist | Register the proposed policies under a new schema; implement missing behavior without relabelling the pilot |
| Hardware validation | Pi 5/Pi 4 architecture and purchase plan are documented | Add physical I/O, distributed enforcement, real elapsed time, feedback and clock-uncertainty handling |
| General applicability | One building domain; web AI adapter accepts alarm/fan/door | Demonstrate a second domain or retain generalization as an explicitly unvalidated aim |

Important source locations:

- [Building state and observation oracle](../src/simulation/model.rs): `BuildingState`, `StateEstimate`, `BuildingOracle`, `physical_inputs()` and `joint_safe()`.
- [Simulation engine](../src/simulation/engine.rs): `update_physics()`, `truth_support()`, `dispatch_authority()`, `drain()` and `apply_planner_sequence()`.
- [Evidence semantics](../src/evidence.rs): `EvidenceAtom::value_at()`.
- [Policy configuration](../src/simulation/config.rs): repository B0–B5 and phase contracts.
- [Planner context export](../src/simulation/report.rs): `planner_context_json()` excludes hidden plant state and event details.
- [Web bridge and planner orchestration](../scripts/run_web_demo.py): `Bridge.plan()` holds the session lock throughout inference.
- [Qwen adapter](../scripts/ollama_planner.py): model, prompt, schema, timeout and generation settings.
- [Hardware plan](raspberry-pi-testbed.md), [README](../README.md), and [historical pilot findings](week-five-findings.md).

The pilot findings report 0 false-safe releases among 63 released attempts for the B5 subset of the 180 matched runs. That is historical, same-model preliminary evidence; the 210 total runs do not all contribute to that denominator. It is not an unsafe-proposal escape rate, a hardware measurement or a new independent-oracle result.

## Separate experimental truth from a justified decision

The proposed hidden-state oracle is necessary, but a truth-safe action is not automatically justified by the available information. If the hidden room is empty while occupancy evidence has disappeared, an ideal observer knows closure is physically safe. Evidra does not. Counting every such refusal as an incorrect decision would reward clairvoyance rather than appropriate handling of uncertainty.

Record at least these distinct labels:

1. **Truth safety:** Would the action or resulting trajectory violate the declared experimental invariant?
2. **Objective usefulness:** Does the action advance the declared objective, and is it necessary or redundant in the relevant window?
3. **Information sufficiency:** Does the trusted information available at that boundary justify the action under a predeclared reference policy?
4. **Runtime behavior:** Was the action proposed, accepted into a plan, authorized, dispatched, applied, stopped and confirmed?

Physical scoring should use hidden experimental state and independently implemented effect/invariant functions. An information-sufficiency assessor should use observations, declared error/dynamics bounds and an independently specified reference policy. Neither assessor should treat Evidra's verdict as the correct label.

Report a truth-based missed-opportunity cost separately from an unjustified block. A block can sacrifice a physically safe opportunity and still be the appropriate decision given missing information. The 80% milestone requires a declared mix of decision categories, eligibility windows, treatment of wait/repeated attempts and uncertainty estimates; these cannot be inferred from physical release rates alone.

Keep an additional distinction between environmental violations and violations attributable to action release. The current plant can have pressure below its minimum before any action occurs. Such an exogenous violation must not automatically be blamed on whichever method is being evaluated. Score release admissibility, avoidable harm and objective outcome separately.

## Define the proposed B6 mechanism precisely

Section 11 suggests accepting an action only if it is safe in every plausible state. Use a deterministic set-based estimator for the first extension rather than mixing that approach with uncalibrated probability/confidence scores.

Let the estimator maintain a nonempty set of states compatible with trusted observations and declared model bounds. Propagate observations from their source times to the current time, and propagate the current set across command delay and the next assurance boundary. Admission should cover possible trajectories over that interval, not only an instantaneous state.

The safety argument is conditional on three properties:

- The set contains the actual experimental state under the declared observation and dynamics assumptions.
- Transition bounds contain actual evolution and actuator behavior over the checked interval.
- The action and fallback checks correctly cover those possible trajectories.

Without those properties, a larger or more sophisticated model does not establish safety. Adding an independent scorer improves evaluation; it does not by itself improve the runtime's decisions.

### Evidence interpretation

For a Boolean occupancy observation, a missing current observation can mean `{empty, occupied}`. For a numeric proxy measurement, an error bound and elapsed-time dynamics can produce an interval. For redundant sources, narrowing that interval requires an explicit fault model, such as a bounded number of faulty sensors, not just counting votes.

`Invalid` in the current code means a predicate is contradicted; it does not mean the sensor is corrupt. A valid report that no hazard exists can contradict the hazard predicate while accurately describing a harmless situation. Measurement validity, source trust and predicate truth need separate fields or explicitly separate interpretations.

Current `EvidenceAtom::value_at()` expires positive support, but an `Invalid` value remains invalid after its recorded expiry. `StateEstimate::status()` similarly applies expiry to positive support. This is an intentional existing logical convention, not symmetric measurement freshness. A numeric/state estimator must decide how old negative observations lose physical relevance. Introduce that behavior through versioned observation/model semantics rather than silently changing the historical evidence core.

### Conflicting evidence and empty sets

Blindly intersecting inconsistent readings can produce an empty state set. A universal safety check over an empty set would be vacuously true. The new gate must explicitly reject that interpretation: an empty or inconsistent estimate is an assurance failure requiring observation repair, conservative recovery or escalation.

Other required cases are untrusted provenance, out-of-range readings, source-time skew, unknown error bounds, queue saturation and detected model inconsistency. Safe fallback must be action-specific: stopping an already committed physical process may itself be inappropriate.

### Concrete added-value example

Suppose a future proxy experiment has a safe lower bound of 50 model units. A trustworthy observation is 80 ±2 units and the independently declared maximum rate of decrease is 5 units per second. After 0.6 seconds, the lower bound is 75; it remains above 50. A narrowly defined freshness-only policy with a 0.5-second TTL may refuse a new action, while a sound time-propagated uncertainty model can still justify it for a suitably short future interval.

This is an illustrative proposed fixture, not measured physics or permission to reuse expired evidence arbitrarily. The uncertainty estimator must retain the measurement history under explicitly different semantics, cover communication/actuation delay, and test that the real experiment respects the rate bound. Increasing the baseline's TTL without changing its information assumptions might erase the difference; therefore TTL selection and the comparator's declared capabilities must be frozen and reported.

The opposite example is equally important. If an occupant can enter immediately after an empty-room observation, a fresh sample alone does not guarantee an empty room at closure. The state set should expand unless an interlock, another observation or a justified transition constraint rules out entry. B6 may block more actions than B5 in this case. That is a valid research outcome.

With only the same Boolean AND/threshold constraints as B5, enumerating possible states may reproduce B5's decisions. B6 needs a demonstrable additional inference capability; changing terminology is insufficient.

### Integration boundary

Reuse the existing graph evaluator, phase contracts, leases, dispatch checks and outcome correlation. A bounded model gate can produce action/phase certificates carrying source evidence versions, model/configuration revision, state-set revision, assumptions and validity horizon. Authority must be invalidated when a relevant certificate dependency changes.

Keep the current `BuildingOracle` as a named reference. Do not put the hidden truth oracle in the trusted runtime. Do not feed hidden truth into Qwen. For a small first experiment, enumerate only the variables needed by the candidate action and use bounded intervals for numeric variables; no larger model or high-fidelity simulator is required initially.

## Reconcile comparison policies before implementation

Section 12 defines B0–B6 differently from both the current repository and the earlier final benchmark specification. In particular, repository B2 is conjunctive support, B3 is a global gate for pending admission, and B4 is an initial physical check. The new document instead assigns B2 to freshness-only, B3 to a one-time model gate, and B4 to global fail-closed.

Keep historical policy IDs and manifests intact. Register the new proposal under an explicit new schema, for example `refinement-v1`, using descriptive names as well as numeric IDs. Do not use `B4` without its schema in new tables. The older paper-v2 direct/flat-dependency comparator can remain an auxiliary method if closure is still a claim to be evaluated.

Each proposed method needs exact rules for START, RUN, COMMIT, OUTCOME, required observations, timestamp handling, error bounds, fallback and timeout. In particular:

- **Freshness-only:** specify whether it checks only age/availability or also predicate values, and whether it rechecks during execution.
- **One-time model gate:** state whether it uses a point estimate or uncertainty set, which model it receives, and how it handles incomplete observations.
- **Global fail-closed:** define running-action behavior. Blocking only pending work cannot justify a claim that it safely handles evidence failure during execution.
- **Existing Evidra versus B6:** retain identical graph/lifecycle/transport semantics wherever possible; isolate the state-inference/consequence mechanism.
- **Outcome comparison:** all current baseline policies retain correlated outcome handling. RQ7 needs a separate declared outcome-verification ablation; the existing baseline matrix alone cannot isolate that benefit.

Use the same initial truth, candidate/proposal corpus, exogenous event schedule, fault parameters and random-stream keys. Different released actions produce different subsequent plant states. Requiring identical full hidden-state or observation traces despite different action effects would invalidate a closed-loop comparison. Frozen boundary cases and closed-loop objective experiments should be separate evaluation lanes.

No policy should read hidden state. Match the information budget and declare any richer dynamics or uncertainty knowledge given only to B6. Add ablations to identify whether gains come from that richer information, state propagation, phase monitoring or alternative witnesses.

## Repair metric definitions and statistical claims

Section 13 is a useful outline, but not yet an executable metric dictionary. Freeze the observation unit, boundary, eligibility, numerator, denominator, retry policy, time window and zero-denominator behavior for every rate.

| Endpoint | Recommended operational meaning |
| --- | --- |
| Unsafe proposal escape | Truth-unsafe eligible candidate phase attempts released / truth-unsafe eligible attempts presented at the specified boundary |
| Unsafe release fraction | Truth-unsafe released phase attempts / all released phase attempts |
| Represented-support escape | Releases lacking required represented support / eligible attempts exposed to that failure |
| Truth-based opportunity retention | Truth-safe, objective-useful opportunities served by a declared deadline / eligible truth-safe useful opportunities |
| Unjustified blocking | Information-justified useful opportunities blocked / eligible information-justified useful opportunities |
| Objective achievement | Runs achieving the predeclared objective / eligible runs; separately record achievement with no declared avoidable violation |
| False completion | Runtime completion claims without the independent experiment's required physical effect / completion claims |
| False successor continuation | Successor releases after an unsuccessful or unconfirmed prerequisite / eligible successor opportunities with that prerequisite failure |
| State-set coverage | Fraction of sampled boundaries where hidden truth lies inside the estimated set, plus failures by mismatch class |
| Decision correctness milestone | Predeclared agreement with independent reference responses, by decision category and as a declared aggregate |

The document's FSAR has no denominator and overlaps conceptually with its UARR numerator. Choose distinct units and names rather than reporting two undefined rates. Return null/NA for zero eligible opportunities, alongside counts. Distinguish proposed plan actions, plan acceptance, issued leases, local dispatch, actual device application, physical effects and confirmed outcomes.

A physical effect with a lost acknowledgement is not a physical failure. It is an unconfirmed effect from the runtime's perspective. Conversely, an acknowledgement without the effect is not a successful outcome. The experiment needs an independent scoring trace to distinguish both.

Decompose reaction timing into detection, transport, assurance computation, scheduling, command transit, physical response and confirmation. Local blocking, stop request and successful physical stop are different timestamps. Report missed responses and censored reactions, not only successful latencies. Keep simulation time separate from host/Pi elapsed time and clock-synchronization uncertainty.

The document's phrase statistically comparable safety is not established by a nonsignificant difference. If the intended claim is no materially worse safety than global fail-closed plus better availability, predefine a non-inferiority margin, justify the sampling plan and report compatible confidence bounds. Compare at the independent run/block level or account for clustering of attempts within runs. Preserve zero-event uncertainty, multiple-comparison discipline and adverse results.

H5 should test mismatch sensitivity rather than require performance to worsen monotonically. Conservative mismatch can lower unsafe releases while increasing blocking. A state-set gate cannot contain unbounded or invisible mismatch merely because it represents uncertainty; explicitly test both in-bound and out-of-bound errors, plus hidden dependencies and common-mode sensor failures.

## Preserve the 80% goal while improving the evidence

The review recommends demoting aggregate accuracy from the primary scientific endpoint. That is sensible because aggregate accuracy can conceal critical failure categories. It does not automatically replace the user's stated goal or the README target.

Retain at least 80% independently scored decision correctness as an engineering milestone unless the project owner changes it. Predefine its decision mix and confidence reporting. Publish separate safety, usefulness, outcome and timing endpoints alongside it. Reaching 80% must not be described as establishing acceptable physical safety or as permission for a 20% unsafe-action rate.

## Keep the intended agentic system in scope

The refinement develops the assurance research well, but it does not specify the autonomous planning loop. Current Qwen calls are on demand, the web adapter is restricted to S1 action vocabulary/order, and virtual time pauses during inference. These boundaries are appropriate for a demo.

Before live autonomous operation, place inference outside the timely assurance path. `Bridge.plan()` currently holds the same lock used by simulator controls throughout generation. That cannot become the architecture for continuously arriving physical evidence: safety monitoring must process evidence and actuator events while Qwen is loading or responding.

Use event-triggered planning with bounded retries, a budget, explicit objective progress, immutable started history and human escalation. Reject responses based on stale observation/model/plan revisions. The trusted runtime must continue monitoring when the model fails or stalls. Validate sustained operation and repeated-session resource behavior separately from an individual successful proposal.

Preserve raw malformed/unsafe proposals as planner-quality data. The current adapter rejects history/order violations before runtime installation. Benchmark artifacts must distinguish model output, normalization/validation rejection and runtime release decisions; otherwise good front-end filtering can be mistaken for DCRA's incremental contribution. Generate and freeze outputs first, then replay the same valid candidate proposals across the compared policies.

## Carry the seven project concerns into acceptance criteria

The document mainly addresses reliability, explanation and experimental quality. It is not a complete plan for security, privacy, interoperability or societal accountability.

| Concern | Additional concrete requirement |
| --- | --- |
| Security | Authenticate observation sources and gateway commands; define credential separation, replay protection and bypass tests. A `source_id` label alone is not authenticated provenance. |
| Privacy | Minimize planner inputs; protect stored prompt/evidence records; define retention, deletion and publication redaction. Local inference is an architectural choice, not proof of privacy. |
| Reliability | Real elapsed-time expiry, bounded recovery, durable state/restart behavior, delayed/lost effects and model-stall tests. |
| Explainability | Explain rejected/accepted phase decisions using evidence, possible states, model revision, safety assumptions and horizon. Attribute Qwen explanations separately from runtime certificates. |
| Interoperability | Versioned observation/action/result schemas and typed units; tested Pi adapters and chosen transport with consistent identity and time semantics. |
| Software quality | Independent oracle fixtures, exhaustive checking of a small uncertainty estimator, differential tests, atomic failure handling, replay and release provenance. |
| Societal accountability | Identify who sets goals and thresholds, human override/escalation boundaries, an independent actuator power cutoff, incident review and limits on deployment claims. |

Three identical temperature sensors on one I2C bus do not constitute three independent failure domains. Model sensor faults, bus loss, power failure and shared miscalibration separately. The independent thermometer/scoring trace must not become input to the admission gate. Temperature-threshold experiments should remain explicitly toy environmental demonstrations, not fire detection or certified building protection.

## Recommended implementation sequence and gates

| Stage | Work | Acceptance evidence |
| --- | --- | --- |
| 1. Experimental specification | Declare objective, hidden state/effects, invariants, observation bounds, scoring units and baseline schema | Hand-worked safe/unsafe/useful/wait cases; no unresolved metric denominator |
| 2. Independent scorer | Implement truth/effect and objective scoring outside admission logic | Existing B5 evaluated without calling `BuildingOracle`; deliberate scorer mutations are caught by fixtures |
| 3. Small uncertainty prototype | Add occupancy/egress possibilities and a bounded numeric proxy with a nonempty-set guard | In-bound truth containment; empty-set failure; fresh-but-wrong evidence and expired-but-bounded cases |
| 4. Mechanism comparison | Preserve B5; add B6, freshness-only, one-time and global comparators under new IDs | B5/B6 differentiating fixtures, including ties and possible increased blocking; targeted ablations |
| 5. Exploratory campaign | Run frozen candidates, post-authorization changes, outcome loss and mismatch cases | Event-level artifacts, separate analyzer, exposure/censoring counts and variance estimates |
| 6. Confirmatory freeze | Select primary contrasts, margins, sample sizes, seeds and analysis versions before headline collection | Frozen manifests and model/observation/oracle hashes; no post-result relabelling |
| 7. AI campaign | Freeze Qwen proposals, replay across methods, then test bounded event-driven planning | Proposal quality separated from validator/runtime containment; malformed outputs retained |
| 8. Pi deployment and HIL | Pi 5 assurance; Pi 4 sensing/gateway/actuation; independent effect feedback and timing | No direct planner actuator path; clock uncertainty, physical response and missed deadlines reported |
| 9. Generalization | One additional small domain using the same core and a different model/adapter | No domain-specific control logic added to the generic assurance kernel; otherwise narrow the claim |

Do not begin the full seven-policy campaign before at least one discriminating B5/B6 case and the independent scorer are validated. Keep common harness semantics fixed. Software model richness is a better first investment than more hardware, more sensors or a larger AI model.

## Literature verification and novelty wording

The checked primary sources support the review's caution about claiming runtime contracts, dependency freshness, missing-evidence handling or outcome verification as individually new. They do not prove that the proposed complete formulation is novel or that it will outperform existing systems.

- [Agent Safety Should Be a Runtime Contract](https://arxiv.org/abs/2608.11274) explicitly discusses preventive and evidential runtime controls. Treat it as a position/conceptual source rather than an experimentally matched comparator.
- [Fresh Memory, Stale Plans](https://arxiv.org/abs/2609.03340) currently has the v2 subtitle **Derivation Currency for Distributed LLM-Agent Memory**; update the document's reference or pin the cited earlier version. Its Planfence mechanism binds actions to recorded derivation dependencies.
- [Mission-Level Runtime Assurance](https://arxiv.org/abs/2607.23532) reports an explicit unknown under lost evidence. Its current author list differs from the document: Nikolaos Kekatos, Panagiotis Katsaros, Alexios Lekidis, Theodoros Nestoridis and Tom Nianios. Correct the bibliography to the selected version.
- [Managed Autonomy at Runtime](https://arxiv.org/abs/2607.00334) discusses execution modes and fallback. Its formal claims apply to its stated assumptions; they are not inherited by Evidra.
- [TwinGridShield](https://arxiv.org/html/2608.15391v1) explicitly separates same-model conformance from mismatch evaluation. This strongly supports independent/mismatched evaluation, not importing its power-grid numerical results into our building domain.
- [ModelPlex](https://link.springer.com/article/10.1007/s10703-016-0241-z) already addresses runtime compliance with verified model assumptions and bounded-deviation fallback. A plausible-state gate needs a more specific distinction than model-based safety alone.
- [Praxa](https://arxiv.org/html/2610.00015v1) separates authority and verified effect and carefully limits its empirical claims. A richer architecture does not guarantee measured outcome improvement.
- [Cyber-Physical GraphRAG](https://ieeexplore.ieee.org/document/11576057/) has a publisher abstract/record matching the cited title, DOI and pages. The abstract supports the described freshness and deterministic filtering themes; the full paper needs closer inspection before relying on detailed comparisons not established there.

Most recent cited arXiv works are preprints; label publication status and exact versions. Keep literature-supported descriptions separate from our inferred novelty opportunity. Do not compare published raw latency or safety numbers across different domains/hardware as if they were head-to-head results.

A defensible proposed framing is: **Evidra studies how bounded uncertainty derived from physical observations interacts with evidence-bound authority, phase-specific monitoring and outcome confirmation, and measures safety versus useful operation with independent experimental scoring.** This is a research formulation to validate. Use the stronger model-grounded title only once the uncertainty mechanism and its evaluation exist.

## Immediate decision

Proceed with the independent scorer and a small evidence-to-state prototype while preserving current behavior as a reference. The next concrete engineering deliverable should be a versioned experimental specification plus a validated truth/objective scorer. The next B6 deliverable should be a minimal pair of fixtures showing a meaningful decision difference and its assumptions. These steps provide the evidence needed to decide whether the full refinement strengthens the intended agentic assurance system.
