# Step 3: using the model when a direct reading expires

Client review draft — 16 October 2026

The previous step added a physical-model check before actions could proceed. This step lets the model supply selected physical conditions that previously depended entirely on a fresh sensor reading.

For example, a pressure reading can expire while the model still has enough justified information to establish that pressure remains within its limit. The system can now use that model-backed condition while continuing to require permission and checking the proposed action and its recovery. The old reading remains marked expired.

Each condition has a versioned record showing the source readings, model assumptions, uncertainty, validity period and the action contracts it supports. Changed readings, commands or predicted state cause a new version to be published. Previously issued execution leases are invalidated when their supporting certificate changes.

The controlled examples show three results:

- With fresh, clearly safe information, both approaches permit the action.
- During a short sensor gap, the direct evidence gate withholds while the model-backed approach can proceed under its declared bounds.
- Near the pressure limit, the model-backed approach withholds when the uncertainty range includes an unsafe value, even if the reported point value looks acceptable.

The model cannot grant organizational permission or certify that a device completed its task. Those requirements remain separate. Conflicting model information removes support rather than creating a reassuring certificate.

This demonstrates the requested inference capability in software. It is not yet the full B5/B6 research comparison or an independently measured decision-correctness result. The next step is to connect these decisions to the complete simulated plant and independent scoring, then expose the results in the browser.

Please review whether the selected physical conditions, declared assumptions and permission boundaries match the intended application. Future physical deployments will need justified sensor and model bounds for the actual equipment.
