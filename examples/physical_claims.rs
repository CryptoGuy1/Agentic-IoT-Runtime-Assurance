//! Controlled mechanism comparison; not the historical B5/B6 policy campaign.
#[path = "support/physical_claims.rs"]
mod support;
use dcra::actions::ActionPhase;
use dcra::evidence::AssuranceStatus as Status;
use dcra::model_v1::CLAIM_SCHEMA;
use support::*;
fn main() {
    println!("# Physical-claim replacement: controlled comparison\n");
    println!("Schema: `{CLAIM_SCHEMA}`; configuration: `claim-fixtures-v1`.\n");
    println!(
        "Both paths use the same permission and outcome contracts and pressure readings. The direct comparator uses a point threshold and a 500 ms TTL. The model path additionally receives declared dynamics and error bounds, uses a propagated interval claim, and retains candidate/recovery checks. This compares the combined mechanism; it does not isolate the contribution of each feature or reproduce the historical B5/B6 campaign.\n"
    );
    println!("| Fixture | Direct evidence START | Model-backed START | Interpretation |");
    println!("| --- | --- | --- | --- |");
    for (name, value, at, meaning) in [
        ("Fresh safe reading", 0.0, 0, "Both support the action."),
        (
            "Short sensor gap",
            0.0,
            600,
            "Expired direct evidence is replaced by a supported propagated claim.",
        ),
        (
            "Pressure 0.79 ± 0.03",
            0.79,
            0,
            "The range includes pressure above 0.80; the model withholds.",
        ),
    ] {
        let mut baseline = direct(value);
        let mut candidate = model(value);
        baseline.advance_to(ms(at)).unwrap();
        candidate.advance_to(ms(at)).unwrap();
        let direct_allowed = baseline.request_lease("a", ActionPhase::Start).is_ok();
        let model_allowed = candidate.request_lease("a", ActionPhase::Start).is_ok();
        println!(
            "| {name} | {} | {} | {meaning} |",
            if direct_allowed { "Allow" } else { "Withhold" },
            if model_allowed { "Allow" } else { "Withhold" }
        );
    }
    let mut candidate = model(0.0);
    candidate.advance_to(ms(600)).unwrap();
    let token = candidate.request_lease("a", ActionPhase::Start).unwrap();
    let certificate = candidate.certificate("pressure.model").unwrap();
    println!(
        "\nAt 600 ms, the raw leaf is `{:?}` and the certificate is `{:?}`. Certificate version {}, state revision {}, valid until {} ms (exclusive). Source: `{}`, version {}. Model: `{}`.\n",
        candidate.runtime().assurance().evidence()["pressure.direct"].status,
        certificate.status,
        certificate.version,
        certificate.state_revision,
        certificate.valid_until.as_millis(),
        certificate.observations[0].source,
        certificate.observations[0].version,
        certificate.model_version
    );
    assert_eq!(certificate.status, Status::Valid);
    candidate.dispatch("a", ActionPhase::Start, &token).unwrap();
    assert!(candidate.take_command().unwrap().is_some());
    println!(
        "The short-gap command passes dispatch and physical release. No completion is inferred.\n"
    );
    let mut denied = model(0.0);
    denied
        .update_permission(atom("permit", Status::Invalid, 2, ms(20000)))
        .unwrap();
    assert!(denied.request_lease("a", ActionPhase::Start).is_err());
    println!("A supported model claim still cannot bypass denied permission.\n");
    println!(
        "These are deterministic contract fixtures, not independent physical scores, an 80% correctness result or evidence of policy superiority. Numerical states and time evolution remain conditional on the declared discrete model.\n"
    );
    println!("Reproduce: `cargo run --offline --example physical_claims`.");
}
