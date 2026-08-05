//! Run `reqwest::blocking` work on a thread that carries no Tokio context.
//!
//! # The defect this exists to prevent
//!
//! Every `reqwest::blocking` operation — client construction *and* each
//! request/response body read — funnels through
//! `reqwest::blocking::wait::timeout`, which opens with:
//!
//! ```ignore
//! fn enter() {
//!     // Check we aren't already in a runtime
//!     #[cfg(debug_assertions)]
//!     {
//!         let _enter = tokio::runtime::Builder::new_current_thread()
//!             .build()
//!             .expect("build shell runtime")
//!             .enter();
//!     }
//! }
//! ```
//!
//! That throwaway runtime is built and then **dropped** on the calling thread.
//! Tokio refuses to drop a runtime from a thread that is currently inside
//! another runtime's `enter_runtime` region (`try_enter_blocking_region`
//! returns `None`) and panics with:
//!
//! ```text
//! Cannot drop a runtime in a context where blocking is not allowed.
//! This happens when a runtime is dropped from within an asynchronous context.
//! ```
//!
//! An `async fn` Tauri command runs on a Tokio worker inside exactly that
//! region, so any such command that reaches a blocking keyserver call takes
//! the panic. The panic poisons whatever `AppState` mutex the calling frame
//! held, and the next `expect()` on that mutex escalates into a process abort.
//!
//! Note that this is a *debug-assertions* code path in reqwest: the shipping
//! release build never builds the shell runtime, which is why the fault is
//! only visible in `cargo tauri dev` / debug binaries. That does not make it
//! benign — every developer and QA launch hits it, and the crash it produces
//! is indistinguishable from a real one.
//!
//! # Why a dedicated thread
//!
//! We do not own the runtime being dropped, so "drop it elsewhere" and "hold a
//! `Handle` instead" are not available: the runtime is created and destroyed
//! inside reqwest. The only lever is the *context of the calling thread*. A
//! freshly spawned OS thread carries none of Tokio's thread-locals, so the
//! shell runtime is created and dropped in a clean context and the assertion
//! passes.
//!
//! [`KeyServerClient::new`](crate::client::KeyServerClient::new) already used
//! this technique for client construction; this module generalises it to the
//! request path, which was left unguarded.
//!
//! # Why unconditionally, rather than only when a runtime is detected
//!
//! `tokio::runtime::Handle::try_current()` is `Ok` on `spawn_blocking` threads
//! too, where blocking is perfectly legal, and there is no public API that
//! reports the actual "inside `enter_runtime`" condition. Rather than guess
//! with a proxy signal — and rather than take a dependency on Tokio in
//! `keystore` purely to guess — the hop is unconditional. It costs one thread
//! spawn (tens of microseconds) per HTTP round trip that already costs
//! milliseconds, and it makes the call site correct from *any* caller context
//! without the caller having to know which one it is in.

/// Run `work` on a dedicated OS thread and return its value.
///
/// Semantically a plain function call: the current thread blocks until `work`
/// finishes, borrows are preserved via [`std::thread::scope`], and a panic
/// inside `work` is re-raised on the caller with
/// [`std::panic::resume_unwind`], so panic behaviour is unchanged.
///
/// Wrap the *whole* blocking exchange — send **and** body read — in a single
/// call. A `reqwest::blocking::Response` returned across the boundary would
/// perform its body read back on the caller's thread and re-open the fault.
pub fn off_async_context<T, F>(work: F) -> T
where
    F: FnOnce() -> T + Send,
    T: Send,
{
    std::thread::scope(|scope| match scope.spawn(work).join() {
        Ok(value) => value,
        Err(panic) => std::panic::resume_unwind(panic),
    })
}

#[cfg(test)]
mod tests {
    use super::off_async_context;

    #[test]
    fn returns_the_closure_value_and_allows_borrows() {
        let owned = vec![1u8, 2, 3];
        let borrowed = &owned;
        assert_eq!(off_async_context(|| borrowed.len()), 3);
    }

    #[test]
    fn re_raises_a_panic_on_the_caller() {
        let outcome = std::panic::catch_unwind(|| {
            off_async_context(|| panic!("inner panic must reach the caller"));
        });
        assert!(outcome.is_err(), "the panic must not be swallowed");
    }
}
