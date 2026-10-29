use super::*;
#[test]
fn initial_real_events_publish_once_only_after_same_window_ready() {
    let window = WindowId::from_raw(7);
    let mut activity = InputActivity::new(1);
    activity
        .record_pending(
            window,
            WindowInputKind::FocusGained,
            Duration::from_millis(1),
        )
        .unwrap();
    activity
        .record_pending(
            window,
            WindowInputKind::PointerEntered,
            Duration::from_millis(2),
        )
        .unwrap();
    activity
        .record_pending(
            window,
            WindowInputKind::PointerMotion,
            Duration::from_millis(3),
        )
        .unwrap();
    assert_eq!(activity.latest(scope(1)).unwrap(), None);
    activity.invalidate(ActionError::NotReady);
    assert!(activity.publish_initial(scope(1), window).unwrap());
    let original = activity.latest(scope(1)).unwrap().unwrap();
    assert_eq!(original.sequence, 3);
    assert_eq!(original.observed_at, Duration::from_millis(3));
    assert_eq!(original.focused, Some(true));
    assert_eq!(original.pointer_inside, Some(true));
    assert!(!activity.publish_initial(scope(1), window).unwrap());
    assert_eq!(activity.latest(scope(1)).unwrap(), Some(original));
    activity
        .record(scope(1), WindowInputKind::Key, Duration::from_millis(4))
        .unwrap();
    assert_eq!(activity.latest(scope(1)).unwrap().unwrap().sequence, 4);
    assert_eq!(
        activity.latest(scope(1)).unwrap().unwrap().focused,
        Some(true)
    );
}
#[test]
fn initial_facts_do_not_cross_foreign_host_or_replaced_window() {
    let first = WindowId::from_raw(7);
    let second = WindowId::from_raw(8);
    let mut activity = InputActivity::new(1);
    activity
        .record_pending(first, WindowInputKind::FocusGained, Duration::ZERO)
        .unwrap();
    assert_eq!(
        activity.record_pending(second, WindowInputKind::PointerEntered, Duration::ZERO),
        Err(ActionError::StaleWindow)
    );
    let mut foreign = scope(1);
    foreign.host = 2;
    assert_eq!(
        activity.publish_initial(foreign, first),
        Err(ActionError::ForeignHost)
    );
    assert_eq!(
        activity.publish_initial(scope(2), second),
        Err(ActionError::StaleWindow)
    );
    assert_eq!(activity.latest(scope(2)).unwrap(), None);
    assert_eq!(
        activity.record_pending(first, WindowInputKind::Key, Duration::ZERO),
        Err(ActionError::NotReady)
    );
}
#[test]
fn suspend_and_close_discard_initial_facts_without_resume_inheritance() {
    let window = WindowId::from_raw(7);
    let mut activity = InputActivity::new(1);
    activity
        .record_pending(window, WindowInputKind::FocusGained, Duration::ZERO)
        .unwrap();
    activity.retire();
    assert!(!activity.publish_initial(scope(2), window).unwrap());
    assert_eq!(
        activity.record_pending(window, WindowInputKind::PointerEntered, Duration::ZERO),
        Err(ActionError::NotReady)
    );
    activity
        .record(scope(2), WindowInputKind::PointerMotion, Duration::ZERO)
        .unwrap();
    let actual = activity.latest(scope(2)).unwrap().unwrap();
    assert_eq!(actual.focused, None);
    assert_eq!(actual.pointer_inside, None);
    let mut closing = InputActivity::new(1);
    closing
        .record_pending(window, WindowInputKind::FocusGained, Duration::ZERO)
        .unwrap();
    closing.invalidate(ActionError::Closed);
    assert_eq!(
        closing.publish_initial(scope(1), window),
        Err(ActionError::Closed)
    );
    assert_eq!(closing.latest(scope(1)), Err(ActionError::Closed));
}
#[test]
fn pending_counter_and_time_errors_do_not_replace_original_event() {
    let window = WindowId::from_raw(7);
    let mut activity = InputActivity::new(1);
    activity
        .record_pending(window, WindowInputKind::FocusGained, Duration::from_secs(1))
        .unwrap();
    assert_eq!(
        activity.record_pending(window, WindowInputKind::FocusLost, Duration::ZERO),
        Err(ActionError::StaleObservation)
    );
    activity.sequence = u64::MAX;
    assert_eq!(
        activity.record_pending(window, WindowInputKind::FocusLost, Duration::from_secs(2)),
        Err(ActionError::CounterExhausted)
    );
    assert!(activity.publish_initial(scope(1), window).unwrap());
    let original = activity.latest(scope(1)).unwrap().unwrap();
    assert_eq!(original.sequence, 1);
    assert_eq!(original.focused, Some(true));
    assert_eq!(original.observed_at, Duration::from_secs(1));
}
use crate::domain::window_lifecycle::WindowStamp;
fn scope(surface: u64) -> ActionScope {
    ActionScope {
        host: 1,
        window: WindowStamp {
            owner: 2,
            epoch: 1,
            surface,
        },
    }
}
#[test]
fn actual_event_mapping_does_not_manufacture_activity_for_render_or_close() {
    assert_eq!(
        kind(&WindowEvent::Focused(true)),
        Some(WindowInputKind::FocusGained)
    );
    assert_eq!(
        kind(&WindowEvent::Focused(false)),
        Some(WindowInputKind::FocusLost)
    );
    assert_eq!(kind(&WindowEvent::RedrawRequested), None);
    assert_eq!(kind(&WindowEvent::CloseRequested), None);
}
#[test]
fn coalesced_activity_retains_focus_inside_and_monotonic_sequence() {
    let mut activity = InputActivity::new(1);
    assert_eq!(activity.latest(scope(1)).unwrap(), None);
    for (index, kind) in [
        WindowInputKind::FocusGained,
        WindowInputKind::PointerEntered,
        WindowInputKind::PointerMotion,
        WindowInputKind::FocusLost,
        WindowInputKind::PointerLeft,
        WindowInputKind::Key,
    ]
    .into_iter()
    .enumerate()
    {
        activity
            .record(scope(1), kind, Duration::from_millis(index as u64))
            .unwrap();
    }
    let latest = activity.latest(scope(1)).unwrap().unwrap();
    assert_eq!(latest.sequence, 6);
    assert_eq!(latest.kind, WindowInputKind::Key);
    assert_eq!(latest.focused, Some(false));
    assert_eq!(latest.pointer_inside, Some(false));
    assert_eq!(activity.latest(scope(1)).unwrap(), Some(latest));
}
#[test]
fn activity_scope_replacement_does_not_reuse_original_metadata() {
    let mut activity = InputActivity::new(1);
    activity
        .record(scope(1), WindowInputKind::FocusGained, Duration::ZERO)
        .unwrap();
    assert_eq!(activity.latest(scope(2)), Err(ActionError::StaleWindow));
    let mut foreign = scope(1);
    foreign.host = 2;
    assert_eq!(
        activity.record(foreign, WindowInputKind::Key, Duration::ZERO),
        Err(ActionError::ForeignHost)
    );
    activity
        .record(scope(2), WindowInputKind::PointerMotion, Duration::ZERO)
        .unwrap();
    let latest = activity.latest(scope(2)).unwrap().unwrap();
    assert_eq!(latest.sequence, 2);
    assert_eq!(latest.focused, None);
    assert_eq!(latest.pointer_inside, None);
    assert_eq!(activity.latest(scope(1)), Err(ActionError::StaleWindow));
}
#[test]
fn close_and_counter_overflow_preserve_bounded_activity_failure() {
    let mut activity = InputActivity::new(1);
    activity
        .record(scope(1), WindowInputKind::Key, Duration::from_secs(1))
        .unwrap();
    let previous = activity.latest(scope(1)).unwrap();
    assert_eq!(
        activity.record(scope(1), WindowInputKind::Key, Duration::ZERO),
        Err(ActionError::StaleObservation)
    );
    assert_eq!(activity.latest(scope(1)).unwrap(), previous);
    activity.sequence = u64::MAX;
    assert_eq!(
        activity.record(scope(1), WindowInputKind::Key, Duration::from_secs(1)),
        Err(ActionError::CounterExhausted)
    );
    assert_eq!(activity.latest(scope(1)).unwrap(), previous);
    activity.invalidate(ActionError::Closing);
    assert_eq!(
        activity.record(scope(1), WindowInputKind::Key, Duration::from_secs(2)),
        Err(ActionError::Closing)
    );
    assert_eq!(activity.latest(scope(1)), Err(ActionError::Closing));
}
