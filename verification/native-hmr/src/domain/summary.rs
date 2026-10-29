use super::Report;
use std::collections::BTreeSet;

impl Report {
    pub fn summary(&self) -> serde_json::Value {
        let mut timings: Vec<_> = self
            .builds
            .iter()
            .filter(|b| (2..=21).contains(&b.generation) && b.success)
            .map(|b| b.elapsed_ms)
            .collect();
        timings.sort_unstable();
        let presented: Vec<_> = self
            .builds
            .iter()
            .filter(|b| (2..=21).contains(&b.generation) && b.success)
            .filter_map(|build| {
                self.receipts
                    .iter()
                    .find(|r| {
                        r.status == "reloaded"
                            && r.rendered
                                .as_ref()
                                .is_some_and(|f| f.generation == build.generation)
                    })
                    .map(|receipt| build.elapsed_ms + receipt.request_receipt_ms)
            })
            .collect();
        let identities: BTreeSet<_> = self
            .receipts
            .iter()
            .map(|r| (r.telemetry.pid, r.telemetry.hwnd))
            .collect();
        let memory: Vec<_> = self
            .receipts
            .iter()
            .map(|r| r.telemetry.working_set_bytes)
            .collect();
        serde_json::json!({
            "compatible_code_swaps": self.compatible_code_swaps,
            "identity_count": identities.len(),
            "identities": identities,
            "rendered_receipts": self.receipts.iter().filter(|r| r.screenshot.is_some()).count(),
            "old_modules_unlocked": self.old_modules_unlocked,
            "owned_host_exited": self.owned_host_exited,
            "final_module_unlocked": self.final_module_unlocked,
            "rustc_swap_build_ms": latency_stats(timings),
            "build_plus_presented_receipt_ms": latency_stats(presented),
            "working_set_bytes": { "first": memory.first(), "last": memory.last(), "min": memory.iter().min(), "max": memory.iter().max() },
            "scope": self.scope
        })
    }
}

fn latency_stats(mut values: Vec<u128>) -> serde_json::Value {
    values.sort_unstable();
    serde_json::json!({
        "count": values.len(), "min": values.first(), "max": values.last(),
        "median": if values.is_empty() {0} else {(values[(values.len()-1)/2] + values[values.len()/2])/2},
        "mean": if values.is_empty() {0} else {values.iter().sum::<u128>() / values.len() as u128}
    })
}
