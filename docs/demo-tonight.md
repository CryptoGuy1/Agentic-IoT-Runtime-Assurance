# Tonight's demo: one fan, one decision

## What you are demonstrating

**We are building software that checks whether an automated action still has the evidence it needs.**

For this demo, the action is running a ventilation fan. The system needs evidence
that the fan is healthy and that pressure is acceptable while it runs.

If the pressure reading disappears, the system does not assume pressure is safe.
It requests an abort and waits for the simulated controller to confirm it.

Everything here is simulated. No physical building or real fan is connected.

## Your opening explanation

> “Imagine a building that can automatically switch on a ventilation fan.
> We don't want it to keep operating just because it received permission earlier.
> Our software checks the evidence required at each stage. I'll show it starting
> the fan, losing a required sensor reading, and handling the stop.”

## Start the program

Open a terminal in the project folder and run:

```sh
cargo run --offline --bin sim -- --scenario S1 --interactive
```

You should see `sim>`. The simulation waits for your commands.

The output includes a long building-state line. For this demo, focus on the three
short action lines at the bottom: alarm, fan and door.

## Step 1 — Let the alarm finish and the fan start

Type:

```text
advance 500ms
```

Expected action lines:

```text
v1.0.alarm: Completed
v1.1.fan: Executing
v1.2.door: Pending
```

Say:

> “The alarm has finished, and the fan is running. The door is waiting for its turn.”

`advance` moves simulated time. It does not wait for that amount of real time.

## Step 2 — Make the pressure reading unavailable

Type:

```text
fault pressure_safe drop
```

Look for:

```text
v1.1.fan: Aborting
```

Say:

> “I have removed the pressure reading. This doesn't tell us that pressure is
> dangerous; it tells us that we can no longer confirm it is acceptable. The
> system requests a stop. It has not yet claimed that stopping succeeded.”

## Step 3 — Let the simulated controller respond

Type:

```text
advance 200ms
```

Look for:

```text
v1.1.fan: Cancelled
```

Say:

> “The simulated controller has now confirmed the abort. The fan action is
> cancelled, and the next action remains blocked because the fan did not complete.”

Finish with:

> “This is the distinction we're researching: current evidence controls execution,
> and requesting an action is different from proving that it finished.”

Type `quit` to exit. The program prints additional metrics; you do not need to
present those in this short demonstration.

## What Weeks 1–5 contributed

| Week | Simple explanation |
| --- | --- |
| 1 | Give conditions three answers: supported, unknown, or contradicted |
| 2 | Track which readings support each answer and update the affected conclusions |
| 3 | Use those answers to control starting, continuing, committing and finishing actions |
| 4 | Put the system inside a simulated building you can interact with |
| 5 | Make messages late, missing, duplicated or out of order and test what happens |

## Optional second demonstration — Permission arrives too late

Only add this if the first demonstration feels comfortable.

Restart with:

```sh
cargo run --offline --bin sim -- --scenario S1 --network stale-lease --interactive
```

Type:

```text
advance 1200ms
status
```

No action should have started. An action may say `Leased` because a fresh request
is travelling; that is not the same as `Executing`.

Say:

> “A permission has a time limit. Here the network takes longer than that limit
> to deliver it. The gateway rejects the expired permission instead of starting
> the action. This also exposes a limitation: a sufficiently slow network can
> prevent useful work. We record that cost.”

This demonstrates Week 5's communication boundary. It does not demonstrate AI.
The current planner uses a fixed script; the AI planner is the next milestone.
