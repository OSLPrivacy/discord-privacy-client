//! TASK 4906 package-run controls.
//!
//! The staged external binary is supplied by the focused command below.  The
//! test copies it into the exact filename a Tauri package exposes, starts it
//! with no OSL_ARTI_PROXY_PATH override, then proves that removing that file
//! fails by its absolute packaged pathname rather than a PATH lookup.

use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;

use transport::tor::{ArtiProxyConfig, TorError, TorTransport};

fn package_dir() -> PathBuf {
    let directory = std::env::temp_dir().join(format!("osl-task-4906-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("create package fixture directory");
    directory
}

fn unused_loopback_address() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("reserve a loopback port");
    let address = listener.local_addr().expect("read reserved port");
    drop(listener);
    address
}

#[test]
fn packaged_sidecar_starts_when_the_environment_override_is_unset() {
    assert!(
        std::env::var_os("OSL_ARTI_PROXY_PATH").is_none(),
        "this control must run with no environment sidecar override"
    );
    let source = PathBuf::from(
        std::env::var_os("OSL_TOR_SIDECAR_TEST_BINARY")
            .expect("test command supplies the staged Tor sidecar"),
    );
    let directory = package_dir();
    let sidecar = directory.join("osl-tor-sidecar");
    std::fs::copy(&source, &sidecar).expect("put sidecar into packaged filename");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&sidecar, std::fs::Permissions::from_mode(0o755))
            .expect("make packaged sidecar executable");
    }
    let address = unused_loopback_address();
    let mut config = ArtiProxyConfig::new(&sidecar);
    config.socks_addr = address;
    config.args = vec![
        "--dial-mode".to_owned(),
        "direct".to_owned(),
        "--listen".to_owned(),
        address.to_string(),
    ];

    let transport = TorTransport::start(config).expect("packaged sidecar starts by its filename");
    transport.stop();
    std::fs::remove_dir_all(directory).expect("remove package fixture");
}

#[test]
fn removed_packaged_sidecar_refuses_by_its_absolute_name() {
    let directory = package_dir();
    let sidecar = directory.join("osl-tor-sidecar");
    let error = match TorTransport::start(ArtiProxyConfig::new(&sidecar)) {
        Ok(_) => panic!("a removed package sidecar must not use PATH"),
        Err(error) => error,
    };
    match error {
        TorError::Spawn { program, .. } => {
            assert_eq!(program, sidecar);
            assert!(
                program.is_absolute(),
                "missing sidecar must be an absolute package path"
            );
        }
        other => panic!("expected named sidecar spawn error, got {other}"),
    }
    std::fs::remove_dir_all(directory).expect("remove package fixture");
}
