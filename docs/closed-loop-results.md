# Recorded end-to-end demonstration results

16 October 2026 · Seed 42 · Schema `model-closed-loop-v1`.

Source fingerprint: `fnv1a64:2d24cc6cf3deeb84`.

| Scenario | START allowed | Final task state | First physical targets (ms) | Confirmations received (ms) |
| --- | --- | --- | --- | --- |
| normal | True | Completed | FanStart: 4000 | 4100 |
| short-gap | True | Completed | FanStart: 4500 | 4600 |
| recovery | True | Cancelled | FanStart: None; FanStop: 3200 | None; 3300 |
| uncertain | False | Ready | No command | — |
| lost-feedback | True | Executing | FanStart: 4000 | None |
| delayed-command | True | Cancelled | FanStart: None; FanStop: 7700 | None; 7800 |
| delayed-reading | True | Completed | FanStart: 4000 | 4100 |

`None` means the event was not observed in the run. A cancelled original task can have a successfully measured and confirmed recovery. The uncertainty fixture makes one proposal at time zero; later Ready status is not a second authorization attempt.

All seven runs contained the actual state within the estimated set at all 81 recorded boundaries (567/567 total), with zero recorded pressure violations, occupied closing motion or false completion claims. These are results for the declared fixtures; they do not establish general model validity or a decision-correctness percentage. Lost-feedback achieved the physical fan target without confirming task completion.

The saved [run index](../target/model-demo-20261016-validated/report.md) and [manifest](../target/model-demo-20261016-validated/manifest.json) link the full reports and traces. These local artifacts are under target/ and can be removed by cargo clean. The table and [reproduction guide](closed-loop-demo.md) remain in the project documentation.
