use super::*;
#[test]
fn cursor_setter_completion_is_terminal_applied_not_os_observed() {
    let mut model = ready(2);
    let admitted = model
        .admit(scope(1), id(1), WindowAction::SetCursorVisible(false))
        .unwrap();
    assert_eq!(
        model.applied(admitted.record.ticket),
        Err(ActionError::StaleRequest)
    );
    let issued = model.next(scope(1)).unwrap().unwrap();
    model
        .observe(
            scope(1),
            capabilities(),
            state(Some(false)),
            Duration::from_millis(1),
        )
        .unwrap();
    assert_eq!(model.record(id(1)).unwrap().status, ActionStatus::Issued);
    model.applied(issued.ticket).unwrap();
    let result = model.take_result(id(1)).unwrap();
    assert_eq!(result.status, ActionStatus::Applied);
    assert_eq!(result.observation, None);
    assert_eq!(result.observed_at, None);
    assert_eq!(model.take_result(id(1)), None);
}
#[test]
fn cursor_application_cannot_complete_foreign_stale_or_closed_request() {
    let mut model = ready(2);
    model
        .admit(scope(1), id(1), WindowAction::SetCursorVisible(false))
        .unwrap();
    let issued = model.next(scope(1)).unwrap().unwrap();
    let mut foreign = issued.ticket;
    foreign.scope.host = 2;
    assert_eq!(model.applied(foreign), Err(ActionError::ForeignHost));
    model
        .observe(
            scope(2),
            capabilities(),
            state(Some(false)),
            Duration::from_millis(1),
        )
        .unwrap();
    assert_eq!(model.applied(issued.ticket), Err(ActionError::StaleWindow));
    model.invalidate(ActionError::Closed);
    assert_eq!(model.applied(issued.ticket), Err(ActionError::Closed));
}
#[test]
fn cursor_completion_does_not_replace_fullscreen_observation_contract() {
    let mut model = ready(2);
    model
        .admit(scope(1), id(1), WindowAction::SetFullscreen(true))
        .unwrap();
    let issued = model.next(scope(1)).unwrap().unwrap();
    assert_eq!(model.applied(issued.ticket), Err(ActionError::StaleRequest));
    assert_eq!(model.record(id(1)).unwrap().status, ActionStatus::Issued);
}
use crate::domain::window_lifecycle::WindowStamp;

fn scope(surface: u64) -> ActionScope {
    ActionScope {
        host: 9,
        window: WindowStamp {
            owner: 7,
            epoch: 1,
            surface,
        },
    }
}
fn id(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value).unwrap()
}
fn capabilities() -> ActionCapabilities {
    ActionCapabilities {
        minimize: true,
        maximize: true,
        borderless_fullscreen: true,
    }
}
fn state(maximized: Option<bool>) -> WindowObservation {
    WindowObservation {
        minimized: Some(false),
        maximized,
        borderless_monitor_coverage: Some(false),
    }
}
fn ready(capacity: usize) -> WindowActions {
    let mut model = WindowActions::new(9, capacity).unwrap();
    model
        .observe(scope(1), capabilities(), state(Some(false)), Duration::ZERO)
        .unwrap();
    model
}

#[test]
fn admission_and_issue_are_not_observation_or_causal_os_ack() {
    let mut model = WindowActions::new(9, 2).unwrap();
    assert_eq!(
        model.admit(scope(1), id(1), WindowAction::SetMaximized(true)),
        Err(ActionError::NotReady)
    );
    model
        .observe(scope(1), capabilities(), state(None), Duration::ZERO)
        .unwrap();
    let admission = model
        .admit(scope(1), id(1), WindowAction::SetMaximized(true))
        .unwrap();
    assert_eq!(admission.record.status, ActionStatus::Queued);
    assert_eq!(
        model.next(scope(1)).unwrap().unwrap().status,
        ActionStatus::Issued
    );
    model
        .observe(
            scope(1),
            capabilities(),
            state(None),
            Duration::from_millis(1),
        )
        .unwrap();
    assert_eq!(model.record(id(1)).unwrap().status, ActionStatus::Issued);
    assert!(model.take_result(id(1)).is_none());
    model
        .observe(
            scope(1),
            capabilities(),
            state(Some(true)),
            Duration::from_millis(2),
        )
        .unwrap();
    let result = model.take_result(id(1)).unwrap();
    assert_eq!(result.status, ActionStatus::Observed);
    assert_eq!(result.observed_at, Some(Duration::from_millis(2)));
    assert_eq!(result.observation.unwrap().maximized, Some(true));
}

