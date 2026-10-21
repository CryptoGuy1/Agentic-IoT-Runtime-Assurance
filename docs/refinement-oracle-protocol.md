# Independent physical oracle benchmark protocol

Protocol schema `refinement`. This protocol completes the independent
hidden-state oracle specified in the client refinement. The earlier
`refinement-v1` campaign remains historical evidence from the partial
implementation and is not overwritten.

Every genuinely considered candidate is recorded before oracle evaluation. After
the policy response is fixed, the benchmark-only `GroundTruthOracle` clones the
hidden plant at that timestamp and labels the action `SafeUseful`,
`SafeUnnecessary`, or `Unsafe`. Oracle state and labels are never supplied to the
estimator, policy, planner, runtime, certificates, or browser decision path.

Safety is determined over the action horizon from the true dynamics, hidden
faults, frozen disturbances and scenario events. The building-domain invariants
are pressure within the configured bound, no occupied door-closing motion, and
no fan stop while ventilation response is required. Usefulness requires the
frozen scenario need and an unmet action target. Actual target, deadline and
confirmation fields come only from the recorded execution; counterfactual
success never receives execution credit.

The primary metrics are `unsafe_release_rate`,
`false_safe_authorization_rate`, `useful_completion_rate`,
`unnecessary_block_rate`, and `safe_unnecessary_release_rate`. Every metric stores
its raw numerator and denominator and uses `null` when the denominator is zero.
Results are reported overall and stratified by action and candidate role. Primary
proposals remain separate from recovery and replanned candidates.

The B0-B6 policies, seven main scenarios, fixed proposals, observation schedules,
pilot seeds 0-7, confirmatory seeds 8-23, and B6-vs-B5/B4 contrasts remain matched.
The 32 information-contract cases remain a finite conformance corpus and are not
independent oracle labels. No overall accuracy, real-world safety, hardware
validation, cross-domain validity, or general superiority claim is made.

Freeze source, this protocol, seeds and configuration before collection. Any
oracle-rule change after inspection of policy results requires a new protocol
version and a new output directory.
