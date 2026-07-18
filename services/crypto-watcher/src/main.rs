use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::{pkcs8::DecodePrivateKey, SigningKey};
use osl_crypto_watcher::{
    create_invoice_handler, read_secret_file, AppState, BitcoinConfig, Config, CoreWalletRpc,
    InvoiceStore, MoneroConfig, WalletRpc,
};
use std::{env, net::SocketAddr, path::Path, sync::Arc, time::Duration};
use url::Url;

fn required(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("missing {name}"))
}

fn credential(file_name: &str, legacy_name: &str) -> String {
    match env::var(file_name) {
        Ok(path) => read_secret_file(Path::new(&path))
            .unwrap_or_else(|_| panic!("invalid credential file configured by {file_name}")),
        Err(env::VarError::NotPresent) => required(legacy_name),
        Err(env::VarError::NotUnicode(_)) => panic!("invalid {file_name}"),
    }
}

fn explicitly_enabled(name: &str) -> bool {
    match env::var(name) {
        Ok(value) if value == "true" => true,
        Ok(value) if value == "false" => false,
        Err(env::VarError::NotPresent) => false,
        _ => panic!("{name} must be exactly true or false"),
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let request_secret = credential(
        "CRYPTO_WATCHER_REQUEST_SECRET_FILE",
        "CRYPTO_WATCHER_REQUEST_SECRET",
    )
    .into_bytes();
    let settlement_signing_key_pem = read_secret_file(Path::new(&required(
        "CRYPTO_WATCHER_SETTLEMENT_SIGNING_KEY_FILE",
    )))
    .expect("invalid settlement signing key file");
    let settlement_signing_key = SigningKey::from_pkcs8_pem(&settlement_signing_key_pem)
        .expect("settlement signing key must be Ed25519 PKCS#8 PEM");
    let db_key: [u8; 32] = BASE64
        .decode(credential(
            "CRYPTO_WATCHER_DB_KEY_FILE",
            "CRYPTO_WATCHER_DB_KEY_B64",
        ))
        .expect("db key base64")
        .try_into()
        .expect("db key must be 32 bytes");
    let bitcoin = explicitly_enabled("CRYPTO_BTC_ENABLED").then(|| BitcoinConfig {
        bitcoin_rpc_url: Url::parse(&required("BITCOIN_RPC_URL")).expect("bitcoin URL"),
        bitcoin_cookie_file: required("BITCOIN_COOKIE_FILE"),
        bitcoin_wallet: required("BITCOIN_WATCH_WALLET"),
    });
    let monero = explicitly_enabled("CRYPTO_XMR_ENABLED").then(|| MoneroConfig {
        monero_wallet_rpc_url: Url::parse(&required("MONERO_WALLET_RPC_URL")).expect("monero URL"),
        monero_account_index: env::var("MONERO_ACCOUNT_INDEX")
            .unwrap_or_else(|_| "0".into())
            .parse()
            .expect("account index"),
        monero_primary_address: required("MONERO_PRIMARY_ADDRESS"),
    });
    let config = Arc::new(Config {
        bitcoin,
        monero,
        callback_url: Url::parse(&required("CRYPTO_SETTLEMENT_CALLBACK_URL"))
            .expect("callback URL"),
        request_secret,
        settlement_signing_key,
        btc_confirmations: env::var("CRYPTO_BTC_CONFIRMATIONS")
            .or_else(|_| env::var("BTC_CONFIRMATIONS"))
            .unwrap_or_else(|_| "2".into())
            .parse()
            .expect("btc confirmations"),
        xmr_confirmations: env::var("CRYPTO_XMR_CONFIRMATIONS")
            .or_else(|_| env::var("XMR_CONFIRMATIONS"))
            .unwrap_or_else(|_| "10".into())
            .parse()
            .expect("xmr confirmations"),
        invoice_retention_seconds: env::var("INVOICE_RETENTION_SECONDS")
            .unwrap_or_else(|_| "604800".into())
            .parse()
            .expect("retention"),
    });
    config.validate().expect("unsafe watcher configuration");
    let wallets: Arc<dyn WalletRpc> = Arc::new(CoreWalletRpc::new(config.clone()));
    wallets
        .validate_watch_only()
        .await
        .expect("wallet validation failed");
    let store = Arc::new(
        InvoiceStore::open(Path::new(&required("CRYPTO_WATCHER_DB")), &db_key)
            .expect("database unavailable"),
    );
    let state = AppState {
        config: config.clone(),
        wallets,
        store,
        client: reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .build()
            .unwrap(),
    };
    let poll_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(15));
        loop {
            interval.tick().await;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            if let Err(error) = poll_state.poll_once(now).await {
                tracing::error!(%error,"payment poll failed");
            }
        }
    });
    let app = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/v1/invoices", post(create_invoice_handler))
        // Invoice requests contain only a few small scalar fields. Bound the
        // body before the Bytes extractor and HMAC verification allocate it.
        .layer(DefaultBodyLimit::max(16 * 1024))
        .with_state(state);
    let address: SocketAddr = env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8789".into())
        .parse()
        .expect("listen addr");
    assert!(
        address.ip().is_loopback(),
        "watcher must listen on loopback behind an authenticated tunnel"
    );
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("listen failed");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .expect("server failed");
}
