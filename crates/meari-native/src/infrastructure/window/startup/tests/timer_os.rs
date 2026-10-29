//! Real original-owner OS path. Run this ignored test alone in a separately assigned process.
use super::*;
use crate::application::WindowOptions;
use crate::infrastructure::window::*;
use std::{cell::RefCell, rc::Rc, time::Instant};

#[derive(Default)]
struct Evidence {
    wake: Option<NativeWindowWake>,
    timer: Option<NativeHostTimer>,
    cancelled_owner: Option<NativeHostTimer>,
    first_submission: Option<HostStamp>,
    commits: Vec<HostCommitReceipt>,
    final_submission: Option<HostStamp>,
    armed_at: Option<Instant>,
    close_requested: bool,
}
struct TimerHost {
    document: NativeDocumentHandle,
    evidence: Rc<RefCell<Evidence>>,
}
impl NativeWindowHost for TimerHost {
    type Error = std::io::Error;
    type CloseCheckpoint = ();
    fn admit_callback(
        &mut self,
        _: &HostStamp,
        _: &meari::application::host_callbacks::PendingCallback,
    ) -> IntentAdmission<Self::Error> {
        panic!("minimal timer document has no host callbacks")
    }
    fn after_dispatch(&mut self, turn: HostTurn) -> Result<HostEffects, Self::Error> {
        let wake = self
            .evidence
            .borrow()
            .wake
            .clone()
            .expect("original attached wake");
        let firing = wake.timers.lock().unwrap().trace.is_firing();
        if !firing {
            return Ok(HostEffects::default());
        }
        assert_eq!(turn.reason, HostTurnReason::Completion);
        let (_, committed) = self
            .document
            .try_reproject(&turn.stamp, |_| Ok::<_, std::io::Error>(()))
            .map_err(std::io::Error::other)?;
        let (timer, count) = {
            let mut evidence = self.evidence.borrow_mut();
            evidence.commits.push(committed.clone());
            (evidence.timer.take().unwrap(), evidence.commits.len())
        };
        if count == 1 {
            // Successful original commit, no retry or manual completion wake.
            timer
                .schedule_after(Duration::from_millis(40))
                .map_err(std::io::Error::other)?;
        } else {
            assert_eq!(count, 2, "unexpected extra timer turn");
            // Keep an outstanding deadline until real close cancels the owner book.
            timer
                .schedule_after(Duration::from_secs(1))
                .map_err(std::io::Error::other)?;
        }
        self.evidence.borrow_mut().timer = Some(timer);
        Ok(HostEffects {
            committed: Some(committed),
            redraw: true,
        })
    }
    fn close(&mut self, _: CloseRequest<()>) -> CloseDecision<(), Self::Error> {
        CloseDecision::HostCleanupComplete { checkpoint: () }
    }
}
struct Controller {
    document: NativeDocumentHandle,
    evidence: Rc<RefCell<Evidence>>,
    watchdog: Option<(std::sync::mpsc::Sender<()>, std::thread::JoinHandle<bool>)>,
}
impl NativeStartupController for Controller {
    type Host = TimerHost;
    type Error = std::io::Error;
    fn observe(
        &mut self,
        context: &mut NativeStartupContext<'_>,
    ) -> Result<NativeStartupPoll<TimerHost>, Self::Error> {
        let runtime = Rc::new(RefCell::new(NativeHostRuntime::new(
            self.document.clone(),
            TimerHost {
                document: self.document.clone(),
                evidence: self.evidence.clone(),
            },
        )));
        let document = self.document.clone();
        let evidence = self.evidence.clone();
        let observer = Box::new(move |notice: NativeWindowNotice| {
            assert!(notice.window.is_none(), "no child authority requested");
            if notice.kind != NativeWindowNoticeKind::FrameSubmitted {
                return Ok(());
            }
            let frame = notice.frame.expect("actual checked frame");
            let (first, final_commit, wake) = {
                let state = evidence.borrow();
                (
                    state.first_submission.is_none(),
                    state
                        .commits
                        .last()
                        .cloned()
                        .filter(|_| state.commits.len() == 2),
                    state.wake.clone().unwrap(),
                )
            };
            if first {
                let timer = evidence.borrow_mut().timer.take().unwrap();
                let old = timer
                    .schedule_after(Duration::from_secs(1))
                    .map_err(|error| error.to_string())?;
                let _replacement = timer
                    .schedule_after(Duration::from_millis(40))
                    .map_err(|error| error.to_string())?;
                assert!(!timer.cancel(&old).unwrap());
                let cancelled = wake.timer(&document).unwrap();
                let token = cancelled.schedule_after(Duration::from_millis(1)).unwrap();
                assert!(cancelled.cancel(&token).unwrap());
                let dropped = wake.timer(&document).unwrap();
                dropped.schedule_after(Duration::from_millis(1)).unwrap();
                drop(dropped);
                let mut state = evidence.borrow_mut();
                state.armed_at = Some(Instant::now());
                state.first_submission = Some(frame.document);
                state.timer = Some(timer);
                state.cancelled_owner = Some(cancelled);
            } else if let Some(commit) = final_commit {
                assert_eq!(
                    frame.document.document_revision,
                    commit.after().document_revision
                );
                let mut state = evidence.borrow_mut();
                if !state.close_requested {
                    state.final_submission = Some(frame.document);
                    state.close_requested = true;
                    drop(state);
                    wake.request_close().map_err(|error| error.to_string())?;
                }
            }
            Ok(())
        });
        context
            .attach_observed(
                runtime,
                WindowOptions {
                    title: "Meari owned timer original OS fixture".into(),
                },
                observer,
            )
            .map(NativeStartupPoll::Attach)
            .map_err(|_| std::io::Error::other("original observed attachment failed"))
    }
    fn attached(
        &mut self,
        runtime: Rc<RefCell<NativeHostRuntime<TimerHost>>>,
        wake: NativeWindowWake,
    ) -> Result<(), Self::Error> {
        let original = runtime.borrow().document();
        assert_eq!(original.id(), self.document.id());
        let timer = wake.timer(&original).map_err(std::io::Error::other)?;
        {
            let mut evidence = self.evidence.borrow_mut();
            evidence.wake = Some(wake.clone());
            evidence.timer = Some(timer);
        }
        // Same diagnostic timeout pattern as startup_window.rs; never drives success.
        let (stop, receiver) = std::sync::mpsc::channel();
        self.watchdog = Some((
            stop,
            std::thread::spawn(move || {
                if receiver.recv_timeout(Duration::from_secs(5)).is_err() {
                    let _ = wake.request_close();
                    true
                } else {
                    false
                }
            }),
        ));
        Ok(())
    }
}

