# Web simulation for the meeting

Run from the repository root:

```sh
python3 scripts/run_web_demo.py
```

Open http://127.0.0.1:8765 in your browser. Keep the terminal open. Ctrl+C stops
both the HTTP server and simulator. The launcher builds the bridge offline;
Rust supporting edition 2024 and Python 3 are required. No external packages,
accounts, internet connection or hardware are needed. The browser and server
share one simulation session. Refreshing the browser reads the current session;
Reset starts a new one. Time is paused between controls.

## Demonstration one

Choose **Loss of pressure evidence** and press Reset if needed.

1. Say: “This system checks whether an action still has the information it needs.”
2. Press **Start demonstration**. At 0.500s the alarm is completed, the fan action
   is running and the simulated fan is physically starting. The door is waiting.
3. Say: “The building state and the information available to the runtime are
   separate. Watch what happens when a reading becomes unavailable.”
4. Press the guided **Remove pressure evidence** button. Pressure stays at 100,
   but its evidence becomes Unknown. The fan action says **Stopping requested**.
5. Say: “Unknown does not mean dangerous. It means we cannot confirm the condition.
   Requesting a stop is not the same as confirming a stop.”
6. Press **Let the controller respond**. At 0.700s the fan action is Cancelled,
   the physical fan is idle and the door remains waiting.
7. Say: “The controller has confirmed the abort. The successor cannot proceed
   because the fan did not complete.”

Restoring evidence does not restart a cancelled action. Reset to repeat the demo.

## Demonstration two

Choose **Permission arrives too late**. The preset selection resets the session.
Press **Show delayed permission** to advance to 1.200s.

Say: “Permission has an expiry time. The network takes too long to deliver it, so
no action executes. A fresh permission may be travelling, but permission issued
is not the same as the device running.” Expand Technical view to inspect the
AuthorityRejected event. Reset to repeat.

## Scope and troubleshooting

This is the existing S1/B5/seed-42 Rust simulator, using reliable transport or
its stale-lease profile. It uses a scripted planner and a discrete plant model.
It is not a new benchmark campaign or a physical safety certification.

The browser illustrates truth only for the experiment observer; the decision
runtime continues to consume its existing delivered observations. Action cards
and evidence panels come from structured snapshots, never terminal-text parsing.
The fan animation follows physical fan_starting/fan_high, including while an
abort is pending, rather than the action label alone.

If the connection fails, press Reset. If the server is stopped, restart the launch
command and refresh the browser. If port 8765 is occupied, stop the existing demo
server before launching another. Only the listed demo controls are accepted.

Checks: cargo test --offline; cargo fmt --check;
cargo clippy --offline --all-targets -- -D warnings;
python3 scripts/test_web_demo.py (after building web_bridge).

## Exploring more sensors
Each observation card has Remove and Restore controls. Removing a reading makes it Unknown without changing the physical building. Restoring resumes observations of the actual simulated condition; it does not restart cancelled actions.

Thermal is a hazard-confirmation sensor in this model, not a temperature reading in degrees. B5 accepts any two of s1, s2 and thermal. Reset, remove Thermal, remove Hazard sensor 1, then advance 500 ms: the fan waits because only one hazard sensor remains. Restore Thermal: the second supporting reading lets the waiting fan start. Removing only Thermal leaves two readings and still permits the fan to start. Hazard evidence is a start requirement here; removing it after the fan starts does not by itself stop the fan.

Expand Change building conditions to set hazard present/absent, occupancy, exit blockage, or pressure (20/100 model units). These change physical truth immediately; advance 500 ms for normal sensor updates. A dropped sensor remains Unknown even when the building changes. For door demonstrations, block the exit or mark the room occupied before the fan finishes, advance time to deliver observations, and inspect Door requirements. Clear the condition and advance again to allow a still-waiting door to proceed.
