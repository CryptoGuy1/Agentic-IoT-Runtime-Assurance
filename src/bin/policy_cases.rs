use dcra::model_v1::{
    BoundaryInformation as Info, DecisionBoundary as Phase, POLICY_SCHEMA, REFINEMENT_POLICIES,
    policy_response,
};
fn main() {
    let mut rows = vec![];
    for phase in [Phase::Start, Phase::Run, Phase::Commit, Phase::Outcome] {
        let base = Info {
            phase,
            phase_ready: true,
            permission: true,
            point_predicate: true,
            relevant_fresh: true,
            all_fresh: true,
            point_model: true,
            bounded_model: true,
            recovery_feasible: true,
            correlated_ack: true,
            observed_target: true,
        };
        // Independent reference responses are declared contract fixtures, not
        // computed from a policy's answer or from hidden experimental truth.
        let labels: &[(&str, bool, &str)] = if phase == Phase::Outcome {
            &[
                (
                    "measured-and-correlated",
                    true,
                    "Correlated measured target supports confirmation.",
                ),
                (
                    "missing-ack",
                    false,
                    "A measured effect without correlated acknowledgement is unconfirmed.",
                ),
                ("missing-target", false, "Receipt is not the target effect."),
                (
                    "wrong-phase",
                    false,
                    "A completion outside its applicable phase is not justified.",
                ),
                (
                    "permission-later-revoked",
                    true,
                    "Permission loss does not erase an already measured correlated effect.",
                ),
                (
                    "unrelated-channel-missing",
                    true,
                    "An unrelated missing source does not erase outcome evidence.",
                ),
                (
                    "direct-pressure-expired",
                    true,
                    "A direct pressure TTL does not invalidate separate correlated completion evidence.",
                ),
                (
                    "ack-without-effect",
                    false,
                    "Acknowledgement without a measured target cannot complete work.",
                ),
            ]
        } else {
            &[
                (
                    "bounded-safe",
                    true,
                    "Pressure possibilities are inside the limit; phase, permission and recovery hold.",
                ),
                (
                    "short-gap-bounded-safe",
                    true,
                    "Historical measurement propagated under declared bounds still establishes the condition.",
                ),
                (
                    "point-safe-range-unsafe",
                    false,
                    "0.79 plus or minus 0.03 admits values above 0.80.",
                ),
                (
                    "contradicted-pressure",
                    false,
                    "Pressure is certainly outside the declared limit.",
                ),
                (
                    "permission-denied",
                    false,
                    "Physical support does not grant permission.",
                ),
                (
                    "phase-prerequisite-missing",
                    false,
                    "Missing phase acknowledgement or prerequisite prevents authority.",
                ),
                (
                    "recovery-unavailable",
                    false,
                    "The specified robust reference requires a feasible recovery.",
                ),
                (
                    "unrelated-channel-missing",
                    true,
                    "A missing unrelated source is not a dependency of this action.",
                ),
            ]
        };
        for (index, (name, expected, reason)) in labels.iter().enumerate() {
            let mut i = base;
            if phase == Phase::Outcome {
                match index {
                    1 => i.correlated_ack = false,
                    2 | 7 => i.observed_target = false,
                    3 => i.phase_ready = false,
                    4 => i.permission = false,
                    5 => i.all_fresh = false,
                    6 => i.relevant_fresh = false,
                    _ => {}
                }
            } else {
                match index {
                    1 => i.relevant_fresh = false,
                    2 => i.bounded_model = false,
                    3 => {
                        i.point_predicate = false;
                        i.point_model = false;
                        i.bounded_model = false;
                    }
                    4 => i.permission = false,
                    5 => i.phase_ready = false,
                    6 => i.recovery_feasible = false,
                    7 => i.all_fresh = false,
                    _ => {}
                }
            }
            let responses = REFINEMENT_POLICIES
                .iter()
                .map(|p| format!("\"{}\":{}", p.id(), policy_response(*p, i)))
                .collect::<Vec<_>>()
                .join(",");
            rows.push(format!("{{\"id\":\"{phase:?}-{name}\",\"category\":\"{phase:?}\",\"expected\":{expected},\"reason\":\"{reason}\",\"information\":{{\"phase_ready\":{},\"permission\":{},\"point_predicate\":{},\"relevant_fresh\":{},\"all_fresh\":{},\"point_model\":{},\"bounded_model\":{},\"recovery_feasible\":{},\"correlated_ack\":{},\"observed_target\":{}}},\"responses\":{{{responses}}}}}",i.phase_ready,i.permission,i.point_predicate,i.relevant_fresh,i.all_fresh,i.point_model,i.bounded_model,i.recovery_feasible,i.correlated_ack,i.observed_target));
        }
    }
    println!(
        "{{\"schema\":\"{POLICY_SCHEMA}\",\"reference\":\"bounded-information-contract-fixtures-v1\",\"unit\":\"one explicitly defined boundary response\",\"population_inference\":false,\"cases\":[{}]}}",
        rows.join(",")
    );
}
