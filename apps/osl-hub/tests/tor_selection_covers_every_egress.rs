//! Tor selected + no tunnel must stop every egress path, not only the sends
//! somebody remembered to gate.
//!
//! Each case here binds a listener, points the hub's configuration at it,
//! selects Tor without a tunnel, and then drives one real egress helper. The
//! assertion is deliberately two-sided: the call must fail *and* the listener
//! must never have accepted a connection. A refusal that arrives after the
//! TCP handshake has already left the machine is not a refusal -- the address
//! is already spent.
//!
//! These run in their own test binary on purpose. The interlock they exercise
//! is process-wide, so sealing it inside the library's test binary would
//! reach across into unrelated cases that legitimately build direct clients.

use osl_privacy_hub::tor_pref::{TorPreference, TorPreferenceState};
use std::net::TcpListener;
use std::path::Path;
use std::sync::{Mutex, OnceLock, PoisonError};

/// One process, one route: these cases must not interleave.
fn serialized() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// A listener that stands in for the real service. Nothing may ever reach it.
struct ClearnetWitness {
    listener: TcpListener,
    port: u16,
}

impl ClearnetWitness {
    fn bind() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a clearnet witness");
        listener
            .set_nonblocking(true)
            .expect("make the witness probe nonblocking");
        let port = listener
            .local_addr()
            .expect("read the witness address")
            .port();
        Self { listener, port }
    }

    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn assert_nothing_connected(&self, what: &str) {
        assert!(
            self.listener.accept().is_err(),
            "{what} opened a direct connection while Tor was selected with no tunnel"
        );
    }
}

/// Point both service URLs at the witness and select Tor with no tunnel.
///
/// The returned guard restores the process default when the case ends,
/// including on a panic.
fn tor_selected_without_a_tunnel(
    directory: &Path,
    witness: &ClearnetWitness,
) -> (TorPreferenceState, keystore::egress::RouteRestoreGuard) {
    let restore = keystore::egress::restore_clearnet_on_drop();
    std::fs::write(
        directory.join("keyserver.json"),
        format!(
            r#"{{"base_url":"{url}","cipher_store_url":"{url}"}}"#,
            url = witness.base_url()
        ),
    )
    .expect("write the local service route");
    let state = TorPreferenceState::load(directory.join("tor-preference.json"));
    state
        .set_preference(TorPreference::Tor)
        .expect("persist the Tor choice");
    assert!(
        state.authorize_store().is_err(),
        "the pre-existing send gate must still refuse"
    );
    (state, restore)
}

#[test]
fn the_attachment_transport_refuses_instead_of_uploading_direct() {
    let _serial = serialized();
    let directory = tempfile::tempdir().expect("temporary configuration directory");
    let witness = ClearnetWitness::bind();
    let (_state, _restore) = tor_selected_without_a_tunnel(directory.path(), &witness);

    let attempted = ipc::cipher_store_client::CipherStoreClient::new(
        ipc::cipher_store_client::resolve_cipher_store_base_url(directory.path()),
    )
    .and_then(|client| {
        client.upload(
            b"ciphertext",
            ipc::cipher_store_client::TTL_1H,
            &[7_u8; ipc::cipher_store_client::FETCH_TOKEN_BYTES],
        )
    });

    assert!(
        attempted.is_err(),
        "an attachment upload must refuse while Tor is selected with no tunnel"
    );
    witness.assert_nothing_connected("the attachment transport");
}

#[test]
fn the_inbox_and_burn_transport_refuses_instead_of_posting_direct() {
    let _serial = serialized();
    let directory = tempfile::tempdir().expect("temporary configuration directory");
    let witness = ClearnetWitness::bind();
    let (_state, _restore) = tor_selected_without_a_tunnel(directory.path(), &witness);

    // Every inbox drain, control-inbox post and keyserver-side burn in the hub
    // reaches the network through a client built exactly like this one.
    let attempted = keystore::KeyServerClient::new(witness.base_url())
        .and_then(|client| client.fetch_pubkeys("0000000000000000000"));

    assert!(
        attempted.is_err(),
        "an inbox or burn request must refuse while Tor is selected with no tunnel"
    );
    witness.assert_nothing_connected("the key server transport");
}

#[test]
fn the_username_directory_refuses_instead_of_looking_up_direct() {
    let _serial = serialized();
    let directory = tempfile::tempdir().expect("temporary configuration directory");
    let witness = ClearnetWitness::bind();
    let (_state, _restore) = tor_selected_without_a_tunnel(directory.path(), &witness);

    // A directory lookup carries the name of somebody the user is about to
    // talk to, so it is exactly the kind of request that must not slip out.
    let attempted = keystore::username::Resolver::new(&witness.base_url()).and_then(|resolver| {
        resolver
            .resolve("someone")
            .map(|resolved| resolved.map(|identity| identity.user_id))
    });

    assert!(
        attempted.is_err(),
        "a username lookup must refuse while Tor is selected with no tunnel"
    );
    witness.assert_nothing_connected("the username directory");
}

#[test]
fn a_rollback_burn_refuses_instead_of_deleting_direct() {
    let _serial = serialized();
    let directory = tempfile::tempdir().expect("temporary configuration directory");
    let witness = ClearnetWitness::bind();
    let (_state, _restore) = tor_selected_without_a_tunnel(directory.path(), &witness);

    // The burn that undoes a failed send is the worst path to leak: it names a
    // blob that has already been uploaded, so a direct DELETE ties that upload
    // to this device's real address.
    let attempted = ipc::prose_token::prose_token_burn_id(
        directory.path(),
        &[3_u8; 32],
        "00112233445566778899aabbccddeeff",
    );

    assert!(
        attempted.is_err(),
        "a rollback burn must refuse while Tor is selected with no tunnel"
    );
    witness.assert_nothing_connected("the rollback burn");
}

#[test]
fn choosing_direct_again_reopens_the_ordinary_route() {
    let _serial = serialized();
    let directory = tempfile::tempdir().expect("temporary configuration directory");
    let witness = ClearnetWitness::bind();
    let (state, _restore) = tor_selected_without_a_tunnel(directory.path(), &witness);

    // Fail-closed must not mean fail-permanently: a user who switches back to
    // a direct route gets a working client again.
    state
        .set_preference(TorPreference::Direct)
        .expect("persist the direct choice");
    assert!(
        ipc::cipher_store_client::CipherStoreClient::new(witness.base_url()).is_ok(),
        "a direct route must still build an ordinary client"
    );
}
