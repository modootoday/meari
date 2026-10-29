use meari::{
    application::{
        media::MediaQueryEvaluator,
        responsive::{ResponsiveRenderer, ResponsiveSession},
    },
    domain::host::HostEnvironment,
};

fn environment(width: u32) -> HostEnvironment {
    serde_json::from_value(serde_json::json!({
        "target":"native", "layout":{"width":width,"height":600,"dark":false},
        "visual":{"width":width,"height":600,"offset_x":0,"offset_y":0,"scale":1},
        "device_scale":1.5,"layout_zoom":1.0,"safe_area":null,
        "capabilities":{"client_resize":"supported","preview_resize":"unsupported", "reduced_motion":"unknown","contrast":"unknown","pointer_hover":"unknown","accessibility":"unknown"}
    })).unwrap()
}

#[derive(Default)]
struct Renderer {
    width: u32,
    fail_query: bool,
    fail_restore: bool,
    trace: Vec<String>,
}
impl ResponsiveRenderer for Renderer {
    fn apply_environment(&mut self, environment: &HostEnvironment) -> Result<(), String> {
        self.trace
            .push(format!("layout:{}", environment.layout.width));
        self.width = environment.layout.width;
        if self.fail_restore && self.width == 800 {
            return Err("restore unavailable".into());
        }
        Ok(())
    }
}
impl MediaQueryEvaluator for Renderer {
    fn matches_media(&mut self, _: &str) -> Result<bool, String> {
        self.trace.push(format!("media:{}", self.width));
        if self.fail_query {
            return Err("query unavailable".into());
        }
        Ok(self.width >= 600)
    }
}

#[test]
fn media_publication_follows_final_layout_and_failed_update_retains_baseline() {
    let mut renderer = Renderer::default();
    let mut session = ResponsiveSession::new(environment(800)).unwrap();
    assert!(session.subscribe(&mut renderer, "wide").is_err());
    assert_eq!(
        session
            .apply(&mut renderer, environment(800))
            .unwrap()
            .revision,
        1
    );
    let subscription = session.subscribe(&mut renderer, "wide").unwrap();
    assert!(subscription.matches);
    renderer.trace.clear();
    renderer.fail_query = true;
    assert!(session.apply(&mut renderer, environment(400)).is_err());
    assert_eq!(renderer.trace, ["layout:400", "media:400", "layout:800"]);
    assert_eq!(renderer.width, 800);
    renderer.fail_query = false;
    let commit = session.apply(&mut renderer, environment(400)).unwrap();
    assert_eq!(commit.revision, 2);
    assert_eq!(commit.media_changes.len(), 1);
    assert_eq!(
        commit.media_changes[0].subscription,
        subscription.subscription
    );
    assert!(!commit.media_changes[0].matches);
    assert!(
        session
            .apply(&mut renderer, environment(400))
            .unwrap()
            .media_changes
            .is_empty()
    );
    assert!(session.unsubscribe(subscription.subscription));
    assert!(
        session
            .apply(&mut renderer, environment(800))
            .unwrap()
            .media_changes
            .is_empty()
    );
}

#[test]
fn observed_host_publication_is_atomic_without_applying_or_restoring_external_geometry() {
    let mut host = Renderer {
        width: 800,
        ..Renderer::default()
    };
    let mut session = ResponsiveSession::new(environment(800)).unwrap();
    assert!(session.subscribe_observed(&mut host, "wide").is_err());
    assert_eq!(
        session
            .observe(&mut host, environment(800))
            .unwrap()
            .revision,
        1
    );
    assert!(host.trace.is_empty());
    let subscription = session.subscribe_observed(&mut host, "wide").unwrap();
    host.trace.clear();
    host.width = 400;
    host.fail_query = true;
    assert!(session.observe(&mut host, environment(400)).is_err());
    assert_eq!(host.trace, ["media:400"]);
    assert_eq!(host.width, 400);
    host.fail_query = false;
    let commit = session.observe(&mut host, environment(400)).unwrap();
    assert_eq!(commit.revision, 2);
    assert_eq!(commit.media_changes.len(), 1);
    assert_eq!(
        commit.media_changes[0].subscription,
        subscription.subscription
    );
    assert!(!commit.media_changes[0].matches);
    let mut invalid = environment(800);
    invalid.device_scale = f64::NAN;
    assert!(session.observe(&mut host, invalid).is_err());
    assert_eq!(
        session
            .observe(&mut host, environment(400))
            .unwrap()
            .revision,
        3
    );
    assert!(session.unsubscribe(subscription.subscription));
    host.width = 800;
    assert!(
        session
            .observe(&mut host, environment(800))
            .unwrap()
            .media_changes
            .is_empty()
    );
}

#[test]
fn failed_rollback_requires_explicit_recovery_before_any_new_publication() {
    let mut renderer = Renderer::default();
    let mut session = ResponsiveSession::new(environment(800)).unwrap();
    session.apply(&mut renderer, environment(800)).unwrap();
    session.subscribe(&mut renderer, "wide").unwrap();
    renderer.fail_query = true;
    renderer.fail_restore = true;
    assert!(
        session
            .apply(&mut renderer, environment(400))
            .unwrap_err()
            .contains("rollback failed")
    );
    renderer.fail_query = false;
    assert!(session.apply(&mut renderer, environment(400)).is_err());
    assert!(session.observe(&mut renderer, environment(400)).is_err());
    assert!(session.recover(&mut renderer).is_err());
    renderer.fail_restore = false;
    session.recover(&mut renderer).unwrap();
    let commit = session.apply(&mut renderer, environment(400)).unwrap();
    assert_eq!(commit.revision, 2);
    assert_eq!(commit.media_changes.len(), 1);
    let before = renderer.trace.len();
    let mut invalid = environment(0);
    assert!(session.apply(&mut renderer, invalid.clone()).is_err());
    invalid.layout.width = 400;
    invalid.target = meari::domain::host::HostTarget::Web;
    assert!(session.apply(&mut renderer, invalid).is_err());
    assert_eq!(renderer.trace.len(), before);
}
