# Versioned model-backed physical claims

Implementation delivery — 16 October 2026; schema `physical-claims-v1`.

This delivery closes the obsolete-conjunct problem identified in the client refinement. Selected direct physical predicates can now be replaced by model-produced evidence in START, RUN and COMMIT contracts. The original direct evidence is retained for inspection and expires normally. Permission leaves, graph thresholds, unrelated requirements and genuine outcome contracts remain unchanged.

## Reproduce

```sh
cargo test --offline --test physical_claims
cargo run --offline --example physical_claims
```

The [recorded comparison](physical-claims-comparison.md) demonstrates a tie on fresh safe information, a short-gap availability difference, and additional withholding when a numeric error range includes unsafe pressure. These are controlled mechanism fixtures. They do not run or rename historical B5, establish full B6 policy performance, or supply an independently scored correctness percentage. The direct comparator uses a point threshold and 500 ms TTL; the model receives additional dynamics and bounded-error information. This difference in capabilities is declared rather than attributed solely to certificate production.

## Configuration and graph migration

`ClaimConfiguration` names an immutable configuration revision, explicit `PhysicalClaimBinding` replacements and protected permission evidence. `substitute_physical_claims` returns a new graph; it never mutates the baseline graph. It replaces each selected direct leaf's premises with a distinct model leaf, preserving rule thresholds and all other premises. The obsolete direct leaf remains in the graph for raw observations but is no longer an admission/run/commit conjunct.

Mappings must be reviewed physical predicates. The helper rejects duplicate/overlapping IDs, unused direct leaves, protected permission replacements, and any direct or model leaf participating in an outcome contract. It rejects a migrated graph that still depends on the obsolete direct leaf in a phase contract. The protected set is an explicit application declaration: the software cannot infer whether an arbitrarily named sensor predicate actually encodes organizational permission.

Create an `ActionRuntime` with the migrated graph, then attach it using `ModelRuntime::with_claims`. Model leaves must initially be unwritten. External writes to producer-owned leaves are rejected. The existing `ModelRuntime::new` remains the original additional-gate mode and does not substitute predicates.

Configuration and model assumptions cannot be changed in place through this interface. A revised configuration requires construction of a new runtime and new local authority. Leases and certificates are scoped to their owning runtime; this prototype does not provide persistent cross-process authority or a cryptographic certificate format.

## Claim semantics

The initial physical predicates are:

| Predicate | Supported condition at every represented boundary |
| --- | --- |
| `PressureWithinLimit` | Entire pressure interval is within −0.80 to +0.80. |
| `OccupancyClear` | Occupancy is certainly empty. |
| `FanStopped` | Entire fan interval is at most 0.05. |

A certificate checks the current state and the next two 100 ms boundaries using existing command history. Its authority interval ends exclusively at the second boundary. A definite predicate violation at any checked boundary yields Invalid; mixed possibilities yield Unknown. Inconsistent model knowledge always yields Unknown, regardless of the numerical enclosure. Occupancy can therefore lose clearance immediately under unrestricted-entry assumptions, despite a recent empty-room reading.

These are physical-state claims over the stated window. They do not assert permission, candidate usefulness, successful actuation or observed completion. The existing separate candidate-delivery and recovery checks still cover action consequences over the longer action-specific horizons at admission, dispatch and release.

The model can carry an accepted historical observation forward after its direct TTL expires. It applies that observation at its generation time and propagates the state under declared bounds; it does not relabel the old observation as fresh. Expired newly arriving observations still supply no constraint. Widening uncertainty or contradiction can remove claim support.

## Certificate dependencies and authority invalidation

Each `PhysicalCertificate` records schema/model/configuration versions, evidence ID and version, estimator revision, predicate, checked time and validity end, affected phase contracts, current and initial enclosures, initial inputs, assumptions, observation identities/source versions/times/quality/values/errors and issued command IDs. All accepted observation messages are represented, including missing or explicit inconsistent quality. Command details remain available in the owning estimator history.

The estimator revision advances on time propagation, accepted observations and issued commands. Duplicates, superseded/expired arrivals, rejected input and no-op time advancement do not create knowledge revisions. Publication follows observation changes, each clock tick and each physical command release. Source changes renew certificate versions even when the verdict remains supported; relevant leases are revoked by the existing evidence-version mechanism. Both START and COMMIT leases are covered.

Related claims are precomputed and published in an atomic evidence batch before action lifecycle reactions. Certificates are renewed at each tick with a two-tick checked window, so ordinary renewal does not create a spurious expiry/recovery boundary. Unsupported new claims trigger the existing running-contract response. Recovery release is still independently checked and can be blocked.

Certificate history is retained in memory and available through read-only accessors. The evidence payload contains a trace reference, not a cryptographic hash. Durable certificate/observation/command export and browser presentation remain subsequent integration work.

## Acceptance evidence and remaining scope

The tests cover short-gap substitution, supported ties, conservative extra blocking, unchanged permission/outcome requirements, START/COMMIT lease invalidation, command dependencies, running contradiction, rejection of claim spoofing, non-mutating graph migration, provenance and renewal across multiple ticks. The full pre-existing runtime and estimator suites remain regression requirements.

A [closed-loop plant/scorer integration](closed-loop-demo.md) is now available. Explicitly versioned B5/B6 policy comparison with independently defined information-sufficiency and outcome labels remains a subsequent research boundary. Multiple recovery alternatives, durable traces, autonomous planning and Pi adapters remain separate work. This delivery does not resolve the known model-mismatch limitation or establish the project's 80% target.


Verification on 16 October 2026: 140 Rust unit/integration tests plus one
documentation test passed, including 13 physical-claim integration tests and three
claim-boundary tests. One existing manual replay test remained ignored. The 18
Python tests, Clippy with warnings denied, formatting, offline binary/example
builds and whitespace checks passed.
