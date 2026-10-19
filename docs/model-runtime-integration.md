# Model/runtime integration

16 October 2026

The optional `model_v1::ModelRuntime` owns an `ActionRuntime` and an `Estimator`. It connects the physical model to action authorization without replacing evidence permissions, leases, or correlated outcome handling. The original simulation/browser policy path remains separate.

## Run the review

```sh
cargo run --offline --example model_runtime
cargo test --offline --test model_runtime
```

The walkthrough exercises release, continuing-risk recovery, supervision after a synthetic completion report, recovery-command loss and a hazard-conflicting recovery. It prints fixture results and assumptions. Its early completion report deliberately tests a lifecycle boundary; it is not a scalar-plant outcome measurement or a new policy benchmark.

## Authority and release

Construct the wrapper before registering plans. Supply one `ModelBinding` per action definition: the physical action, a permitted physical recovery, a maximum delivery delay on the 100 ms grid, and whether delivery may be lost. Both normal and recovery commands use those transport bounds. The initial estimator must have default inactive inputs and no issued commands; importing an already actuated installation requires a future explicit effect-registration interface. Existing physical state values may be nonzero and are checked normally.

The wrapper exposes read-only access to its runtime and estimator. Use its methods for permission updates, model observations, plan registration, lease issuance, dispatch, command release, result submission and clock advancement. Do not independently use a second runtime or send commands around this boundary.

At lease issuance and dispatch, the current model checks candidate safety across its action window, including uncertain delivery. It also predicts the candidate's effect through the next 100 ms monitoring boundary and checks the configured recovery from that enclosure. Recovery must satisfy its constraints over its window and enclose its target by the deadline. Fan-stop feasibility therefore requires the entire fan range to reach at most 0.05, for example. This is finite recovery feasibility, not an invariant-set proof or an indefinite guarantee. Normal candidate usefulness/target achievement is not an admission guarantee in this delivery.

Dispatch retains the runtime's single-use lease checks. Physical release through `take_command` separately rechecks current phase permission, running support, plan version, candidate safety and recovery feasibility. A model change between lease issuance, enqueue and release can block a command. Blocked commands are retained for inspection/retry and are not added to the estimator's physical command history. Superseded held commands are discarded. Results for unreleased commands or observations predating physical release are rejected.

Runtime Start/Commit commands map to the configured action. SafeAbort, Recover and ForwardMitigation map to the explicitly configured recovery. CertifiedComplete maps to the original action with a recovery-style safety/target check. These mappings are configuration assumptions that must match the adapter and task semantics; they are not controller certifications. This first interface has one recovery mapping per definition, rather than multiple contingent recovery strategies.

## Monitoring and persistent effects

Clock advancement visits every 100 ms model boundary, even on a large caller jump; the inner runtime still processes its exact lifecycle/evidence deadlines. Non-grid or backwards time is rejected before mutation. The estimator's one-hour experiment limit reserves 6.1 seconds for recovery lookahead; this remains an in-memory prototype.

Every released device effect is retained until a newer command on that device supersedes it, regardless of whether its originating task becomes terminal. Monitoring projects the existing command history, without pretending to resend the original action. It checks continuing constraints and the configured recovery from the next boundary. Unsupported continuation or recovery feasibility requests the existing runtime's recovery while the task runs. For terminal tasks it queues a separate persistent-effect recovery, retrievable through `take_command` or `recover_persistent`. The binding explicitly authorizes that recovery after task completion.

A recovery release is checked for current safety and target feasibility. If unsupported, the API returns `IntegratedError::Blocked` with the check and reasons; it sends no model command and makes no physical-success claim. Callers must surface blocked recovery and keep servicing the release boundary. Monitoring requests and returned commands do not automatically operate a device.

Separate persistent recovery has no runtime command ID and cannot manufacture a task outcome. Feed real model observations back into the estimator to assess its effect. Completion or cancellation never erases an issued command's possible physical effect. Possible lost commands remain represented until superseded by a guaranteed newer command or constrained by observations.

`ModelAudit` records instance, actual recording time, phase, verdict, predicted check time/window, observation references, reasons and pressure enclosure. Recovery-feasibility checks can start at a future boundary; `recorded_at` distinguishes prediction time from the time the decision was made. Audit remains in memory; the example prints a human-readable walkthrough, not a durable production trace format.

## Verification and limits

The integration tests cover separate permission/outcome requirements, dispatch/release revalidation, active and terminal-effect recovery, unsafe/lost/delayed recovery, commit acknowledgement/permission checks, rejection of unreleased or pre-release results, and per-tick monitoring across time jumps. The existing estimator suite supplies containment, delayed observations, command ordering, fault and mismatch coverage; it remains part of regression checks.

All guarantees remain conditional on the configured discrete model. In particular, the existing model-mismatch counterexample still applies: a wrong bound can support a wrong prediction without detecting inconsistency. There is no new physical certification, 80% score, policy superiority result, autonomous planner or Pi adapter. Browser presentation and matched independently scored policy comparisons remain the next delivery.


Verification on 16 October 2026: 124 Rust unit/integration tests and one documentation
test passed; one existing manual replay test remained ignored. All 14 new
integration tests passed. The 18 Python tests, formatting, Clippy with warnings
denied, offline binary/example builds and the five-case walkthrough passed.


## Physical-claim replacement extension

The additional-gate behavior above remains available through `ModelRuntime::new`.
`ModelRuntime::with_claims` now supports reviewed replacement of selected direct
physical leaves with versioned model evidence. See [physical claims](physical-claims.md)
for graph migration, protected permissions/outcomes, certificate dependencies,
lease invalidation and the controlled difference fixtures. Multiple contingent
recoveries and the full independently scored policy campaign remain future work.


## End-to-end experiment connection

The [closed-loop demonstration](closed-loop-demo.md) now links this runtime to
actual simulated command application, separately measured correlated feedback
and independent post-run scoring. The browser continues to use its earlier
simulation path until the presentation integration is implemented.
