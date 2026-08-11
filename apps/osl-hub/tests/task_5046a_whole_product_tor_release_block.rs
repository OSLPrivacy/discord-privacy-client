// TASK 5046a: whole-product Tor routing release blocker.
//
// The older Tor gate inventories a handful of `reqwest` constructors.  That
// is not a process-tree inventory: WebView2, launched browsers/native provider
// clients, raw sockets, updater code and their descendants can all originate
// traffic without constructing one of those clients.  Until a packaged
// Windows run supplies the packet/process evidence required by TASK 5046a,
// this test keeps the release blocked and names every independently discovered
// gap.  It must not be converted into a positive Tor certification test.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Finding {
    entry_point: &'static str,
    path: &'static str,
    marker: &'static str,
    refusal: &'static str,
}

/// Source/package inventory classes. `local-network` is currently a promised
/// UI path with dormant socket code and missing registered commands; it stays
/// in the inventory because silently dropping it would evade the required
/// Tor/local-network transition proof.
const DISCOVERED_NETWORK_CLASSES: &[&str] = &[
    "dns",
    "keyserver",
    "directory",
    "private-link",
    "identity-key",
    "control-protected-message",
    "cipher-blob",
    "cipher-attachment",
    "osl-mail",
    "updater",
    "embedded-web-provider",
    "native-provider-child",
    "browser-child",
    "tor-bootstrap",
    "external-helper-child",
    "local-network",
];

/// Fixed from independent package/process-tree and source/socket inspection.
///
/// This is intentionally broader than an HTTP-constructor census. Removing an
/// entry to make the test green is a release-gate regression; the entry may be
/// removed only after the unchanged packaged capture has observed its real,
/// non-empty operation through authenticated Tor and its dedicated leak mutant.
const RELEASE_BLOCKERS: &[Finding] = &[
    Finding {
        entry_point: "tor-bootstrap-packaged-startup",
        path: "apps/osl-hub/src/main.rs",
        marker: "tor_pref::arti_proxy_config_from_env()",
        refusal: "packaged startup does not resolve or launch the bundled Tor sidecar",
    },
    Finding {
        entry_point: "tor-socks-authentication",
        path: "crates/transport/src/tor.rs",
        marker: "socks5h://{socks_addr}",
        refusal: "OSL's SOCKS client supplies no sidecar authentication credential",
    },
    Finding {
        entry_point: "provider-webviews",
        path: "apps/osl-hub/src/service_host.rs",
        marker: "WebviewUrl::External(initial_url)",
        refusal: "remote provider WebViews are not bound to the authenticated Tor client",
    },
    Finding {
        entry_point: "provider-native-children",
        path: "apps/osl-hub/src/native_window_host.rs",
        marker: "Command::new(spec.executable.path())",
        refusal: "launched native provider process trees have no enforced Tor egress policy",
    },
    Finding {
        entry_point: "provider-firefox-and-installers",
        path: "apps/osl-hub/src/native_apps.rs",
        marker: "Command::new(executable.path())",
        refusal: "launched Firefox/provider/installer children have no enforced Tor egress policy",
    },
    Finding {
        entry_point: "browser-companion-child",
        path: "apps/osl-hub/src/browser_companion.rs",
        marker: "Command::new(executable.path())",
        refusal:
            "browser companion descendants can perform DNS, update, crash and telemetry egress",
    },
    Finding {
        entry_point: "website-driver-child",
        path: "apps/osl-hub/src/website_driver.rs",
        marker: "Command::new(&browser_executable)",
        refusal: "website-driver browser descendants are outside the Tor interlock",
    },
    Finding {
        entry_point: "protected-realtime-dns-tcp",
        path: "apps/osl-hub/src/realtime_pipe.rs",
        marker: ".to_socket_addrs()",
        refusal: "realtime protected messaging performs host DNS and raw TCP directly",
    },
    Finding {
        entry_point: "local-network-tcp-udp",
        path: "apps/osl-hub/src/osl_lan.rs",
        marker: "UdpSocket::bind",
        refusal: "no runtime proof stops LAN sockets/listeners before Tor activity",
    },
    Finding {
        entry_point: "tauri-updater",
        path: "apps/osl-hub/src/main.rs",
        marker: ".updater()",
        refusal: "the Tauri updater transport is not bound to the authenticated Tor client",
    },
    Finding {
        entry_point: "site-update-manifest",
        path: "crates/ipc/src/commands.rs",
        marker: "fn check_site_for_update(",
        refusal: "the direct site updater builds a non-interlocked HTTP client",
    },
    Finding {
        entry_point: "site-update-download",
        path: "crates/ipc/src/commands.rs",
        marker: "pub fn cmd_osl_install_site_update(",
        refusal: "the direct site update download builds a non-interlocked HTTP client",
    },
    Finding {
        entry_point: "external-url-handler-children",
        path: "apps/osl-hub/src/main.rs",
        marker: "std::process::Command::new(\"xdg-open\")",
        refusal: "default-browser child processes are outside the Tor interlock",
    },
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("apps/osl-hub is two levels below the workspace root")
        .to_path_buf()
}

