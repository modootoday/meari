use meari::application::media::{MediaQueryEvaluator, MediaSubscriptions};

struct Host {
    matches: bool,
    fail: bool,
}
impl MediaQueryEvaluator for Host {
    fn matches_media(&mut self, query: &str) -> Result<bool, String> {
        if self.fail && query == "second" {
            Err("host unavailable".into())
        } else {
            Ok(self.matches)
        }
    }
}

#[test]
fn failed_refresh_is_atomic_and_unmounted_subscription_never_reappears() {
    let mut host = Host {
        matches: false,
        fail: false,
    };
    let mut store = MediaSubscriptions::default();
    let first = store.subscribe(&mut host, "first").unwrap();
    let second = store.subscribe(&mut host, "second").unwrap();
    host.matches = true;
    host.fail = true;
    assert!(store.refresh(&mut host).is_err());
    host.fail = false;
    let changes = store.refresh(&mut host).unwrap();
    assert_eq!(changes.len(), 2);
    assert!(changes.iter().all(|change| change.matches));
    assert!(store.refresh(&mut host).unwrap().is_empty());
    assert!(store.unsubscribe(first.subscription));
    host.matches = false;
    let changes = store.refresh(&mut host).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].subscription, second.subscription);
    let next = store.subscribe(&mut host, "first").unwrap();
    assert!(next.subscription > second.subscription);
}
