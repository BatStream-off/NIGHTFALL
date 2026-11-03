//! Run closures on CEF's UI thread.

use cef::*;
use std::sync::{Arc, Mutex};

type Job = Box<dyn FnOnce() + Send + 'static>;

wrap_task! {
    struct ClosureTask {
        job: Arc<Mutex<Option<Job>>>,
    }

    impl Task {
        fn execute(&self) {
            let job = self.job.lock().ok().and_then(|mut j| j.take());
            if let Some(job) = job {
                job();
            }
        }
    }
}

/// Run `f` on the UI thread (immediately if already there).
pub fn on_ui(f: impl FnOnce() + Send + 'static) {
    if currently_on(ThreadId::UI) != 0 {
        f();
        return;
    }
    let mut task = ClosureTask::new(Arc::new(Mutex::new(Some(Box::new(f)))));
    post_task(ThreadId::UI, Some(&mut task));
}

/// Run `f` on the UI thread after `delay_ms`.
pub fn on_ui_delayed(delay_ms: i64, f: impl FnOnce() + Send + 'static) {
    let mut task = ClosureTask::new(Arc::new(Mutex::new(Some(Box::new(f)))));
    post_delayed_task(ThreadId::UI, Some(&mut task), delay_ms);
}

/// Run blocking work on a plain OS thread, then deliver the result on the UI thread.
pub fn background<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(T) + Send + 'static) {
    std::thread::spawn(move || {
        let r = work();
        on_ui(move || done(r));
    });
}
