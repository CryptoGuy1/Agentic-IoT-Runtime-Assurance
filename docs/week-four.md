# Week 4: a building you can experiment with

Weeks 1–3 built the decision-making machinery. Week 4 gives that machinery a
small imaginary building to operate. Everything runs on your computer in Rust.
You do not need sensors, a microcontroller, an MQTT server, or an AI account.

The building can sound an alarm, start a fan, close a damper, close an isolation
door, arm suppression, and discharge suppression. An emergency light is included
to test two harmless actions happening together.

There are two separate views of the building:

- **What actually happens in the simulation.** For example, someone enters a room.
- **What the runtime has been told.** The occupancy sensor might be silent, late,
  or wrong. The runtime must make its decision from the observations it received.

Keeping those views separate lets us test whether the runtime wrongly permitted
an action. Rust does not make the simulated sensor truthful.

## Start here: control the simulation yourself

From the project directory, run:

```sh
cargo run --offline --bin sim -- --interactive
```

You will see a `sim>` prompt. Type these commands, one at a time:

```text
advance 0ms
status
advance 500ms
explain fan.run
fault pressure_safe drop
status
advance 200ms
status
```

Here is what that does:

1. `advance 0ms` delivers the initial sensor readings and starts the alarm.
2. Advancing 500 milliseconds lets the alarm finish and the fan start.
3. `explain fan.run` shows the evidence needed to keep the fan running.
4. Dropping `pressure_safe` tells the runtime it can no longer confirm that condition.
5. The fan enters `Aborting`. After another 200 milliseconds, the simulated abort
   controller has responded and the fan becomes `Cancelled`.

The clock is paused while you type. `advance 5s` moves five **simulated** seconds;
it does not ask the computer to sleep for five seconds.

## Other useful controls

| Command | Meaning |
| --- | --- |
| `help` | List the commands |
| `status` or `actions` | Show virtual time, simulated ground truth and action states |
| `evidence` | Show observations, versions and expiry times |
| `explain hazard` | Show a conclusion and its supporting witness |
| `events 10` | Show the last ten log entries |
| `step` | Process the next event timestamp, including other events at that time |
| `advance 500ms` or `run 5s` | Move forward by that amount |
| `fault s1 drop` | Make one hazard sensor unavailable until restored |
| `fault s1 restore` | Restore that sensor and provide a fresh observation |
| `sensor occupancy_clear unknown 2s` | Supply a one-off observation; later sampling can replace it |
| `plant occupancy occupied` | Put an occupant in zone 2; the runtime learns through later sensor sampling |
| `plant occupancy clear` | Clear zone 2 |
| `plant pressure 40` | Change simulated pressure; the minimum is 50 in this toy model |
| `plant egress blocked` / `plant egress clear` | Change the escape path |
| `plant hazard on` / `plant hazard off` | Change the building hazard |
| `ack alarm drop` / `ack alarm restore` | Suppress or restore acknowledgement in subsequent alarm results |
| `replan` | Replace unstarted work with fresh instances while retaining execution history |
| `save target/my-session` | Save reports and replay commands to a **new** directory |
| `quit` | Exit |

`Unknown` means the evidence is insufficient. `Invalid` means the declared
condition is contradicted. Neither grants permission to act. `Completed` means
both the correlated result and the outcome contract support completion.

## Run the prepared experiments

```sh
cargo run --offline --bin sim -- --scenario S1 --baseline B5 --seed 42 --out target/my-first-run
```

This runs ten simulated seconds and saves the results. Choose a new output
directory for each run. Use `--until 6s` for a different duration.

| Scenario | What to look for with B5 |
| --- | --- |
| S1 | Alarm, fan and door finish normally |
| S2 | One hazard sensor disappears; the other two preserve support |
| S3 | Occupancy evidence expires just before suppression commitment; no discharge |
| S4 | Communication with the agent is lost; the running fan finishes on local evidence, while the door cannot start |
| S5 | Sensor loss, packet loss and unavailable human approval; supported work continues |
| C11 | Fan and damper conflict; only one starts |
| C12 | Alarm and emergency light start together |
| C13 | Fan support loss gets a response in 180ms, inside the 500ms experiment margin |
| C14 | A deliberately slow response produces `ReactionDeadlineMiss` |
| C15 | A committed door finishes through its local controller after agent communication is lost |
| C16 | Replanning preserves the completed alarm and committed door, replacing the pending work |

**B5 is our complete assurance policy.** B0–B4 are comparison policies with
specific protections omitted. For example, run S3 with B0 to see why a stale
occupancy observation matters. The plant is allowed to demonstrate an unsafe
result; it does not quietly protect the weaker baselines.

## Save, inspect and replay

Each saved experiment contains:

| File | What it tells you |
| --- | --- |
| `manifest.json` | Scenario, seed, policy, code/configuration fingerprints and preset faults |
| `metrics.json` | Releases, unsupported releases, completion, replanning and reaction timing |
| `events.jsonl` | The ordered event and decision history |
| `workload.csv` | How much graph evaluation each update needed and its measured host elapsed time |
| `session.commands` | The commands needed to reproduce the session |
| `semantic.txt` | The final states, leases, commands and semantic history for exact comparison |

Replay a saved session:

```sh
cargo run --offline --bin sim -- --replay target/my-first-run/session.commands --out target/my-replay
```

The same code and session produce the same decisions and simulated results.
Measured host timings and the report creation timestamp naturally vary.

To run the complete software gate and save its test evidence:

```sh
python3 scripts/verify_week_four.py
```

The default report is `target/week-four-verification/gate-report.json`. If that
directory already exists, use `--out target/week-four-verification-2`. Python
only orchestrates verification and reads reports; the simulator remains Rust.

## Where the AI comes in

Week 4 uses a predictable scripted planner so we can reproduce mistakes and
compare policies fairly. The planner interface receives an observation estimate
and execution history. A later Python AI adapter can propose plans through this
boundary. Its proposals must still pass runtime checks before execution.

This milestone tests our software logic against a deliberately simple building
model. It does not certify real ventilation, suppression, evacuation or actuator
recovery procedures. Distributed fault sweeps and richer simulation follow in
Week 5. No quantum or post-quantum security mechanism is added here.

See [the technical reference](week-four-reference.md) for exact policies,
measurement definitions and modelling defaults.
