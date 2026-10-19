# Refinement benchmark protocol

Protocol frozen before collection on 19 October 2026. Schema `refinement-v1`.
Historical Week 5 B0–B5 manifests retain their meanings. IDs below always carry
this new schema and descriptive policy names.

| ID | Policy | START and COMMIT | RUN and persistent effects |
| --- | --- | --- | --- |
| B0 | permission only | Permission and existing phase prerequisites | Permission only |
| B1 | static point predicates | Last pressure point predicate without age enforcement | Same static predicate |
| B2 | fresh point admission | Fresh pressure point predicate | Permission only |
| B3 | point model once | Point-initial-state model check at START | Permission only |
| B4 | global fresh continuous | Fresh pressure predicate and all six sensor channels fresh | Same global requirement |
| B5 | dependency fresh continuous | Fresh pressure predicate on relevant action contracts | Same relevant requirement |
| B6 | bounded model continuous | Versioned physical claims, candidate and feasible recovery checks | Bounded continuation and recovery checks |

Every method retains permission, identity, single-use leases, plan versions and
correlated measured outcome handling. COMMIT requires acknowledged START in the
underlying runtime. No policy receives hidden truth. B3 collapses current state
and disturbances to a point for its one-time check, retaining command timing.
B6 receives the declared dynamics and bounded-error model; differences cannot
be attributed solely to graph closure. All use the same physical plant,
exogenous keyed noise, observation schedule, fixed proposals and device adapter.
Policy-dependent commands and subsequent trajectories may differ.

Two evaluation lanes are collected:

1. Frozen boundary cases with manually specified information-justified reference
   responses for START, RUN, COMMIT and OUTCOME. The reference is part of the
   versioned fixture specification, not the runtime verdict. Repeated attempts
   are separate rows only when explicitly specified; waiting/withholding is one
   binary boundary response. Every category has equal weight. Results apply to
   this finite corpus; no population-level 80 percent claim is made.
2. Closed-loop execution on seven fixed scenarios, with scoring from actual
   trajectories over 12 seconds. This extends beyond the fan-start window to test
   persistent device effects; the historical demonstration remains eight seconds. Pressure TTL is 750 ms in this schema, greater than the 500 ms
   source period; the historical demonstration keeps its 500 ms TTL. Primary
   contrasts are B6 versus B5 and B6 versus B4. Seeds 0–7 are pilot and seeds
   8–23 are confirmatory. No thresholds/configuration may be tuned after pilot
   results without a new protocol version. Freeze source/protocol hashes and
   preserve one summary per policy/scenario/seed. Replay reconstructs full traces.

Report physical violations, unsafe observed command windows, goal achievement,
physical targets without confirmation, false completion, containment, release
counts and censoring separately. Rates always include raw denominators and use
null when no eligible units exist. Report paired per-seed run-block differences
with uncertainty intervals across the fixed scenario mix; do not treat correlated
commands within runs as independent. Report ties and worse results openly.
There is no non-inferiority claim or general superiority claim.

Main policy comparisons use fixed proposals and the same configured recovery
choice. Broader recovery alternatives and bounded autonomous replanning are
separate intervention runs/ablations, so their gains do not get mislabeled as
state-estimation gains. The deterministic planner is outside execution authority,
uses only delivered model knowledge/history, preserves started work, and has
explicit retry, cooldown and planning limits. Hardware and continuous-time
validation remain outside this software protocol.
