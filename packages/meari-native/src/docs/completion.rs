use super::model::{Catalog, CheckStatus, Receipt};
use serde::Serialize;
use std::collections::BTreeMap;

pub(super) const REQUIRED: &[&str] = &[
    "source-semantics",
    "roundtrip",
    "native-render",
    "native-interaction",
    "os-input",
    "os-accessibility",
    "visual-parity",
    "responsive-dark-rtl-motion",
];

#[derive(Debug, Serialize)]
pub(super) struct Completion {
    pub full_acceptance: bool,
    pub total_examples: usize,
    pub receipt_count: usize,
    pub incomplete_by_gate: BTreeMap<String, usize>,
    pub inventory_gaps: Vec<String>,
}

pub(super) fn evaluate(catalog: &Catalog, receipts: &[Receipt]) -> Completion {
    let mut incomplete = BTreeMap::new();
    for example in &catalog.examples {
        let receipt = receipts.iter().find(|row| row.id == example.id);
        for gate in REQUIRED {
            let passed = receipt.is_some_and(|row| {
                let matches: Vec<_> = row
                    .checks
                    .iter()
                    .filter(|check| check.gate == *gate)
                    .collect();
                matches.len() == 1 && matches[0].status == CheckStatus::Passed
            });
            if !passed {
                *incomplete.entry((*gate).into()).or_default() += 1;
            }
        }
    }
    // These are observed collection gaps, not an overrideable CLI success flag.
    let inventory_gaps = vec![
        "Tailwind shared/generated example transclusions are not fully enumerated".into(),
        "Tailwind code entries need source-owned runnable fixtures and explicit scenarios".into(),
        "Reference/configuration snippets need reviewed feature-to-scenario mappings".into(),
    ];
    Completion {
        full_acceptance: incomplete.is_empty()
            && inventory_gaps.is_empty()
            && receipts.len() == catalog.examples.len(),
        total_examples: catalog.examples.len(),
        receipt_count: receipts.len(),
        incomplete_by_gate: incomplete,
        inventory_gaps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn screenshots_and_partial_receipts_cannot_satisfy_all_feature_acceptance() {
        let catalog: Catalog = serde_json::from_value(serde_json::json!({
            "schema":1,"sources":[],"pages":[],"examples":[{
                "id":0,"page":0,"line":1,"kind":"owned","language":"tsx",
                "name":"partial","source":null,"sha256":null,"collection":"owned"
            }]
        }))
        .unwrap();
        let receipts: Vec<Receipt> = serde_json::from_value(serde_json::json!([{
            "id":0,"status":"rendered-partial-validation","detail":"owned fixture",
            "screenshot":"capture.png","interaction":"not verified","accessibility":"not verified",
            "checks":[{"gate":"native-render","status":"passed","detail":"capture exists"}]
        }]))
        .unwrap();
        let result = evaluate(&catalog, &receipts);
        assert!(!result.full_acceptance);
        assert!(result.incomplete_by_gate["os-input"] > 0);
        assert!(!result.inventory_gaps.is_empty());
    }
}