#[test]
fn duplicate_intents_are_idempotent_and_conflicts_or_consumed_ids_are_rejected() {
    let mut model = ready(2);
    let first = model
        .admit(scope(1), id(5), WindowAction::SetMinimized(true))
        .unwrap();
    let duplicate = model
        .admit(scope(1), id(5), WindowAction::SetMinimized(true))
        .unwrap();
    assert!(duplicate.duplicate);
    assert_eq!(first.record, duplicate.record);
    assert_eq!(
        model.admit(scope(1), id(5), WindowAction::SetMinimized(false)),
        Err(ActionError::ConflictingDuplicate)
    );
    model
        .fail(first.record.ticket, ActionError::BackendFailed)
        .unwrap();
    assert_eq!(
        model.take_result(id(5)).unwrap().status,
        ActionStatus::Failed(ActionError::BackendFailed)
    );
    assert_eq!(
        model.admit(scope(1), id(5), WindowAction::SetMinimized(true)),
        Err(ActionError::StaleRequest)
    );
    assert_eq!(
        model.admit(scope(1), id(4), WindowAction::SetMinimized(true)),
        Err(ActionError::StaleRequest)
    );
}

#[test]
fn capacity_counts_pending_and_history_until_terminal_result_is_taken() {
    let mut model = ready(2);
    let first = model
        .admit(scope(1), id(1), WindowAction::SetMaximized(true))
        .unwrap();
    model
        .admit(scope(1), id(2), WindowAction::SetMaximized(false))
        .unwrap();
    assert_eq!(
        model.admit(scope(1), id(3), WindowAction::SetMaximized(true)),
        Err(ActionError::Capacity)
    );
    model
        .fail(first.record.ticket, ActionError::BackendFailed)
        .unwrap();
    assert_eq!(
        model.admit(scope(1), id(3), WindowAction::SetMaximized(true)),
        Err(ActionError::Capacity)
    );
    model.take_result(id(1)).unwrap();
    assert!(
        !model
            .admit(scope(1), id(3), WindowAction::SetMaximized(true))
            .unwrap()
            .duplicate
    );
    assert!(matches!(
        WindowActions::new(9, 0),
        Err(ActionError::Capacity)
    ));
    assert!(matches!(
        WindowActions::new(9, 65),
        Err(ActionError::Capacity)
    ));
}

#[test]
fn contradictory_commands_wait_for_original_fifo_observation() {
    let mut model = ready(4);
    model
        .admit(scope(1), id(1), WindowAction::SetMaximized(true))
        .unwrap();
    model
        .admit(scope(1), id(2), WindowAction::SetMaximized(false))
        .unwrap();
    assert_eq!(model.next(scope(1)).unwrap().unwrap().ticket.id, id(1));
    assert!(model.next(scope(1)).unwrap().is_none());
    assert_eq!(model.record(id(2)).unwrap().status, ActionStatus::Queued);
    model
        .observe(
            scope(1),
            capabilities(),
            state(Some(true)),
            Duration::from_millis(1),
        )
        .unwrap();
    assert_eq!(model.next(scope(1)).unwrap().unwrap().ticket.id, id(2));
    model
        .observe(
            scope(1),
            capabilities(),
            state(Some(false)),
            Duration::from_millis(2),
        )
        .unwrap();
    assert_eq!(
        model.record(id(1)).unwrap().observation.unwrap().maximized,
        Some(true)
    );
    assert_eq!(
        model.record(id(2)).unwrap().observation.unwrap().maximized,
        Some(false)
    );
}

#[test]
fn closing_revokes_both_queued_and_issued_work_and_cannot_be_reopened() {
    let mut model = ready(4);
    model
        .admit(scope(1), id(1), WindowAction::SetMaximized(true))
        .unwrap();
    model
        .admit(scope(1), id(2), WindowAction::SetMinimized(true))
        .unwrap();
    model.next(scope(1)).unwrap();
    assert!(model.invalidate(ActionError::Closing));
    for request in [id(1), id(2)] {
        assert_eq!(
            model.record(request).unwrap().status,
            ActionStatus::Failed(ActionError::Closing)
        );
    }
    assert_eq!(model.next(scope(1)), Err(ActionError::Closing));
    assert_eq!(
        model.admit(scope(1), id(3), WindowAction::SetFullscreen(true)),
        Err(ActionError::Closing)
    );
    assert_eq!(
        model.observe(
            scope(1),
            capabilities(),
            state(Some(true)),
            Duration::from_millis(1)
        ),
        Err(ActionError::Closing)
    );
}

