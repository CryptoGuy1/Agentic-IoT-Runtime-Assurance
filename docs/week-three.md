# Understanding Week 3

Weeks 1 and 2 checked whether evidence supported a conclusion. Week 3 adds the
rules for letting an action begin, watching it while it runs, and deciding
whether it actually succeeded.

Think of a fan in a building. These are different questions:

1. Is there a good reason to start the fan?
2. Is it still acceptable for the fan to keep running?
3. Has the fan actually reached the expected state?

Our program now keeps those questions separate. It still uses simulated commands
and observations; it does not control a physical fan.

## 1. Contracts are rules for each stage

An action definition points to the graph rules we already built.

| Contract | Question it answers |
| --- | --- |
| Start | Can this action begin? |
| Run | Can it continue running? |
| Commit | Can it cross a point where an effect cannot simply be undone? |
| Outcome | Do we have evidence that it achieved the expected result? |

For example, a fan might need confirmed hazard evidence to start, but only fan
health and acceptable pressure to continue. Losing the original hazard reading
after starting should not automatically stop it when its running rules remain
satisfied. This is the C8 test.

An irreversible action, such as suppression discharge, also needs a separate
commit check. Preparing the action does not authorize discharge.

## 2. A lease is a short-lived permission ticket

A valid conclusion alone is not a permanent permission to send commands.
The runtime issues a **lease** that records:

- Which action instance and stage it authorizes.
- Which plan version it belongs to.
- The evidence and observation versions used to justify it.
- The exact time when it expires.

Imagine receiving a ticket to start the fan, then a sensor supplies a newer
reading before you use it. The old ticket is rejected—even if the new reading
also supports starting. The runtime must issue a fresh ticket using the new evidence.

The ticket also expires and can be used only once. An unrelated sensor change
does not invalidate it just because the system's overall epoch increased.

A lease is a software permission object here, not a cryptographically authenticated
hardware token.

## 3. Dispatch means queueing a command, not proving success

When asked to dispatch, the runtime checks the lease, consumes it, changes the
action state and queues a command in one operation.

No sensor update can run halfway through that operation because one Rust owner
controls both evidence updates and command dispatch.

The fake actuator receives the command afterward. These are separate facts:

```text
Command queued → command delivered → response received → outcome verified
```

We do not treat “command sent” as “fan running.” For a separate commit stage,
the preparation command must first be delivered and acknowledged. This prevents
a commit command from silently replacing preparation that never happened.

## 4. Actions now have a lifecycle

A normal successful action follows this path:

```text
Pending → Ready → Leased → Executing → Completed
```

An action with a commit stage also passes through `Committed` after `Executing`.

| State | Meaning |
| --- | --- |
| Pending | Waiting for start conditions or earlier actions. |
| Ready | Start conditions and plan dependencies are satisfied. |
| Leased | A start ticket has been issued. |
| Executing | A start command has been queued. |
| Committed | A separately authorized commit command has been queued. |
| Aborting | A safe-abort response has been requested. |
| Recovering | A completion, compensation or mitigation controller is handling the situation. |
| Completed | A correlated completion report and valid outcome evidence support success. |
| Failed | Execution or required handling did not establish success. |
| Cancelled | An unstarted action was cancelled, or an abort/compensation was confirmed. |

The runtime controls these transitions. Callers cannot jump directly from Pending
to Executing or restart an instance that has finished. A retry uses a new instance.

## 5. What happens if running evidence disappears?

First, the runtime checks whether another witness still supports the run contract.
If so, it keeps running and updates its explanation.

If there is no remaining support, the response depends on the action:

| Kind of action | Requested response |
| --- | --- |
| Can be safely interrupted | Safe abort |
| Must be finished by a completion controller | Certified completion |
| Has already committed an irreversible effect | Forward mitigation |
| Has not crossed a required commit boundary | Precommit abort; no commit is authorized |

“Certified completion” is the name of a controller responsibility. Our fake
controller is not a physical safety certificate. Similarly, forward mitigation
tries to manage consequences; it does not undo an irreversible effect.

A request is not a successful recovery. The runtime waits for a correlated
controller response. If compensation is required after an abort, that is a
separate command and must also be confirmed.

When recovery begins, old outcome support is cleared. Other running actions that
depend on it are checked in the same operation. The runtime does not return to
normal execution just because a sensor later becomes valid again.

## 6. An acknowledgement is not the same as success

A response identifies the command it belongs to and can report:

- Whether it was acknowledged: yes, no, or unknown.
- Named observations such as whether the expected state was reached.
- Whether completion has been reported.
- When the observations were made.

The runtime maps only explicitly configured observations into evidence.
Missing acknowledgement becomes Unknown; it is not silently treated as success
or failure. A positive acknowledgement alone is also insufficient.

An action becomes Completed only when the appropriate completion report exists,
its outcome rule is valid, its minimum duration has elapsed, and any required
commit has occurred. Dependencies in the plan require predecessors to be Completed.

Duplicate, older, unknown-command and superseded-command responses are rejected.
A later response can supply information missing from an earlier response, using
a higher sequence number.

## 7. Time and plan changes still matter

Virtual time now processes action deadlines as well as evidence and lease expiry.
If we advance directly from time 0 to time 5, running evidence expiring at time 3
causes a response at **time 3**, not time 5.

An action that reaches its maximum duration without verified completion triggers
its handling policy. A controller that never confirms handling also has a deadline;
missing that deadline leaves the action Failed. No successful observations are invented.

Changing the plan version cancels unstarted instances and revokes old leases.
Running instances retain their history and running obligations. An instance from
an old plan cannot obtain a new commit ticket; an uncommitted action with a commit
stage is sent to its precommit abort controller.

## 8. Run the Week 3 demonstration

From the project directory:

```sh
cargo run --offline --example week_three
```

The example shows:

1. An old lease being rejected after a sensor refresh, with no command queued.
2. A fresh lease allowing execution to start.
3. Unknown acknowledgement keeping the outcome unknown and a successor blocked.
4. Running evidence expiring at time 3 while we advance to time 5.
5. A safe-abort command followed by a simulated confirmation, leaving the action Cancelled.

The automated tests cover C5–C10 and C17, plus lease reuse, result ordering,
controller failures, duration boundaries, outcome resets and cascading loss of support.
They exercise both full and incremental evaluation and compare lifecycle histories.

```sh
cargo test --offline
```

## 9. Where the code lives and what follows

Use `dcra::actions::ActionRuntime` when working with actions. It owns an assurance
runtime and exposes read-only access through `assurance()`. The standalone
`AssuranceRuntime` remains available for graph-only experiments.

The main methods are `register_plan`, `request_lease`, `dispatch`, `update`,
`advance_to`, `take_command` and `submit_result`. The example provides a working
sequence rather than requiring you to assemble these calls from scratch.

Read the [technical reference](week-three-reference.md) for precise data fields,
transition rules and implementation assumptions.

Week 4 will add the building simulator, physical interactions, resource-conflict
scheduling and the interactive terminal controls we discussed. The AI planner
remains a later integration step.
