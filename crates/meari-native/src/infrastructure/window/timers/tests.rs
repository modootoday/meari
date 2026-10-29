use super::*;
fn scope() -> HostStamp {
    let source = meari::parse(
        "type Props={};export function App(props:Props){return <main>Timer model</main>;}",
        meari::application::Language::Tsx,
    )
    .unwrap();
    let document = crate::infrastructure::document::InteractiveDocument::new(
        source,
        "",
        crate::domain::Viewport {
            width: 320,
            height: 240,
            dark: false,
        },
    )
    .unwrap();
    NativeDocumentHandle::new(document, 1)
        .unwrap()
        .stamp()
        .unwrap()
}
#[test]
fn deadline_fires_once_at_or_after_monotonic_due_time() {
    let mut book = TimerBook::default();
    let scope = scope();
    let now = Instant::now();
    let key = book.register(scope.clone()).unwrap();
    book.arm(key, &scope, now, Duration::from_millis(250))
        .unwrap();
    assert_eq!(
        book.poll(now + Duration::from_millis(249), &scope, false),
        (false, Some(now + Duration::from_millis(250)))
    );
    assert_eq!(
        book.poll(now + Duration::from_millis(250), &scope, false),
        (true, None)
    );
    assert_eq!(
        book.poll(now + Duration::from_secs(1), &scope, false),
        (false, None)
    );
}
#[test]
fn replacement_invalidates_old_cancel_and_current_cancel_is_idempotent() {
    let mut book = TimerBook::default();
    let scope = scope();
    let now = Instant::now();
    let key = book.register(scope.clone()).unwrap();
    let first = book
        .arm(key, &scope, now, Duration::from_millis(10))
        .unwrap();
    let second = book
        .arm(key, &scope, now, Duration::from_millis(20))
        .unwrap();
    assert!(!book.cancel(key, first));
    assert!(book.cancel(key, second));
    assert!(!book.cancel(key, second));
    assert_eq!(
        book.poll(now + Duration::from_secs(1), &scope, false),
        (false, None)
    );
}
#[test]
fn ordinary_commits_do_not_stale_timer_but_original_source_replacement_does() {
    let mut book = TimerBook::default();
    let original = scope();
    let now = Instant::now();
    let key = book.register(original.clone()).unwrap();
    book.arm(key, &original, now, Duration::from_millis(10))
        .unwrap();
    let mut current = original.clone();
    current.document_revision += 1;
    current.turn_sequence += 1;
    assert!(
        book.poll(now + Duration::from_millis(10), &current, false)
            .0
    );
    book.arm(key, &current, now, Duration::from_millis(10))
        .unwrap();
    current.owner.source_generation += 1;
    assert!(
        !book
            .poll(now + Duration::from_millis(10), &current, false)
            .0
    );
    assert_eq!(
        book.arm(key, &current, now, Duration::from_millis(10)),
        Err(HostAccessError::Stale)
    );
}
#[test]
fn pending_retry_defers_due_completion_without_past_deadline_spin() {
    let mut book = TimerBook::default();
    let scope = scope();
    let now = Instant::now();
    let key = book.register(scope.clone()).unwrap();
    book.arm(key, &scope, now, Duration::from_millis(10))
        .unwrap();
    let due = now + Duration::from_millis(10);
    assert_eq!(book.poll(due, &scope, true), (false, None));
    assert_eq!(
        book.poll(due + Duration::from_millis(5), &scope, false),
        (true, None)
    );
    assert_eq!(
        book.poll(due + Duration::from_millis(6), &scope, false),
        (false, None)
    );
}
#[test]
fn close_cancel_and_explicit_bounds_preserve_no_hidden_deadline() {
    let mut book = TimerBook::default();
    let scope = scope();
    let now = Instant::now();
    let key = book.register(scope.clone()).unwrap();
    assert!(book.arm(key, &scope, now, Duration::ZERO).is_err());
    assert!(
        book.arm(key, &scope, now, Duration::from_secs(86401))
            .is_err()
    );
    book.arm(key, &scope, now, Duration::from_millis(10))
        .unwrap();
    book.cancel_all();
    assert_eq!(
        book.poll(now + Duration::from_secs(1), &scope, false),
        (false, None)
    );
    book.close();
    assert_eq!(
        book.arm(key, &scope, now, Duration::from_millis(10)),
        Err(HostAccessError::Closed)
    );
    assert_eq!(book.register(scope), Err(HostAccessError::Closed));
}
#[test]
fn owner_budget_and_sequence_overflow_do_not_replace_original_deadline() {
    let mut book = TimerBook::default();
    let scope = scope();
    let now = Instant::now();
    let key = book.register(scope.clone()).unwrap();
    book.arm(key, &scope, now, Duration::from_millis(10))
        .unwrap();
    book.entries.get_mut(&key).unwrap().sequence = u64::MAX;
    assert_eq!(
        book.arm(key, &scope, now, Duration::from_millis(20)),
        Err(HostAccessError::CounterExhausted)
    );
    assert!(book.poll(now + Duration::from_millis(10), &scope, false).0);
    for _ in 1..64 {
        book.register(scope.clone()).unwrap();
    }
    assert_eq!(book.register(scope), Err(HostAccessError::Busy));
}
