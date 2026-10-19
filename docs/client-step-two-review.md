# Step 2: connecting the model to action decisions

Client review draft — 16 October 2026

The first step gave the system a model of what could be happening when sensor information is incomplete or delayed. This step connects that model to the software that controls whether actions may proceed.

Before releasing a command, the system now checks both the existing permission rules and the model's prediction. It considers command delay and possible loss, and checks whether the configured recovery action could remain safe and reach its target if needed at the next monitoring point. It repeats these checks when execution starts or reaches a commitment point.

The system also keeps checking after a command starts. If continued operation is no longer supported, it requests recovery and checks the recovery command before releasing it. A task marked “complete” does not end supervision of an effect that continues, such as a running fan.

For example, a fan can be acceptable initially but create excessive pressure if it remains on with a closed door. The demonstration detects the predicted pressure problem and releases a checked fan-stop command. It also shows why stopping is not always the right recovery: if stopping conflicts with an active hazard, the system withholds an action that depends on that recovery plan.

| Review example | What the software demonstrates |
| --- | --- |
| Acceptable command with permission | Releases the command; waits for a separate outcome report before marking the task complete. |
| Predicted pressure problem during execution | Requests recovery and checks the recovery command before release. |
| Controller reports completion while an effect continues | Keeps monitoring and can release a separate recovery command. |
| Recovery may be lost or arrive too late | Withholds the action when it cannot establish the recovery target. |
| Proposed recovery conflicts with a hazard | Withholds the action and records the reason. |

These examples are reproducible software fixtures with declared assumptions. The completion-report example supplies a synthetic controller report to test continued supervision; it does not measure physical completion. Model predictions never create acknowledgements or outcome evidence.

This delivery establishes the integration and its tested behavior. It does not establish real-world safety, the 80% decision-correctness target, or superiority over competing approaches. The checks cover finite prediction windows and depend on the declared model, uncertainty bounds and recovery policy. Conservative withholding can prevent useful actions and must be measured in the later benchmark.

Please review whether these behaviors match the intended operation, particularly the recovery policy for each action and the requirement to keep supervising persistent effects. The next delivery is to expose these decisions and reasons in the browser, then evaluate matched policies against independently scored scenarios. Raspberry Pi inputs and outputs follow as a separate integration stage.
