# Software refinement delivery

19 October 2026

The software work now covers a browser demonstration, a versioned seven-policy
benchmark, ordered recovery alternatives and bounded autonomous replanning.
Hardware purchasing and installation can proceed separately.

## Open the dashboard

```sh
python3 scripts/run_web_demo.py
```

Open `http://127.0.0.1:8765/model`. The original demonstration remains at `/`.
No new packages, remote AI account or hardware are required. Each Run computes
a fresh Rust experiment; Play, Next 100 ms, First decision and the timeline
replay its actual recorded trajectory. This is explicit recorded playback,
not a claim that a physical runtime is operating in wall-clock real time.

The page shows actual and estimated fan/pressure, delivered sensor ages,
physical certificates, action history, release/receipt/measurement/confirmation
and post-run independent scores. It can compare every refinement-v1 policy.
Run data can be downloaded and is also saved under `output/model-dashboard/`.
The local HTTP service validates parameters, admits at most two concurrent
experiments and keeps the older Ollama planner lock separate from these runs.

For a client walkthrough:

1. Run **Short pressure sensor gap** with B6, jump to the first decision, and
   inspect expired direct pressure information alongside a valid model claim.
2. Compare the same scenario and seed with B5 to see the declared direct-evidence
   policy withhold the initial action. These policy IDs belong to refinement-v1.
3. Run **Ventilation needs another recovery**, enable alternative door-open
   recovery and inspect the rejected fan-stop option and selected door opening.
   The configured experiment declares bounded healthy door behavior. The model
   does not infer perfect device health from an ordinary successful reading.
4. Run **Open the door and replan**, enable autonomous replanning, and play to
   the end. The original fan task remains Cancelled; new door-opening and fan
   tasks complete. Feedback still gates predecessor completion.
5. Run **Physical success without confirmation** to distinguish physical effects,
   task confirmation and later timeout/recovery.

## Recovery and replanning

Before registering plans, an application may explicitly authorize up to four
unique recovery alternatives per action definition. The primary recovery is
tried first. The runtime checks each option for safety and target feasibility
under command timing and the current state enclosure, both before admission
and at actual recovery release. Every attempted option is audited. If no option
is supported, release remains blocked and the unresolved condition stays visible;
no generic stop is assumed safe.

The deterministic local goal supervisor uses only delivered model knowledge,
permission and instance history. It can retry pending work after information
changes and, after a measured cancellation, install one new DoorOpen → FanStart
plan with fresh instance IDs. It cannot rewrite started work or fabricate an
outcome. Its budget is four proposals including the initial proposal, a 500 ms
cooldown, one new plan and a 12-second proposal deadline. Completed goals stop
planning. This is a bounded ventilation-domain planner, not a new general LLM
agent. The earlier Qwen demonstration remains a separate optional path.

Superseded device commands cannot receive target credit from a later command.
Late acknowledgement/completion packets are rejected by the existing correlation
checks. A standalone persistent-effect recovery can have measured confirmation
without manufacturing a completed action instance.

## Versioned policy benchmarks

The [frozen protocol](refinement-benchmark-protocol.md) defines all seven policies,
phase rules, information budgets, denominators, primary contrasts and seeds.
Historical Week 5 B0–B5 results remain unchanged.

```sh
cargo build --offline --bins
python3 scripts/run_refinement_benchmarks.py --out output/NEW_BENCHMARK_DIRECTORY
```

The complete campaign is 1,176 closed-loop runs: seven policies × seven scenarios
× 24 seeds. Seeds 0–7 are pilot; seeds 8–23 are confirmatory. There are also 32
explicit information-contract cases per policy, eight each for START, RUN,
COMMIT and OUTCOME. Correlated outcome handling is retained by every main policy.

The runner writes the source/protocol hash before collection and refuses to
continue if it changes. It saves raw counts, one summary and replay command per
run, finite-corpus agreement by category, paired confirmatory seed-block
contrasts and uncertainty intervals. Zero-event uncertainty is reported; zero
observed violations do not mean zero underlying risk. No non-inferiority claim
is made. Matched initial conditions and exogenous draws are retained while
policy-induced physical trajectories may diverge.

Main comparisons exclude autonomous replanning and additional recoveries; those
are explicit separate intervention experiments. The benchmark uses a 750 ms
pressure TTL and 12-second runs, which cover persistent effects after the
initial action window. The historical closed-loop demonstration retains its
original 500 ms TTL and eight-second duration when no comparison flags are used.

Only a successfully completed campaign updates `output/refinement-latest.json`,
which supplies the dashboard benchmark table. Every row can be reconstructed
with the self-contained `frozen-source/` archive, seed and replay arguments.
The archive manifest is checksum-verified against the pre-collection freeze. Complete traces are generated
on replay rather than duplicating large JSON files for every campaign row.

## What the results can establish

The 32 cases are hand-authored conformance fixtures with declared reference
responses. Agreement on them is not a population-level accuracy estimate or
the general 80 percent milestone. Actual closed-loop scoring independently
measures safety, targets, confirmation and final task objectives; it does not
supply a separate information-justification label for every live decision.

The software campaign supports inspection of this fixed domain and scenario
mix. A broader research claim still requires a representative independently
labelled corpus, additional objectives and domains, outcome-verification and
other targeted ablations, justified information assumptions, and confirmatory
study design appropriate to those claims. Physical performance, continuous-time
behavior and calibrated bounds require the hardware testbed.


## Recorded validation

The [validated campaign](../output/refinement-benchmarks-20261019-validated/report.md)
contains all 1,176 runs. Read its summary.json for uncertainty intervals and per-case
labels. The frozen source is preserved inside that campaign directory. This
separates a reproducible experimental revision from later report/presentation
edits in the working tree. No model thresholds or benchmark scenarios were tuned
between pilot and confirmation.

The software checks passed 157 Rust unit/integration tests plus the compile-fail
documentation test, 24 Python tests, formatting and Clippy. One existing manual
replay test remained ignored. Browser checks exercised fresh experiment generation,
responsive controls, playback to completion, preserved cancelled/new task history,
charts and independent score display.
