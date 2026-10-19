# Step 4: the demonstration now runs as one loop

Client review draft — 16 October 2026

The individual software parts now work together in an end-to-end simulation. Sensor messages update the model, the runtime checks an action, and a released command actually changes the simulated device. Device receipt and measured completion return as separate messages.

A second part of the experiment independently checks the recorded simulated world after the run. It reports physical targets, safety conditions and whether the runtime received confirmation. Its answers do not influence the runtime's decisions.

The walkthrough includes these examples:

- **Normal operation:** a fan starts, reaches its measured target and is confirmed complete.
- **Short sensor interruption:** the direct pressure reading expires, but a justified model-backed condition supports the action; execution still requires measured feedback.
- **Predicted pressure risk:** continued supervision requests a checked fan stop, then waits for measured recovery. It does not claim the original fan-start task succeeded.
- **Lost feedback:** the fan physically reaches its target, but the runtime does not mark the task complete because confirmation messages are missing.

Additional examples cover uncertainty near a limit, delayed commands and delayed readings. A long command delay can cause early recovery because it reduces the margin available to stop safely.

Each run saves a readable report and a detailed trace linking readings, model certificates, decisions, command delivery and feedback. These are reproducible software demonstrations under declared assumptions. No physical devices are connected yet.

The remaining presentation step is to connect this new loop to the browser dashboard. The full policy benchmark, independently measured decision correctness and Raspberry Pi integration remain separate work.


The [recorded demonstration results](closed-loop-results.md) provide the measured
simulation timings and links to the saved reports.
