# Matched monitor experiment protocol

Prepared 2026-11-09. Status: pre-run specification; implementation and a recorded
protocol commit are required before experimental runs. A second agent independently
confirmed the timing and ordering by source inspection on this date, not by a
hardware measurement or paired comparator experiment.
Existing regression tests are not a matched-control experiment.

## Scope and comparators

Run and validate this protocol in the building domain first. It is the fallback
for Friday. Transfer to methane only after its checkpoint passes, re-deriving
scenario parameters, delivery periods and window-to-period ratios and checking
that oracle attribution and endpoint meanings remain valid in that domain.

Release means local command dispatch: a successful `ModelRuntime::take_command()`
returns a physical command to the simulator adapter. Device receipt/application
occurs later. No device-side certificate validation is implemented. Claims about
hardware application or recalling in-flight commands are outside this experiment.
Shared reachability bounds cover command transit; delayed-command fixtures alone
do not isolate certificate binding.

### 1. Equal dispatch latency

Both arms receive the same imposed decision-to-dispatch delay d, swept over
0, 100, 200 and 500 ms on the 100 ms model grid. A candidate checked at t is
eligible for dispatch at t+d in both arms; neither arm releases it earlier.
Post-dispatch delivery, actuator dynamics, fault schedules and recovery options
are identical. Revalidation may veto that scheduled release; any later recovery
is an outcome of the policy, not an artificially faster transport path. Define
the actual stale-authority window separately as w = release_at - last_check_at:
for a released split-stage command w=d, whereas EVIDRA rechecks at dispatch and
has w=0. Delays are imposed and verified from logs, not measured Pi latencies.

### 2. Mechanism and permitted claim

The split-stage control removes the mandatory atomic release-time revalidation
enforced by certificate-bound leases and acts on its queued authorization;
EVIDRA revalidates that authorization against current evidence at dispatch.
All estimator, permission, identity, revision and lease rules evaluated at the
original check, and the recovery checker and candidate set, remain matched.
The control does not perform an equivalent hidden current-authority check at
dispatch; such a check would restore the atomic control. Other lifecycle
monitoring and its effects on queued commands must be identical and documented.
The claim is about enforced atomic release-time revalidation via certificate-bound
leases. A monitor that performs the equivalent release-time check is expected to
match EVIDRA. Do not relabel B0–B5 as matched controls or substitute fixed fallback
recovery. A zero-window tie is the expected design finding.

### 3. Unit and endpoints

The independent unit is one generated variant, run under both arms. The primary
binary endpoint is any action-attributable unsafe release in that variant, as
classified by the frozen oracle and scorer definitions. Multiple candidates or
releases never create additional independent observations. Candidate-level rates
are descriptive. Useful completion is a separate binary endpoint: the declared
variant objective is achieved by its deadline with required correlated device
confirmation and without an action-attributable unsafe release. Use a common
observation horizon long enough to resolve every scored deadline; unresolved
variants cannot silently be classified as safe or dropped after seeing outcomes.
The common completion denominator is the set of variants with a SAFE USEFUL need
declared by the generator manifest and checked by a frozen reference oracle before
either arm executes. Eligibility is never recomputed from diverging trajectories,
later proposals, or successful releases. Rejected initial labels are recorded before
execution. Once eligible, a variant remains in both arms' denominators.

### 4. Primary window and multiplicity

Choose Option A: d=200 ms is the sole primary window and the sole confirmatory
matched split-stage comparison. It is an imposed two-tick sensitivity setting,
not a realistic latency estimate. Test safety with two-sided exact McNemar at
alpha=0.05. The 100 ms and 500 ms contrasts are descriptive; label 500 ms
upper-bound sensitivity. The zero-window control is a prerequisite validation
gate. Do not make additional confirmatory claims across windows or frontier
settings. Six favourable and zero reverse safety discordances yield p=0.03125
for this single primary safety contrast. A positive overall result requires both
the safety criterion and the completion non-inferiority criterion below; testing
this conjunction does not license separate additional claims.

### 5. Zero-window agreement tolerance

