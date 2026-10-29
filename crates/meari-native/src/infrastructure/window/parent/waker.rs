//! Split from the parent module to keep files under the 600-line structure limit.
use super::super::forwarding::ForwardingSeed;
use super::*;
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    sync::Arc,
};

#[cfg(test)]
mod notice_tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        task::{Wake, Waker},
    };
    struct Count(AtomicUsize);
    impl Wake for Count {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
        fn wake_by_ref(self: &Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    #[test]
    fn reentrant_notice_is_busy_but_retained_for_the_next_turn() {
        let slot: Rc<RefCell<Option<Weak<ParentBinding>>>> = Rc::new(RefCell::new(None));
        let target = slot.clone();
        let calls = Rc::new(RefCell::new(Vec::new()));
        let out = calls.clone();
        let seed = ForwardingSeed::new(11);
        let wakes = Arc::new(Count(AtomicUsize::new(0)));
        seed.bind_once(11, Waker::from(wakes.clone())).unwrap();
        let binding = ParentBinding::new(
            11,
            seed.clone(),
            Some(Box::new(move |notice| {
                out.borrow_mut().push(notice.kind);
                if notice.kind == NativeWindowNoticeKind::Ready {
                    let binding = target.borrow().as_ref().unwrap().upgrade().unwrap();
                    assert!(matches!(
                        binding.publish(NativeWindowNoticeKind::GeometryChanged, None),
                        Err(NativeNoticeError::Busy)
                    ));
                    assert!(matches!(
                        binding.take_notices(),
                        Err(NativeNoticeError::Busy)
                    ));
                }
                Ok(())
            })),
        );
        *slot.borrow_mut() = Some(Rc::downgrade(&binding));
        binding
            .publish(NativeWindowNoticeKind::Ready, None)
            .unwrap();
        assert_eq!(*calls.borrow(), [NativeWindowNoticeKind::Ready]);
        assert_eq!(wakes.0.load(Ordering::SeqCst), 1);
        assert!(seed.take(11).unwrap());
        binding.flush_notices().unwrap();
        assert_eq!(
            *calls.borrow(),
            [
                NativeWindowNoticeKind::Ready,
                NativeWindowNoticeKind::GeometryChanged
            ]
        );
        assert!(binding.take_notices().unwrap().is_empty());
    }
    #[test]
    fn failed_callback_is_not_replayed_until_explicit_recovery() {
        let calls = Rc::new(Cell::new(0));
        let out = calls.clone();
        let binding = ParentBinding::new(
            12,
            ForwardingSeed::new(12),
            Some(Box::new(move |_| {
                out.set(out.get() + 1);
                if out.get() == 1 {
                    Err("technical observer failure".into())
                } else {
                    Ok(())
                }
            })),
        );
        assert!(matches!(
            binding.publish(NativeWindowNoticeKind::Created, None),
            Err(NativeNoticeError::Callback(_))
        ));
        assert!(matches!(
            binding.flush_notices(),
            Err(NativeNoticeError::Paused)
        ));
        assert_eq!(calls.get(), 1);
        binding.retry_observer().unwrap();
        assert_eq!(calls.get(), 2);
        assert!(binding.take_notices().unwrap().is_empty());
    }
    #[test]
    fn polling_observer_keeps_only_latest_notice_per_kind() {
        let binding = ParentBinding::new(13, ForwardingSeed::new(13), None);
        for _ in 0..100 {
            binding
                .publish(NativeWindowNoticeKind::GeometryChanged, None)
                .unwrap();
        }
        let notices = binding.take_notices().unwrap();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].revision, 100);
    }
}
