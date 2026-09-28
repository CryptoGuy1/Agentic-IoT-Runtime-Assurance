# Week 3 technical reference

Start with [Understanding Week 3](week-three.md) for the plain-language guide.
This implementation follows the accepted Week 3 plan and Implementation
Specification v1 sections 18–25 and 33–35, with C5–C10 and C17. The referenced
Formal Models remain unavailable; choices not fully specified there are listed
below as implementation defaults, not formal claims.

## Ownership and API

`ActionRuntime::new(graph, definitions)` uses incremental evaluation;
`with_mode(graph, definitions, EvaluationMode::Full)` selects the independent
full evaluator. Both constructors validate the complete definition set before
returning a runtime. No new dependencies were added.

| API | Behavior |
| --- | --- |
| `register_plan(Plan)` | Validate instance identities and precedence DAG, stamp registration time, create Pending instances and refresh readiness. |
| `change_plan_version(id, version)` | Require a strictly higher version, invalidate unused leases and cancel unstarted instances. |
| `request_lease(instance, phase)` | Check phase eligibility and return a `LeaseToken`. |
| `dispatch(instance, phase, token)` | Validate authority, consume the lease, reset outcome evidence, transition and enqueue. |
| `update(EvidenceAtom)` | Apply externally supplied sensor evidence, then react to changed action support. |
| `advance` / `advance_to` | Process evidence/lease expiries and action/controller deadlines using virtual time. |
| `take_command()` | Deliver the next still-current queued command once; skip superseded or terminal-instance commands. |
| `submit_result(ActuatorResult)` | Validate correlation and freshness, apply bound evidence and update lifecycle state. |
| Inspection | `assurance`, `definitions`, `plans`, `instances`, `leases`, `commands`, `decisions`, `audit`, `running_by_root`, `running_by_evidence`; all return immutable views. |

The inner kernel cannot be mutated by clients. Local dispatch is one synchronous
`&mut ActionRuntime` operation with no callback or await between checking the
lease and enqueueing the command. The in-memory queue has no recoverable I/O
failure in this operation. This is not atomicity with a real device, nor a durable
transaction across process crashes.

## Data and configuration

`ActionDefinition` carries the specified action ID, phase roots, min/max duration,
interruptibility, recovery type, resource read/write sets and `max_start_lease`.
It additionally declares controller capabilities and result bindings.

`OutcomeBindings` contains an acknowledgement evidence ID, a map from observed
field names to evidence IDs, and an observation `max_age`. All bound IDs must be
distinct Evidence nodes among the outcome root's ancestors. At least one observed
field is required. Field name `acknowledged` is reserved for the acknowledgement.

Definitions cannot share writable bindings. Instances of the same definition may
reuse them sequentially, but an instance in Leased, Executing, Committed, Aborting
or Recovering reserves them exclusively. Start/run/commit roots cannot depend on
the definition's own resettable outputs. These restrictions make outcome resets
safe without dynamic graph rebuilding in Week 3.

Resource declarations are recorded; resource conflict scheduling is Week 4 work.
Definitions require matching ActionStart/ActionRun/ActionCommit/ActionOutcome
node kinds, positive maximum duration, positive lease/outcome lifetimes, and
`min_duration <= max_duration`. An irreversible class requires a commit contract,
irreversible recovery classification, safe precommit abort and mitigation
capabilities. Preemptible actions require safe abort; completion-safe actions
require certified completion. Any commit contract additionally requires precommit
abort; compensatable recovery requires a compensation capability.

Capability flags describe the configured controller contract. They do not prove
physical certification or establish a live hardware connection.

`Plan.actions` maps globally unique instance IDs to definition IDs. `precedence`
contains `(predecessor, successor)` pairs referencing that same plan, with no
cycles or self-edges. `created_at` is stamped by registration; any caller-supplied
value is ignored. Empty plans are allowed. Registered IDs are never reused.

`ActionInstance` records its original plan version permanently, lifecycle state,
creation/start/commit/terminal timestamps and current lease/command IDs.
`completed_at` records terminal time for Completed, Failed or Cancelled.

## State transitions

