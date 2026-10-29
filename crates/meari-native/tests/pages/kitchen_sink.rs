use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, path::PathBuf};

fn corpus() -> PathBuf {
    std::env::var_os("MEARI_KITCHEN_SINK")
        .map(PathBuf::from)
        .expect("set MEARI_KITCHEN_SINK to the separately collected pinned corpus")
}

#[test]
#[ignore = "requires external official corpus; set MEARI_KITCHEN_SINK"]
fn pinned_official_inventory_has_no_missing_shadcn_payloads() {
    let root = corpus();
    let catalog: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("catalog.json")).unwrap()).unwrap();
    let pages = catalog["pages"].as_array().unwrap();
    let examples = catalog["examples"].as_array().unwrap();
    assert_eq!(
        pages.len(),
        396,
        "upstream census changes need explicit review"
    );
    assert_eq!(examples.len(), 4687);
    let mut providers = BTreeSet::new();
    for (id, example) in examples.iter().enumerate() {
        assert_eq!(example["id"].as_u64().unwrap(), id as u64);
        let page = &pages[example["page"].as_u64().unwrap() as usize];
        if page["project"] == "shadcn" {
            providers.insert(page["provider"].as_str().unwrap());
            assert_eq!(example["collection"], "collected");
            let code = fs::read(root.join(example["source"].as_str().unwrap())).unwrap();
            assert_eq!(example["sha256"], format!("{:x}", Sha256::digest(code)));
        } else {
            assert_eq!(example["collection"], "license-metadata-only");
            assert!(example["source"].is_null());
        }
    }
    assert_eq!(providers, BTreeSet::from(["aria", "base", "radix"]));
    assert!(root.join("SHADCN-LICENSE.txt").is_file());
}

#[test]
#[ignore = "requires external official corpus receipts; set MEARI_KITCHEN_SINK"]
fn every_inventory_item_has_an_honest_execution_receipt() {
    let root = corpus();
    let rows: Vec<serde_json::Value> =
        serde_json::from_slice(&fs::read(root.join("receipts/receipts.json")).unwrap()).unwrap();
    assert_eq!(rows.len(), 4687);
    for (id, row) in rows.iter().enumerate() {
        assert_eq!(row["id"].as_u64().unwrap(), id as u64);
        assert_ne!(row["status"], "pending-render");
        assert_ne!(row["status"], "collected");
        if row["status"] == "rendered-partial-validation" {
            assert!(
                root.join("receipts")
                    .join(row["screenshot"].as_str().unwrap())
                    .is_file()
            );
            assert!(
                row["interaction"]
                    .as_str()
                    .unwrap()
                    .contains("not verified")
            );
        }
    }
}