#[test]
fn replacement_scope_rejects_old_queue_and_foreign_host_without_new_issue() {
    let mut model = ready(4);
    model
        .admit(scope(1), id(1), WindowAction::SetMaximized(true))
        .unwrap();
    model
        .observe(
            scope(2),
            capabilities(),
            state(Some(false)),
            Duration::from_millis(1),
        )
        .unwrap();
    assert_eq!(
        model.record(id(1)).unwrap().status,
        ActionStatus::Failed(ActionError::StaleWindow)
    );
    assert_eq!(model.next(scope(1)), Err(ActionError::StaleWindow));
    let mut foreign = scope(2);
    foreign.host = 10;
    assert_eq!(
        model.admit(foreign, id(2), WindowAction::SetMaximized(true)),
        Err(ActionError::ForeignHost)
    );
    assert_eq!(
        model.observe(
            foreign,
            capabilities(),
            state(None),
            Duration::from_millis(2)
        ),
        Err(ActionError::ForeignHost)
    );
    assert!(model.next(scope(2)).unwrap().is_none());
}

#[test]
fn unsupported_capability_and_lost_readback_never_become_false_success() {
    let mut model = ready(2);
    model
        .observe(
            scope(1),
            ActionCapabilities::default(),
            WindowObservation::default(),
            Duration::from_millis(1),
        )
        .unwrap();
    assert_eq!(
        model.admit(scope(1), id(1), WindowAction::SetFullscreen(true)),
        Err(ActionError::Unsupported)
    );
    model
        .observe(
            scope(1),
            capabilities(),
            state(Some(false)),
            Duration::from_millis(2),
        )
        .unwrap();
    model
        .admit(scope(1), id(1), WindowAction::SetFullscreen(true))
        .unwrap();
    model.next(scope(1)).unwrap();
    let mut unknown = state(Some(false));
    unknown.borderless_monitor_coverage = None;
    model
        .observe(scope(1), capabilities(), unknown, Duration::from_millis(3))
        .unwrap();
    assert_eq!(model.record(id(1)).unwrap().status, ActionStatus::Issued);
    model
        .observe(
            scope(1),
            ActionCapabilities::default(),
            unknown,
            Duration::from_millis(4),
        )
        .unwrap();
    assert_eq!(
        model.record(id(1)).unwrap().status,
        ActionStatus::Failed(ActionError::Unsupported)
    );
}

#[test]
fn backend_failure_is_bound_to_the_exact_ticket() {
    let mut model = ready(2);
    let admitted = model
        .admit(scope(1), id(1), WindowAction::SetMinimized(true))
        .unwrap();
    let mut wrong = admitted.record.ticket;
    wrong.scope = scope(2);
    assert_eq!(
        model.fail(wrong, ActionError::BackendFailed),
        Err(ActionError::StaleRequest)
    );
    assert_eq!(model.record(id(1)).unwrap().status, ActionStatus::Queued);
    model
        .fail(admitted.record.ticket, ActionError::BackendFailed)
        .unwrap();
    assert_eq!(
        model.record(id(1)).unwrap().status,
        ActionStatus::Failed(ActionError::BackendFailed)
    );
}

#[test]
fn stale_samples_and_revision_overflow_leave_snapshot_and_request_unchanged() {
    let mut model = ready(2);
    model
        .observe(
            scope(1),
            capabilities(),
            state(Some(false)),
            Duration::from_millis(10),
        )
        .unwrap();
    model
        .admit(scope(1), id(1), WindowAction::SetMaximized(true))
        .unwrap();
    model.next(scope(1)).unwrap();
    let baseline = model.snapshot();
    assert_eq!(
        model.observe(
            scope(1),
            capabilities(),
            state(Some(true)),
            Duration::from_millis(9)
        ),
        Err(ActionError::StaleObservation)
    );
    assert_eq!(model.snapshot(), baseline);
    assert_eq!(model.record(id(1)).unwrap().status, ActionStatus::Issued);
    model.snapshot.as_mut().unwrap().revision = u64::MAX;
    let baseline = model.snapshot();
    assert_eq!(
        model.observe(
            scope(1),
            capabilities(),
            state(Some(true)),
            Duration::from_millis(11)
        ),
        Err(ActionError::CounterExhausted)
    );
    assert_eq!(model.snapshot(), baseline);
    assert_eq!(model.record(id(1)).unwrap().status, ActionStatus::Issued);
}

