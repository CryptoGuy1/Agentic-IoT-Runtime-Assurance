# Physical-claim replacement: controlled comparison

Schema: `physical-claims-v1`; configuration: `claim-fixtures-v1`.

Both paths use the same permission and outcome contracts and pressure readings. The direct comparator uses a point threshold and a 500 ms TTL. The model path additionally receives declared dynamics and error bounds, uses a propagated interval claim, and retains candidate/recovery checks. This compares the combined mechanism; it does not isolate the contribution of each feature or reproduce the historical B5/B6 campaign.

| Fixture | Direct evidence START | Model-backed START | Interpretation |
| --- | --- | --- | --- |
| Fresh safe reading | Allow | Allow | Both support the action. |
| Short sensor gap | Withhold | Allow | Expired direct evidence is replaced by a supported propagated claim. |
| Pressure 0.79 ± 0.03 | Allow | Withhold | The range includes pressure above 0.80; the model withholds. |

At 600 ms, the raw leaf is `Unknown` and the certificate is `Valid`. Certificate version 7, state revision 8, valid until 800 ms (exclusive). Source: `pressure-sensor`, version 1. Model: `evidra-model-v1.1`.

The short-gap command passes dispatch and physical release. No completion is inferred.

A supported model claim still cannot bypass denied permission.

These are deterministic contract fixtures, not independent physical scores, an 80% correctness result or evidence of policy superiority. Numerical states and time evolution remain conditional on the declared discrete model.

Reproduce: `cargo run --offline --example physical_claims`.