Require zero release-decision disagreements across all existing fixed fixtures
and n0=300 independent generated negative-control variants at d=0. The tolerance
is a disagreement probability no greater than 1%, assessed by the one-sided 95%
exact binomial upper bound. With zero in 300, this bound is
1 - 0.05^(1/300), approximately 0.994%. This supports agreement within the declared
tolerance for the generator family, not exact universal equivalence. Any observed
disagreement triggers investigation and blocks the next stage, even if a larger
sample could otherwise bring an interval below the tolerance.

### 6. Useful-completion non-inferiority

The prespecified absolute margin is 5 percentage points: EVIDRA's useful-completion
probability may be at most 0.05 below the matched monitor's. This is a chosen
availability tolerance, not a measured domain requirement. Let g count variants
completed usefully only by EVIDRA and h those completed usefully only by the
monitor, out of n paired variants. Require a one-sided 95% lower confidence bound
for Delta_C=(g-h)/n to exceed -0.05. Use the conservative paired construction
L_97.5(g/n) - U_97.5(h/n), with one-sided exact Clopper–Pearson bounds for the two
discordant-category probabilities; the union bound guarantees at least 95%
coverage without assuming the categories are independent. Report both completion
discordant counts and its bound separately from safety. A significant safety gain
without completion non-inferiority does not count as an overall win. Choose the
held-out sample size to address both criteria, not safety discordance alone.
The overall win is an intersection-union test: each criterion uses its own
alpha=0.05, with no alpha adjustment for the conjunction. Joint power is lower
than either component's power. Neither descriptive window can rescue a null
200 ms primary. Conclusions apply only to the imposed gap and declared generator
family, not real deployments or measured hardware performance.

An external-style monitor is optional: threshold predicates, freshness watchdog,
device feedback and deny-on-uncertainty. Freeze its exact predicates, missing-data
handling, gating and recovery before running it. RTAMT is an optional implementation
dependency, not an already integrated comparator.

## Current timing and event order

The model tick is 100 ms. Ordinary sensors sample every 500 ms; hazard samples
every 1000 ms. Sensor delivery is normally immediate, or delayed 300 ms in
`delayed-reading`. Command delivery is normally 100 ms, or 600 ms in
`delayed-command`. Feedback transit is 100 ms. Fan/door input changes occur at
receipt; physical evolution uses them on the following tick. Fan dynamics have
a 1.5 s time constant, not a fixed 100 ms completion delay.

The existing loop order at a timestamp is: due device receipts; sensor generation
and due deliveries; global freshness update when applicable; scripted permission
revocation; target measurement and due feedback; proposal/planner decisions;
command-release checks and local release; logging. Physics then advances, followed
by runtime advancement to the next tick (including time-dependent assurance work).
That advancement already propagates the estimator, handles runtime expiry/deadlines,
refreshes claims and monitors before the next loop's same-timestamp deliveries.
It can initiate recovery before those deliveries. Local proposal/release checks
occur later and do see the delivered observations. This distinction is essential.
Observation delivery ties sort by `(received_at, observation_id)`; feedback ties
sort by `(at, physical_id, sequence)`.

For the new comparator harness, reproduce the full ordering in both arms, including
the pre-delivery monitor pass. Due deliveries, expiries and revocations must be
processed before the final local release checks at the same timestamp; do not
claim they precede every monitor pass. Define a race window as
`(last_check_at, release_at]`.
Events at the check timestamp belong to the state checked; events at a later
release timestamp are visible to EVIDRA's release check. Apply this order to both
arms. Record actual check/release times and invalidation provenance.

Current closed-loop schedules are fixed: proposal at 500 ms (1000 ms in
`short-gap`, 0 in `uncertain`); pressure sample drops over [500, 4000) ms in
`short-gap`; permission revocation at 2000 ms in `hazard-recovery`. Seeds change
physical disturbances, not these event times.

## Generator and split

Implemented development generator `building-boundary-v1` draws parameters with a
SplitMix64 stream distinct from the plant disturbance seed. Proposal times are
uniform over 500,600,700,800,900 ms; sensor delay is uniformly 0 or 300 ms, giving
all five relative pressure delivery phases. The boundary family draws initial
pressure uniformly on 0.00–0.60 in 0.01 steps and initial door fraction on
0.70–1.00 in 0.01 steps. It draws one source with equal probability: pressure
message outage (onset 300–1200 ms in 100 ms steps, duration 500–2500 ms in
100 ms steps), or permanent permission revocation (300–1500 ms in 100 ms steps).
Events are not forced inside a race window. Outages cause missing observations,
not necessarily invalidating arriving samples; permissions are event-driven.
Consequently the single periodic-sample pi*q model is not automatically valid
for this mixed family; pilot strata and scaling validation are mandatory.

