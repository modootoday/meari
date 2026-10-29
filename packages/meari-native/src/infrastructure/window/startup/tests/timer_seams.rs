//! Headless seams using the existing startup proxy and real document/runtime owners.
//! No OS window, event-loop deadline delivery, or presentation is exercised here.
use super::*;
use std::time::Instant;

fn pair() -> (NativeDocumentHandle, NativeWindowWake) {
    let document = document();
    let (mut wake, _) = target();
    // Existing fake-loop backend binds the real handle ID, never an OS authority.
    wake.host = document.id();
    (document, wake)
}

#[test]
fn public_timer_rejects_foreign_tokens_and_preserves_replacement() {
    let (document, wake) = pair();
    let first = wake.timer(&document).unwrap();
    let second = wake.timer(&document).unwrap();
    let old = first.schedule_after(Duration::from_secs(1)).unwrap();
    assert!(matches!(second.cancel(&old), Err(HostAccessError::Stale)));
    let replacement = first.schedule_after(Duration::from_secs(2)).unwrap();
    assert!(!first.cancel(&old).unwrap());
    assert!(first.cancel(&replacement).unwrap());
    assert!(!first.cancel(&replacement).unwrap());
    assert_eq!(wake.take(), super::super::super::wake::TIMER_UPDATE);
}

#[test]
fn public_timer_drop_returns_owner_budget_and_cancels_deadline() {
    let (document, wake) = pair();
    let mut owners = (0..64)
        .map(|_| wake.timer(&document).unwrap())
        .collect::<Vec<_>>();
    assert!(matches!(wake.timer(&document), Err(HostAccessError::Busy)));
    let owner = owners.pop().unwrap();
    owner.schedule_after(Duration::from_millis(1)).unwrap();
    drop(owner);
    let replacement = wake.timer(&document).unwrap();
    let stamp = document.stamp().unwrap();
    assert_eq!(
        wake.timers
            .lock()
            .unwrap()
            .poll(Instant::now() + Duration::from_secs(1), &stamp, false),
        (false, None)
    );
    drop(replacement);
    drop(owners);
    let fresh = (0..64)
        .map(|_| wake.timer(&document).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(fresh.len(), 64);
}

#[test]
fn public_timer_rejects_different_document_and_closed_wake() {
    let (document, wake) = pair();
    let foreign = super::document();
    assert!(matches!(wake.timer(&foreign), Err(HostAccessError::Stale)));
    let timer = wake.timer(&document).unwrap();
    timer.schedule_after(Duration::from_secs(1)).unwrap();
    wake.close();
    assert!(matches!(
        wake.timer(&document),
        Err(HostAccessError::Closed)
    ));
    assert!(matches!(
        timer.schedule_after(Duration::from_secs(1)),
        Err(HostAccessError::Closed)
    ));
    assert_eq!(
        wake.timers.lock().unwrap().poll(
            Instant::now() + Duration::from_secs(2),
            &document.stamp().unwrap(),
            false
        ),
        (false, None)
    );
}

#[test]
fn due_timer_after_real_commit_delivers_fresh_runtime_completion_stamp() {
    let (document, wake) = pair();
    let timer = wake.timer(&document).unwrap();
    timer.schedule_after(Duration::from_millis(1)).unwrap();
    let before = document.stamp().unwrap();
    document
        .try_reproject(&before, |_| Ok::<_, std::io::Error>(()))
        .unwrap();
    let current = document.stamp().unwrap();
    assert!(current.document_revision > before.document_revision);
    let mut runtime = NativeHostRuntime::new(document.clone(), Host);
    assert!(!runtime.has_pending_turn());
    let now = Instant::now() + Duration::from_secs(1);
    assert_eq!(
        wake.timers.lock().unwrap().poll(now, &current, false),
        (true, None)
    );
    // Real runtime path, explicitly not Application::about_to_wait/OS delivery.
    let receipt = runtime.after_dispatch(HostTurnReason::Completion).unwrap();
    assert_eq!(receipt.stamp, document.stamp().unwrap());
    assert_eq!(receipt.stamp.host_id, current.host_id);
    assert_eq!(receipt.stamp.document_revision, current.document_revision);
    assert!(receipt.stamp.turn_sequence > current.turn_sequence);
    assert!(!runtime.has_pending_turn());
    assert_eq!(
        wake.timers.lock().unwrap().poll(now, &receipt.stamp, false),
        (false, None)
    );
    let next = timer.schedule_after(Duration::from_secs(1)).unwrap();
    assert!(timer.cancel(&next).unwrap());
    runtime.request_close(CloseCause::User).unwrap();
    assert!(matches!(
        timer.schedule_after(Duration::from_secs(1)),
        Err(HostAccessError::Closing)
    ));
}

#[test]
fn original_timer_stales_after_actual_source_replacement() {
    let source = include_str!("../../../../../../meari/fixtures/state-counter.tsx");
    let component = meari::parse(source, meari::application::Language::Tsx).unwrap();
    let document = NativeDocumentHandle::new(
        InteractiveDocument::new(
            component,
            "",
            Viewport {
                width: 320,
                height: 240,
                dark: false,
            },
        )
        .unwrap(),
        1,
    )
    .unwrap();
    let (mut wake, _) = target();
    wake.host = document.id();
    let timer = wake.timer(&document).unwrap();
    timer.schedule_after(Duration::from_millis(1)).unwrap();
    let before = document.stamp().unwrap();
    let replacement = meari::parse(
        &source.replace("Count ", "Total "),
        meari::application::Language::Tsx,
    )
    .unwrap();
    document
        .try_reproject(&before, |document| {
            let prepared = document.prepare_source_replacement(replacement)?;
            document.commit_source_replacement(prepared)
        })
        .unwrap();
    let after = document.stamp().unwrap();
    assert_ne!(after.definition_revision, before.definition_revision);
    assert!(matches!(
        timer.schedule_after(Duration::from_secs(1)),
        Err(HostAccessError::Stale)
    ));
    assert_eq!(
        wake.timers
            .lock()
            .unwrap()
            .poll(Instant::now() + Duration::from_secs(2), &after, false),
        (false, None)
    );
    let fresh = wake.timer(&document).unwrap();
    let token = fresh.schedule_after(Duration::from_secs(1)).unwrap();
    assert!(fresh.cancel(&token).unwrap());
}