#[test]
#[ignore = "explicit Windows original NativeLoopOwner timer OS slot; run alone"]
fn original_os_timer_one_shot_rearm_cancel_and_destroy_are_causal() {
    let limits =
        HostScheduleLimits::new(Duration::from_millis(5), Duration::from_secs(1), 12).unwrap();
    let mut owner = NativeLoopOwner::new(limits).unwrap();
    let startup = owner.startup().unwrap();
    let document = document();
    let evidence = Rc::new(RefCell::new(Evidence::default()));
    let mut outcome = owner.run_startup(
        startup,
        Controller {
            document: document.clone(),
            evidence: evidence.clone(),
            watchdog: None,
        },
    );
    let (stop, watchdog) = outcome.controller.watchdog.take().unwrap();
    let _ = stop.send(());
    assert!(
        !watchdog.join().unwrap(),
        "timeout is failure, never timer evidence"
    );
    assert!(outcome.result.is_ok(), "{:?}", outcome.result);
    assert_eq!(document.lifecycle(), HostLifecycle::Closed);
    let mut state = evidence.borrow_mut();
    assert!(state.first_submission.is_some());
    assert!(state.armed_at.is_some());
    assert!(state.final_submission.is_some());
    assert!(state.close_requested);
    assert_eq!(state.commits.len(), 2);
    let wake = state.wake.as_ref().unwrap();
    let book = wake.timers.lock().unwrap();
    assert!(!book.trace.overflow);
    assert_eq!(
        book.trace.records.len(),
        2,
        "cancelled/dropped/close arms must not fire"
    );
    assert!(
        book.trace.destroyed.is_some(),
        "actual original WindowEvent::Destroyed required"
    );
    for (index, record) in book.trace.records.iter().enumerate() {
        assert!(record.firing.now >= record.firing.deadline);
        assert_eq!(record.firing.scope.host_id, document.id());
        assert_eq!(record.firing.sequence, index as u64 + 2);
        let receipt = record
            .result
            .as_ref()
            .expect("dispatch was not skipped")
            .as_ref()
            .expect("no failed-turn retry");
        assert_eq!(&receipt.stamp, state.commits[index].after());
        assert!(receipt.stamp.turn_sequence > record.firing.scope.turn_sequence);
    }
    assert_eq!(
        book.trace.records[0].firing.owner,
        book.trace.records[1].firing.owner
    );
    eprintln!(
        "owned_timer_original_os records={:?} destroyed={:?}",
        book.trace.records, book.trace.destroyed
    );
    drop(book);
    assert!(matches!(
        state
            .timer
            .as_ref()
            .unwrap()
            .schedule_after(Duration::from_millis(1)),
        Err(HostAccessError::Closed)
    ));
    drop(state.timer.take());
    drop(state.cancelled_owner.take());
}