| From | Allowed destinations | Guard or interpretation |
| --- | --- | --- |
| Pending | Ready, Cancelled | Start/dependency eligibility, or plan supersession. |
| Ready | Pending, Leased, Cancelled | Conditions lost, authority issued, or plan supersession. |
| Leased | Ready, Pending, Executing, Cancelled | Lease revoked, conditions lost, valid dispatch, or plan supersession. |
| Executing | Committed, Completed, Aborting, Recovering, Failed | Commit authority, supported completion, or handling policy. |
| Committed | Completed, Aborting, Recovering, Failed | Supported completion or handling policy; irreversible class uses mitigation. |
| Aborting | Recovering, Cancelled, Failed | Compensation required, successful abort, or failed/timed-out handling. |
| Recovering | Completed, Cancelled, Failed | Certified completion, successful compensation, or mitigation/failure outcome. |
| Completed / Failed / Cancelled | None | Retry requires a new instance. |

Transitions are private. Public operations return typed errors for illegal state
or phase use; callers cannot set state or enqueue arbitrary actuator commands.
Ready reflects predecessors and the start contract. Lease requests and dispatch
also require a valid run contract when configured.

A commit request requires an Executing instance and a delivered, positively
acknowledged Start command. This additional implementation guard prevents commit
from superseding preparation before it was delivered. Commit dispatch resets
outcome evidence again, so preparation observations cannot prove committed success.

## Lease checks and revocation

`AssuranceLease` contains the action instance, phase, plan version, assurance epoch,
issue/expiry times, preferred witness, per-evidence versions, nonce, consumed flag
and optional revocation reason. Expiry is:

```text
min(witness.horizon, issued_at + max_start_lease)
```

The same configured cap applies to commit leases for Week 3. Overflow when
computing admission deadlines is rejected before authority is issued or consumed.

The stored lease—not a caller-editable copy—is authoritative. Dispatch checks the
token's ID/nonce, instance/phase, active lease identity, consumption/revocation,
expiry, current plan version, witness versions and current contract validity.
Dependencies, binding exclusivity and running support are rechecked.

Evidence versions are compared precisely; the global assurance epoch is logged
but not required to match. Thus unrelated evidence changes do not revoke a lease.
Changing selected evidence makes the old lease unusable even if an alternative
witness exists; request a new lease. One active lease per instance is allowed;
issuing a replacement revokes the prior one. Consumed leases remain in history.

Lease IDs, command IDs and nonces are deterministic unique values within a runtime.
They prevent accidental reuse here; they are not authentication credentials and
must not be used as a hardware security protocol.

## Running subscriptions and response policies

Running instances are indexed by run root and selected witness leaves. Changed
roots and evidence identify affected subscriptions. The lifecycle pass also
checks current run support before completion so a controller-induced cascade
cannot complete another action against newly invalidated support.

| Situation | Command / state | Successful handling result |
| --- | --- | --- |
| Alternative run witness remains | Continue; refresh subscriptions | No interruption. |
| Preemptible loses support | SafeAbort / Aborting | Cancelled, or Recovering plus Recover if compensation is required. |
| Completion-safe loses support after any required commit | CertifiedComplete / Recovering | Completed only with positive completion and valid outcome support after minimum duration. |
| Required commit has not occurred | SafeAbort / Aborting | Cancelled after confirmed abort; compensatable policy may require Recover. |
| Irreversible action already committed | ForwardMitigation / Recovering | Failed as an action outcome, even when mitigation succeeds; commitment history remains. |
| Compensation needed after abort | Recover / Recovering | Cancelled after confirmed compensation. |
| Controller negative acknowledgement or deadline exceeded | Failed | Never infer a successful physical response. |

Unknown and Invalid triggers remain distinguishable in decisions. Evidence becoming
valid later does not undo Aborting/Recovering. New safety commands supersede old
normal commands; delayed results for superseded commands are rejected.

Beginning recovery and terminal Failed/Cancelled transitions invalidate this
execution's outcome support to Unknown. Resets are runtime-owned evidence updates,
not successful actuator observations. Resulting changes are processed to a fixed
point in the same operation, including other running actions that depend on them.
Cancelling an instance that never started does not reset shared definition outputs.