/// Inventory A: the package/process-tree review's fixed entry-point names.
fn packaged_process_tree_inventory() -> BTreeSet<&'static str> {
    RELEASE_BLOCKERS
        .iter()
        .map(|finding| finding.entry_point)
        .collect()
}

/// Inventory B: source/socket discovery, admitted only when its exact shipping
/// marker is still present.  Comparing it with Inventory A prevents a missing
/// file or renamed/uninspected implementation from being treated as closure.
fn source_and_socket_inventory(root: &Path) -> BTreeSet<&'static str> {
    RELEASE_BLOCKERS
        .iter()
        .filter_map(|finding| {
            let source = std::fs::read_to_string(root.join(finding.path)).unwrap_or_default();
            source
                .contains(finding.marker)
                .then_some(finding.entry_point)
        })
        .collect()
}

#[test]
fn independent_nonempty_inventories_agree_and_keep_release_blocked() {
    let root = workspace_root();
    let packaged = packaged_process_tree_inventory();
    let source = source_and_socket_inventory(&root);

    assert!(
        !packaged.is_empty(),
        "TASK5046A inventory must be non-empty"
    );
    let classes = DISCOVERED_NETWORK_CLASSES
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        classes.len(),
        16,
        "network class inventory changed or contains a duplicate"
    );
    assert_eq!(
        source, packaged,
        "TASK5046A source/socket discovery changed. Do not shrink the fixed process-tree inventory; inspect and classify the changed entry point."
    );

    for finding in RELEASE_BLOCKERS {
        println!(
            "TASK5046A_BLOCKED entry_point={} path={} refusal={}",
            finding.entry_point, finding.path, finding.refusal
        );
    }
    println!(
        "TASK5046A_FINISH release_blocked=true network_classes={} blocker_inventory_a={} blocker_inventory_b={} authenticated_tor_receipts=0 observed_host_capture_packets=not_run required_leak_mutations=4",
        classes.len(),
        packaged.len(),
        source.len(),
    );
}

#[test]
fn old_reqwest_census_cannot_certify_the_product() {
    let root = workspace_root();
    let tor_gate = std::fs::read_to_string(root.join("apps/osl-hub/src/tor_pref.rs"))
        .expect("read the prior Tor gate");
    let updater = std::fs::read_to_string(root.join("crates/ipc/src/commands.rs"))
        .expect("read updater source");
    let realtime = std::fs::read_to_string(root.join("apps/osl-hub/src/realtime_pipe.rs"))
        .expect("read realtime socket source");

    assert!(
        tor_gate.contains("fn shipping_region(source: &str) -> &str"),
        "the prior census changed; re-audit it before changing this blocker"
    );
    assert!(
        updater
            .matches("reqwest::blocking::Client::builder()")
            .count()
            >= 2,
        "update manifest and download constructors changed; re-audit both"
    );
    assert!(
        realtime.contains(".to_socket_addrs()") && realtime.contains("TcpStream::connect_timeout"),
        "realtime DNS/TCP path changed; re-audit it"
    );

    println!(
        "TASK5046A_PRIOR_GATE_INSUFFICIENT updater_direct_builders={} raw_realtime_dns=true raw_realtime_tcp=true",
        updater.matches("reqwest::blocking::Client::builder()").count()
    );
}
