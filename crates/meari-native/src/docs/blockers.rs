use super::model::Receipt;
use serde::Serialize;
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Serialize)]
struct Group {
    status: String,
    reason: String,
    count: usize,
    sample_ids: Vec<usize>,
}

pub(super) fn report(receipts: &Path, output: &Path) -> Result<(), String> {
    let rows: Vec<Receipt> =
        serde_json::from_slice(&fs::read(receipts).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let span = regex::Regex::new(r"unsupported \[\d+\.\.\d+\]: ").map_err(|e| e.to_string())?;
    let mut groups: BTreeMap<(String, String), Group> = BTreeMap::new();
    for row in rows.into_iter().filter(|r| {
        matches!(
            r.status.as_str(),
            "frontend-blocked" | "native-blocked" | "execution-failed"
        )
    }) {
        let reason = row
            .detail
            .rsplit_once(". ")
            .map_or(row.detail.as_str(), |(_, reason)| reason);
        let reason = span.replace(reason, "").into_owned();
        let group = groups
            .entry((row.status.clone(), reason.clone()))
            .or_insert_with(|| Group {
                status: row.status,
                reason,
                count: 0,
                sample_ids: Vec::new(),
            });
        group.count += 1;
        if group.sample_ids.len() < 5 {
            group.sample_ids.push(row.id);
        }
    }
    let mut groups: Vec<_> = groups.into_values().collect();
    groups.sort_by(|a, b| b.count.cmp(&a.count).then(a.reason.cmp(&b.reason)));
    super::write_json(output, &groups)?;
    for group in groups.iter().take(25) {
        println!(
            "{} {}: {} {:?}",
            group.count, group.status, group.reason, group.sample_ids
        );
    }
    Ok(())
}
