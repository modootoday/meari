//! Split from the parent module to keep files under the 600-line structure limit.
use super::super::completion::CompletionSignal;
use super::*;
use std::{rc::Rc, task::Waker, time::Duration};

#[cfg(test)]
mod parent_authority_tests {
    use super::*;
    use std::{cell::Cell, io};
    struct Clock;
    impl MonotonicClock for Clock {
        fn now(&self) -> Duration {
            Duration::ZERO
        }
    }
    struct Probe {
        starts: Rc<Cell<usize>>,
        losses: Rc<Cell<usize>>,
        hook: Option<Box<dyn FnMut(CleanupRequest)>>,
    }
    impl NativeChild for Probe {
        type Error = io::Error;
        fn parent_lost(&mut self) {
            self.losses.set(self.losses.get() + 1);
        }
        fn begin_cleanup(
            &mut self,
            request: CleanupRequest,
            _: CompletionSignal,
        ) -> Result<CleanupProgress, io::Error> {
            self.starts.set(self.starts.get() + 1);
            if let Some(hook) = &mut self.hook {
                hook(request);
            }
            Ok(CleanupProgress::Complete)
        }
        fn observe_cleanup(&mut self, _: CleanupRequest) -> Result<CleanupProgress, io::Error> {
            Ok(CleanupProgress::Complete)
        }
    }
    fn owner() -> Rc<WindowLifecycle<Probe>> {
        Rc::new(
            WindowLifecycle::new(
                LifecycleLimits::new(2, Duration::from_secs(1)).unwrap(),
                Clock,
                Waker::noop().clone(),
            )
            .unwrap(),
        )
    }
    fn probe() -> Probe {
        Probe {
            starts: Rc::new(Cell::new(0)),
            losses: Rc::new(Cell::new(0)),
            hook: None,
        }
    }

    #[test]
    fn creation_authority_is_not_an_active_cleanup_or_foreign_permission() {
        let owner = owner();
        let stamp = owner.ready(ReadyKind::NewWindow).unwrap();
        let lease = owner
            .register(stamp, |lease| {
                owner.validate_parent_creation(lease).unwrap();
                assert_eq!(owner.validate_access(lease), Err(LifecycleError::Busy));
                Ok(probe())
            })
            .unwrap();
        let parent = lease.for_parent();
        owner.validate_access(&parent).unwrap();
        assert_eq!(
            owner.validate_parent_creation(&parent),
            Err(LifecycleError::WrongState)
        );
        let other = self::owner();
        other.ready(ReadyKind::NewWindow).unwrap();
        assert_eq!(
            other.validate_parent_creation(&parent),
            Err(LifecycleError::Stale)
        );
    }

    #[test]
    fn cleanup_parent_requires_the_lent_adapter_and_current_attempt() {
        let owner = owner();
        let stamp = owner.ready(ReadyKind::NewWindow).unwrap();
        let observed = Rc::new(Cell::new(false));
        let weak = Rc::downgrade(&owner);
        let out = observed.clone();
        owner
            .register(stamp, |lease| {
                let parent = lease.for_parent();
                let mut child = probe();
                child.hook = Some(Box::new(move |request| {
                    let owner = weak.upgrade().unwrap();
                    owner.validate_parent_cleanup(&parent, request).unwrap();
                    assert_eq!(owner.validate_access(&parent), Err(LifecycleError::Busy));
                    let mut stale = request;
                    stale.stamp.attempt += 1;
                    assert_eq!(
                        owner.validate_parent_cleanup(&parent, stale),
                        Err(LifecycleError::Stale)
                    );
                    out.set(true);
                }));
                Ok(child)
            })
            .unwrap();
        assert_eq!(owner.begin_close().unwrap().status, CleanupStatus::Complete);
        assert!(observed.get());
    }

    #[test]
    fn abandon_revokes_without_callbacks_until_explicit_residual_cleanup() {
        let owner = owner();
        let stamp = owner.ready(ReadyKind::NewWindow).unwrap();
        let child = probe();
        let starts = child.starts.clone();
        let losses = child.losses.clone();
        let lease = owner.register(stamp, |_| Ok(child)).unwrap();
        owner.abandon_window().unwrap();
        assert_eq!(starts.get(), 0);
        assert_eq!(losses.get(), 0);
        assert_eq!(owner.validate_access(&lease), Err(LifecycleError::Closed));
        assert_eq!(owner.report().unwrap().remaining, 1);
        let report = owner.force_loss().unwrap();
        assert_eq!(report.status, CleanupStatus::ForcedIncomplete);
        assert_eq!(report.remaining, 0);
        assert_eq!(starts.get(), 1);
        assert_eq!(losses.get(), 1);
        owner.force_loss().unwrap();
        assert_eq!(starts.get(), 1);
        assert!(owner.window_removed().is_err());
    }

    #[test]
    fn abandonment_during_factory_returns_partial_owner_instead_of_active_lease() {
        let owner = owner();
        let stamp = owner.ready(ReadyKind::NewWindow).unwrap();
        let child = probe();
        let starts = child.starts.clone();
        let failure = owner
            .register(stamp, |_| {
                owner.abandon_window().unwrap();
                Ok(child)
            })
            .unwrap_err();
        assert!(matches!(
            failure.problem,
            RegistrationProblem::Lifecycle(LifecycleError::Stale)
        ));
        assert!(failure.cleanup.is_some());
        assert_eq!(starts.get(), 0);
        assert_eq!(owner.report().unwrap().remaining, 1);
        assert_eq!(owner.force_loss().unwrap().remaining, 0);
    }
}
