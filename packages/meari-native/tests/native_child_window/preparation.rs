use super::*;
pub(super) fn check(event_loop: &blitz_shell::EventLoop) {
    let document = document();
    let prepared = NativeWindowPreparation::<parent::Child>::new(
        event_loop,
        document.clone(),
        LifecycleLimits::new(2, Duration::from_secs(2)).unwrap(),
        WindowOptions::default(),
        None,
    )
    .unwrap();
    let children = prepared.children();
    let registrar = prepared.registrar();
    let called = Cell::new(false);
    let rejected = registrar.register(children.stamp().unwrap(), |_, _| {
        called.set(true);
        panic!("pre-ready OS creation")
    });
    assert!(matches!(
        rejected.unwrap_err().problem,
        RegistrationProblem::Lifecycle(LifecycleError::NotReady)
    ));
    assert!(!called.get());
    let runtime = Rc::new(RefCell::new(NativeHostRuntime::new(
        document,
        ChildWindowHost::new(
            Host(Rc::new(Cell::new(0)), Rc::new(Cell::new(0))),
            children.clone(),
        ),
    )));
    let guard = runtime.borrow_mut();
    let failure = match prepared.attach(runtime.clone()) {
        Err(failure) => failure,
        Ok(_) => panic!("borrowed runtime attached"),
    };
    assert_eq!(failure.stage, NativeAttachStage::Validation);
    assert!(matches!(failure.problem, NativeAttachProblem::Borrowed));
    assert!(Rc::ptr_eq(&failure.runtime, &runtime));
    assert!(Rc::ptr_eq(&failure.preparation.children(), &children));
    assert_eq!(children.phase().unwrap(), WindowPhase::NotReady);
    drop(guard);
    let NativeAttachFailure {
        preparation,
        runtime: returned,
        ..
    } = *failure;
    let failure = match preparation.attach(returned) {
        Err(failure) => failure,
        Ok(_) => panic!("preparation was consumed twice"),
    };
    assert!(matches!(failure.problem, NativeAttachProblem::NotPristine));
    assert!(Rc::ptr_eq(&failure.runtime, &runtime));
    drop(failure);
    assert_eq!(children.phase().unwrap(), WindowPhase::ForcedIncomplete);
    assert_eq!(runtime.borrow().host().host().0.get(), 0);
    assert_eq!(runtime.borrow().host().host().1.get(), 0);
}
