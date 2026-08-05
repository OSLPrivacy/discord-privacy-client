//! Regression test for defect D5: the startup panic
//! "Cannot drop a runtime in a context where blocking is not allowed."
//!
//! Every `reqwest::blocking` operation goes through
//! `reqwest::blocking::wait::timeout`, whose debug-assertions `enter()` builds
//! a throwaway current-thread Tokio runtime and drops it on the calling
//! thread. Tokio refuses that drop while the thread is inside another
//! runtime's entered region and panics. Any `async fn` Tauri command that
//! reaches a keyserver call (onboarding's username claim is the one observed)
//! runs in exactly that region, so every such call took the panic — which then
//! poisoned the `AppState` mutex the frame held.
//!
//! `Runtime::block_on` enters the runtime the same way a worker polling a task
//! does, so it reproduces the fault without any scheduling nondeterminism.
//!
//! No server is needed: the panic fires inside `wait::timeout` *before* the
//! connection outcome is known, so a refused connection is a perfectly good
//! probe. These tests assert only that a `Result` comes back at all — the
//! request is expected to fail.

use std::net::TcpListener;

/// A loopback URL with nothing listening on it: bind to get a
/// kernel-assigned free port, then drop the listener.
fn closed_loopback_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback probe port");
    let port = listener.local_addr().expect("read the probe port").port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .build()
        .expect("build a test Tokio runtime")
}

#[test]
fn keyserver_client_can_be_built_inside_an_async_context() {
    let url = closed_loopback_url();
    let built = runtime().block_on(async { keystore::KeyServerClient::new(&url).is_ok() });
    assert!(
        built,
        "the keyserver client must build from an async context"
    );
}

#[test]
fn keyserver_request_from_an_async_context_returns_instead_of_panicking() {
    let url = closed_loopback_url();
    let identity = keystore::generate_identity("999000111222333444".to_owned());

    // Pre-fix this panicked inside `send_request`; the value of the `Result`
    // is irrelevant, reaching one at all is the property under test.
    let outcome = runtime().block_on(async {
        let client = keystore::KeyServerClient::new(&url).expect("build the keyserver client");
        client.register(&identity)
    });
    assert!(
        outcome.is_err(),
        "nothing is listening on the probe port, so the request must fail cleanly"
    );
}

#[test]
fn keyserver_request_from_a_spawned_task_returns_instead_of_panicking() {
    let url = closed_loopback_url();

    // The production shape: an `async fn` Tauri command body polled on a
    // multi-thread worker, calling straight into blocking keyserver code.
    let outcome = runtime().block_on(async move {
        tokio::spawn(async move {
            let client = keystore::KeyServerClient::new(&url).expect("build the keyserver client");
            client.fetch_pubkeys("999000111222333444").is_err()
        })
        .await
    });
    assert!(
        outcome.expect("the task must complete rather than panic"),
        "the request must fail cleanly against a closed port"
    );
}

#[test]
fn username_resolution_from_an_async_context_returns_instead_of_panicking() {
    let url = closed_loopback_url();
    let outcome = runtime().block_on(async {
        keystore::username::Resolver::new(&url)
            .expect("build the username resolver")
            .resolve("osl-runtime-safety-probe")
    });
    assert!(
        outcome.is_err(),
        "nothing is listening on the probe port, so resolution must fail cleanly"
    );
}