#[test]
fn borderless_geometry_is_an_observation_not_exclusive_fullscreen_permission() {
    let mut model = ready(2);
    model
        .admit(scope(1), id(1), WindowAction::SetFullscreen(true))
        .unwrap();
    model.next(scope(1)).unwrap();
    let mut observed = state(Some(false));
    observed.borderless_monitor_coverage = Some(true);
    model
        .observe(scope(1), capabilities(), observed, Duration::from_millis(1))
        .unwrap();
    let record = model.record(id(1)).unwrap();
    assert_eq!(record.status, ActionStatus::Observed);
    assert_eq!(
        record.observation.unwrap().borderless_monitor_coverage,
        Some(true)
    );
}

#[test]
fn execution_guard_rechecks_scope_thread_and_close_before_entering_setter() {
    let mut model = ready(2);
    model
        .admit(scope(1), id(1), WindowAction::SetMaximized(true))
        .unwrap();
    let command = model.next(scope(1)).unwrap().unwrap();
    for (current, expected) in [
        (Err(ActionError::Closing), ActionError::Closing),
        (Err(ActionError::Closed), ActionError::Closed),
        (Err(ActionError::WrongThread), ActionError::WrongThread),
        (Ok(scope(2)), ActionError::StaleWindow),
    ] {
        let mut entered = false;
        assert_eq!(
            issue_if_current(
                command,
                || current,
                |_| {
                    entered = true;
                    Ok(())
                }
            ),
            Err(expected)
        );
        assert!(!entered);
    }
    let mut entered = false;
    assert_eq!(
        issue_if_current(
            command,
            || Ok(scope(1)),
            |_| {
                entered = true;
                Err(ActionError::BackendFailed)
            }
        ),
        Err(ActionError::BackendFailed)
    );
    assert!(entered);
    assert_eq!(model.record(id(1)).unwrap().status, ActionStatus::Issued);
}

#[test]
fn execution_guard_rejects_every_identity_component_before_entering_setter() {
    let mut model = ready(2);
    model
        .admit(scope(1), id(1), WindowAction::SetMaximized(true))
        .unwrap();
    let command = model.next(scope(1)).unwrap().unwrap();
    let original = scope(1);
    let mut foreign_host = original;
    foreign_host.host += 1;
    let mut foreign_owner = original;
    foreign_owner.window.owner += 1;
    let mut new_epoch = original;
    new_epoch.window.epoch += 1;
    for (actual, expected) in [
        (foreign_host, ActionError::ForeignHost),
        (foreign_owner, ActionError::StaleWindow),
        (new_epoch, ActionError::StaleWindow),
        (scope(2), ActionError::StaleWindow),
    ] {
        assert_eq!(
            issue_if_current(command, || Ok(actual), |_| panic!("stale setter entered")),
            Err(expected)
        );
    }
    let mut applied = None;
    assert_eq!(
        issue_if_current(
            command,
            || Ok(original),
            |action| {
                applied = Some(action);
                Ok(())
            }
        ),
        Ok(())
    );
    assert_eq!(applied, Some(WindowAction::SetMaximized(true)));
    assert_eq!(model.record(id(1)).unwrap().status, ActionStatus::Issued);
}

#[test]
fn execution_guard_requires_issued_and_never_replays_terminal_or_queued_records() {
    let mut model = ready(2);
    let admitted = model
        .admit(scope(1), id(1), WindowAction::SetMaximized(true))
        .unwrap();
    for status in [
        ActionStatus::Queued,
        ActionStatus::Observed,
        ActionStatus::Failed(ActionError::Closed),
    ] {
        let mut record = admitted.record;
        record.status = status;
        assert_eq!(
            issue_if_current(
                record,
                || panic!("non-issued authority checked"),
                |_| panic!("non-issued setter entered")
            ),
            Err(ActionError::StaleRequest)
        );
    }
    assert_eq!(model.record(id(1)), Some(admitted.record));
}