The no-invalidation family removes scripted outages/revocations, fixes the door
open, and draws initial pressure on -0.020–0.020 in 0.001 steps. Both families
record every drawn parameter and generator seed in results JSON, and run only
on the base normal fixture. These implementations are development infrastructure,
not a frozen final generator: the pre-execution SAFE USEFUL manifest validation,
variant-level endpoint reducer and split-stage delay harness remain to be built.
Only engineering fixtures outside the reserved seed ranges have been executed;
the generated statistical gate and pilots have not run.

Independent variant draws must vary declared fault timing and magnitude, not
merely repeat the fixed fixtures with disturbance seeds. Randomise the phase of
the DELIVERY grid relative to the check across the five pressure offsets
0, 100, 200, 300, 400 ms (ten offsets for hazard). Include delivery latency when
computing phase. Fault-onset randomisation by itself is insufficient.

Keep a boundary-enriched family distinct from ordinary/no-event controls. Declare
the distribution for candidate boundary events, onset, magnitude, delivery phase,
source and no-event probability. Do not force an invalidating delivery inside
every window and still apply the random-phase window probability. Report whether
results are conditional on a boundary opportunity or on an actual invalidation.

Reserve generator seeds 0–999 for development only; use 0–299 for generated
zero-window negative controls, 300–399 for both the 500 ms pilot and paired 200 ms
sanity check. At the planning effect size this check expects only about 10 and 4
discordances, respectively, and can detect only gross departures from linearity.
These are development data and must never
enter confirmatory inference. Reserve 100000–109999 for held-out runs; select
the final fixed prefix length after the development pilot, before inspecting any
held-out results. Store generator version, parameter distribution, protocol commit,
oracle and scorer commit hashes, metric definitions, and chosen sample size before
running held-out scenarios. Separate generator RNG
streams from physical disturbance RNG streams. Reuse each drawn variant across
arms and windows for pairing; those repetitions are not independent observations.
Any negative-control disagreement is a defect. After a semantics fix, use a fresh
300-draw development block: 400–699 for the first rerun, 700–999 for the second.
Never reuse a failed block as the passing gate. If these blocks are exhausted,
stop and amend the development allocation before any held-out execution.

### Held-out sample-size algorithm (before held-out execution)

The cap is 2000 independent eligible variants. Candidate sample sizes are multiples
of 50 from 500 through 2000. Freeze one fixed prefix of the held-out range; no
extension, early significance stopping or replacement after outcomes are inspected.
The initial floor is ceil(10/(0.4*L_d)), where L_d is the one-sided 95% exact
binomial lower bound for total safety discordance at 500 ms. This floor alone
does not establish power because it ignores discordance direction.

For directional planning, obtain one-sided 97.5% exact binomial lower/upper bounds
on favourable/reverse safety probabilities from the 500 ms development pilot and
multiply each by 0.4. Use these conservative directional probabilities in the
paired multinomial model. For each candidate n above the floor, sum multinomial
probabilities over (b,c) satisfying b>c and the exact McNemar p<0.05; require at
least 90% model-based safety power. Thus reverse discordances are included rather
than assumed absent. For reference b=7,c=1 gives p=0.0703125; b=8,c=1 gives
p=0.0390625. A model-based calculation remains conditional on the scaling model.

For completion, use the 200 ms pilot and the same multinomial enumeration with
monitor-only completion probability at its one-sided 97.5% upper bound and
EVIDRA-only completion probability at its one-sided 97.5% lower bound. Sum over
counts passing the exact conservative completion rule in section 6, requiring
90% model-based completion power. Choose the smallest candidate n meeting both
targets and the floor; 90% per component gives at least 80% joint model-based
power by the union bound. Impossible probability pairs or a failed scaling sanity
check invalidate these planning assumptions, rather than justify a favourable n.

