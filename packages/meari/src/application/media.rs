use std::collections::BTreeMap;

pub trait MediaQueryEvaluator {
    fn matches_media(&mut self, query: &str) -> Result<bool, String>;
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct MediaChange {
    pub subscription: u64,
    pub matches: bool,
}

/// Explicit subscription lifetime for native responsive hooks. Poll only after
/// final layout commit, never during the host sizing measurement.
#[derive(Clone, Default)]
pub struct MediaSubscriptions {
    next: u64,
    entries: BTreeMap<u64, (String, bool)>,
}

impl MediaSubscriptions {
    pub fn subscribe(
        &mut self,
        evaluator: &mut dyn MediaQueryEvaluator,
        query: &str,
    ) -> Result<MediaChange, String> {
        let matches = evaluator.matches_media(query)?;
        let subscription = self
            .next
            .checked_add(1)
            .ok_or("media subscription generation exhausted")?;
        self.next = subscription;
        self.entries.insert(subscription, (query.into(), matches));
        Ok(MediaChange {
            subscription,
            matches,
        })
    }

    pub fn refresh(
        &mut self,
        evaluator: &mut dyn MediaQueryEvaluator,
    ) -> Result<Vec<MediaChange>, String> {
        let values: Result<Vec<_>, String> = self
            .entries
            .iter()
            .map(|(id, (query, _))| Ok((*id, evaluator.matches_media(query)?)))
            .collect();
        let mut changes = Vec::new();
        for (id, matches) in values? {
            let previous = &mut self
                .entries
                .get_mut(&id)
                .ok_or("media subscription disappeared")?
                .1;
            if *previous != matches {
                *previous = matches;
                changes.push(MediaChange {
                    subscription: id,
                    matches,
                });
            }
        }
        Ok(changes)
    }

    pub fn unsubscribe(&mut self, subscription: u64) -> bool {
        self.entries.remove(&subscription).is_some()
    }
}
