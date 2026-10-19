# Software refinement benchmark findings

19 October 2026 · refinement-v1 · 1,176 runs plus 32 contract cases per policy.

| Policy | Goals achieved out of 168 runs | Runs with pressure violation | False completion claims |
| --- | --- | --- | --- |
| B0 | 144 | 24 | 0 |
| B1 | 120 | 24 | 0 |
| B2 | 120 | 24 | 0 |
| B3 | 144 | 24 | 0 |
| B4 | 72 | 24 | 0 |
| B5 | 72 | 24 | 0 |
| B6 | 72 | 0 | 0 |

B6 had no observed pressure-violation runs in this fixed campaign. Its final goal achievement tied B4/B5 and was lower than several other policies. A blocked or recovered action can sacrifice a physically achievable goal while respecting the information-based contract. These tradeoffs are retained in the results.

The 32 hand-authored cases test START, RUN, COMMIT and OUTCOME contract responses. B6 matched all 32 specified responses. That is finite-corpus conformance, not a general decision-accuracy result or achievement of the broader 80 percent milestone.

Pilot seeds were 0–7 and confirmatory seeds 8–23. Paired contrasts use whole seed blocks across the fixed seven-scenario mix. The zero-event seed-block interval for B6 has a nonzero upper bound; zero observed failures do not establish zero risk. There is no non-inferiority or general superiority claim.

The [complete report](../output/refinement-benchmarks-20261019-validated/report.md) and [machine-readable summary](../output/refinement-benchmarks-20261019-validated/summary.json) contain denominators, per-category responses, paired intervals, censoring and limitations. The source and protocol used by the campaign are archived and checksum-verified. A confirmatory short-gap B6 run was rebuilt from that archive and exactly reproduced its stored source/configuration fingerprints, scores, coverage and runtime result.

Main policy comparisons exclude autonomous replanning and additional recovery choices. Those interventions are demonstrated separately by the browser scenarios and their saved traces.