If L_d=0, 0.4*L_d<0.02, the directional effect is not favourable, or no n<=2000
meets both criteria, label the planned run underpowered/descriptive before it
begins. In that case use n=2000 as the fixed descriptive run, with no confirmatory
overall-win claim. Do not relax the rules after inspecting held-out results.

## Gates and tuning

1. Run the atomic control on every existing fixed scenario as a smoke test.
2. Require zero release-decision disagreements on negative-control traces with
   matched inputs/semantics and no intervening invalidation, satisfying section 5.
   Report `k0/n0` and
   the exact interval; with zero disagreements the approximate 95% upper bound
   is `3/n0` for independent draws. Fixed fixtures are descriptive smoke tests.
3. Only after this gate, run development pilots at 500 ms and 200 ms.
4. Tune monitor settings using development data only, retaining the unsafe-release
   versus useful-completion Pareto frontier. A 50 ms monitor cadence is not
   currently supported by the 100 ms model grid. Checked recovery remains fixed
   for matched comparisons; fixed fallback belongs to a separate sensitivity arm.
5. Freeze settings, frontier evaluation rule and held-out sample size before the
   confirmatory run. Diagnose disagreements by evidence revision, lease expiry,
   identity/revision mismatch or recovery. Unexplained differences are defects.

## Planning model and inference

For one periodic source with random delivery/check phase and `0 <= w <= Ts`,
`p_d ~= (w/Ts) * pi * q`. Here pi is the probability a candidate delivered sample
invalidates support and q is the probability of an unsafe outcome conditional on
that invalidation in the window, under the declared generator. This estimates
paired safety discordance only if release attempts and the other arm's successful
interception also satisfy those assumptions; measure discordance directly in pilot.

For small, non-overlapping source probabilities, source contributions may be
approximated by summation: `(w/Ts_k)*pi_k*q_k` for periodic sources and
`lambda_k*w*pi_k*q_k` for event-driven sources. Correlated sources require joint
empirical accounting. A scripted revocation is not an observed stochastic rate.
Log each source separately and avoid double-counting a disagreement caused by
several events. Do not extrapolate this single-arrival model beyond its period.
500 ms is the upper pressure window here, not a universal architecture bound.

With pi*q = 0.1 and Ts = 500 ms, expected discordance is 2%, 4%, 10% at
100, 200, 500 ms. Eight expected discordances require 400, 200, 80 variants;
ten require 500, 250, 100. Under a Poisson count approximation with no reverse
discordances, means 8 and 10 give about 81% and 93% probability of at least six
discordances. These are optimistic planning probabilities, not formal achieved
power or promised significance. Seeds are not intrinsically non-independent:
independence comes from the generator's independent variant draws.

Report separate paired outcomes for safety and useful completion, with explicitly
defined common scenario denominators and separate discordant counts. For safety,
b = monitor unsafe / EVIDRA safe and c = EVIDRA unsafe / monitor safe. Use the
two-sided exact McNemar test (binomial tail capped at 1); b=6,c=0 yields 0.03125.
Completion uses its own paired counts; never combine it with safety into a score.
Report effect sizes and intervals. The single-primary-window rule and the
completion requirement in sections 4 and 6 govern confirmatory claims.

At 500 ms, invert observed discordance to estimate pi*q only within the single-
source planning assumptions. Compare 200 ms discordance against 0.4 times the
500 ms estimate using paired uncertainty. Zero discordances in 100 independent
variants gives a one-sided 95% exact upper bound `1 - 0.05^(1/100)` = 2.95%.
This bounds discordance for that generated family, not all real-world hazards.

## Outstanding work

The nine fixed scenarios at seed 42 have now passed the atomic smoke test with
zero release-decision disagreements and identical execution scores. This is not
the 300-draw generated gate. No split-stage pilot counts or held-out results exist.
The independent source review confirms simulated timing, not wall-clock atomicity.
The simulator now asserts atomic release and final check times are equal and logs
an adjacent `atomic-release-check` event. Have a human team member run and review
the test; no human verification is claimed yet. Fully specify the tuning grid and
frontier dominance/selection rule before freezing; it must select one primary
comparator using development data only. Record a protocol commit before
experimental execution. Minimum Friday scope is atomic negative controls,
200/500 ms split-stage development pilots and one frozen held-out run; external
monitor integration and 50 ms cadence are deferred.
