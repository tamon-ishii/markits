use std::cell::RefCell;
use std::rc::Rc;

type CapturePreparation = Rc<dyn Fn() -> Result<(), String>>;

thread_local! {
    static PRE_CAPTURE_HOOK: RefCell<Option<CapturePreparation>> = RefCell::new(None);
}

struct HookScope {
    previous: Option<CapturePreparation>,
}

impl HookScope {
    fn install(hook: CapturePreparation) -> Self {
        let previous = PRE_CAPTURE_HOOK.with(|slot| slot.replace(Some(hook)));
        Self { previous }
    }
}

impl Drop for HookScope {
    fn drop(&mut self) {
        let previous = self.previous.take();
        PRE_CAPTURE_HOOK.with(|slot| {
            slot.replace(previous);
        });
    }
}

/// Runs `operation` with a preparation hook scoped to the current worker thread.
/// The prior hook is restored on normal return, error, or panic.
pub fn with_capture_preparation<T>(
    prepare: impl Fn() -> Result<(), String> + 'static,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let _scope = HookScope::install(Rc::new(prepare));
    operation()
}

/// Runs the current thread's preparation hook before a platform capture.
pub(crate) fn run_pre_capture_hook() -> Result<(), String> {
    // Clone while borrowed, then call after the RefCell borrow ends. A hook may
    // itself enter another capture-preparation scope or capture operation.
    let hook = PRE_CAPTURE_HOOK.with(|slot| slot.borrow().clone());
    if let Some(hook) = hook {
        hook()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{run_pre_capture_hook, with_capture_preparation};
    use std::cell::RefCell;
    use std::path::Path;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn nested_scopes_restore_the_previous_hook() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let outer_events = Rc::clone(&events);
        with_capture_preparation(
            move || {
                outer_events.borrow_mut().push("outer");
                Ok(())
            },
            || {
                run_pre_capture_hook()?;
                let inner_events = Rc::clone(&events);
                with_capture_preparation(
                    move || {
                        inner_events.borrow_mut().push("inner");
                        Ok(())
                    },
                    run_pre_capture_hook,
                )?;
                run_pre_capture_hook()
            },
        )
        .unwrap();
        assert_eq!(*events.borrow(), ["outer", "inner", "outer"]);
        assert!(run_pre_capture_hook().is_ok());
    }

    #[test]
    fn hook_can_enter_a_nested_preparation_scope() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let outer_events = Rc::clone(&events);
        with_capture_preparation(
            move || {
                outer_events.borrow_mut().push("outer");
                let inner_events = Rc::clone(&outer_events);
                with_capture_preparation(
                    move || {
                        inner_events.borrow_mut().push("inner");
                        Ok(())
                    },
                    run_pre_capture_hook,
                )
            },
            run_pre_capture_hook,
        )
        .unwrap();
        assert_eq!(*events.borrow(), ["outer", "inner"]);
        assert!(run_pre_capture_hook().is_ok());
    }

    #[test]
    fn previous_hook_is_restored_after_operation_error() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let outer_events = Rc::clone(&events);
        with_capture_preparation(
            move || {
                outer_events.borrow_mut().push("outer");
                Ok(())
            },
            || {
                let inner_events = Rc::clone(&events);
                let result = with_capture_preparation(
                    move || {
                        inner_events.borrow_mut().push("inner");
                        Ok(())
                    },
                    || Err::<(), _>("operation failed".into()),
                );
                assert_eq!(result, Err("operation failed".into()));
                run_pre_capture_hook()
            },
        )
        .unwrap();
        assert_eq!(*events.borrow(), ["outer"]);
        assert!(run_pre_capture_hook().is_ok());
    }

    #[test]
    fn hook_scope_is_isolated_to_the_current_thread() {
        let calls = Arc::new(AtomicUsize::new(0));
        let owner_calls = Arc::clone(&calls);
        with_capture_preparation(
            move || {
                owner_calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            || {
                std::thread::spawn(run_pre_capture_hook).join().unwrap()?;
                run_pre_capture_hook()
            },
        )
        .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn hook_error_stops_capture_before_platform_capture() {
        let error = with_capture_preparation(
            || Err("preparation failed".into()),
            || {
                super::super::window_capture::capture_window(
                    "not-a-window",
                    0,
                    Path::new("/tmp/manual-core-capture-must-not-exist.png"),
                    false,
                    None,
                )
                .map(|_| ())
            },
        );
        assert_eq!(error, Err("preparation failed".into()));
        assert!(run_pre_capture_hook().is_ok());
    }

    #[test]
    fn previous_hook_is_restored_after_panic() {
        let calls = Rc::new(RefCell::new(0));
        let outer_calls = Rc::clone(&calls);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_capture_preparation(
                move || {
                    *outer_calls.borrow_mut() += 1;
                    Ok(())
                },
                || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        with_capture_preparation(
                            || Ok(()),
                            || -> Result<(), String> { panic!("simulated capture panic") },
                        )
                    }));
                    assert!(result.is_err());
                    run_pre_capture_hook()
                },
            )
            .unwrap();
        }));
        assert!(result.is_ok());
        assert_eq!(*calls.borrow(), 1);
        assert!(run_pre_capture_hook().is_ok());
    }
}