Controller deadlines default to `command.issued_at + action.max_duration`.
A completion-safe controller may finish after the original action timeout, but
must confirm a supported outcome before its own deadline. An irreversible
mitigation success never rewrites the original action as Completed or undone.

## Actuator result mapping

`ActuatorResult` includes command ID, monotonically increasing per-command
sequence, `acknowledged: Option<bool>`, an observed-state map of configured field
names to `AssuranceStatus`, `completed`, and virtual observation time.

This is a normalized software adapter boundary. Conversion of raw device values
to predicate statuses remains domain-specific adapter work.

Results require a known, delivered, still-current command on a nonterminal
instance. Reject duplicate/decreasing sequences, observation times before command
issue, decreasing observation times, future times and unbound observed fields.
Equal observation times with higher sequences are permitted. Sequence zero may
be the first response. Every accepted response represents a complete snapshot
of the configured fields; omitted fields become Unknown rather than retaining
older positive observations.

Start, Commit and CertifiedComplete responses write the configured evidence:
acknowledgement None/false/true maps to Unknown/Invalid/Valid. Bindings get stable
`actuator:<action_id>` provenance and runtime-assigned increasing versions.
Other safety-controller responses carry acknowledgement/completion only and must
not write the action's ordinary outcome fields.

All atoms in a result batch are validated before any are applied. The assurance
kernel records per-atom transitions, but action lifecycle reactions occur only
after the entire result has been applied. External `update` cannot write reserved
outcome IDs. Output support is reset on each start and commit; stale responses
from prior executions cannot refill it.

`completed=true` and positive acknowledgement record reported completion; they do
not alone set Completed. The outcome root must be Valid, any required commit must
have happened, and the minimum duration must have elapsed. An early supported
report waits until minimum duration and is rechecked then. Its support can expire
while waiting. A normal result that arrives after timeout is superseded by the
handling command and cannot retroactively complete the action.

`FakeActuator::respond_next` is a convenience for deterministic tests. It takes a
queued command and submits the caller's chosen observations at the current time;
it does not invent success, perform physics, or advance time. For multi-message
progress/acknowledgement sequences, use `take_command` and `submit_result` directly.

## Time, plans and decisions

Virtual time advances through the earliest evidence expiry, active lease expiry,
action minimum/maximum duration or controller deadline before reaching the user's
target time. At a shared timestamp, evidence expiry and running reactions precede
completion/timeout handling and any later caller start/commit request. Previously
received, still-supported completion may finish at minimum duration, including
when minimum and maximum coincide. A new result submitted after advancing to the
maximum deadline cannot bypass the timeout already processed there.

Only clock advancement methods move time. Returned future commands/results do not
automatically move it. Backward time is rejected. Fresh outcome deadlines and
controller deadlines saturate at `Duration::MAX`; no time can wrap backwards.

Plan version updates must increase monotonically. Unstarted instances become
Cancelled; running execution attribution remains unchanged. An uncommitted
instance with a commit contract requests safe precommit abort. Other running or
committed instances keep their run obligations and may finish on the old version,
but cannot obtain fresh old-version commit authority. This API does not yet
implement partial-plan suffix replacement or adoption of running instances.

Unknown admission/commit support yields Revalidate; Invalid yields BlockAndReplan.
Timeout, LeaseRejected, Eligible and RecoveryRequested are also structured decisions.
There is no planner callback. Action audits record time and assurance epoch with
state changes, lease operations, queue/delivery, results, plan versions and decisions.
Assurance and action audits are separate immutable histories. Neither is durable.

## Verification and deferred work

`tests/week_three.rs` covers C5–C10/C17 and additional state, correlation, recovery,
outcome reset, deadline and cascading-dependency cases in both evaluation modes.
It also compares states, leases, commands, decisions, evidence and both audit
histories after each step across all three interruptibility classes.
A kernel unit test verifies that a malformed batch never applies a valid prefix.

The existing Week 1 tests and Week 2 exhaustive/generated tests remain part of the
completion gate. Resource scheduling, building physics, hardware adapters, real
controller certification, durable logs, concurrent gateway operation, interactive
terminal controls and AI planning are not implemented by Week 3.
