# Transport

`crates/transport` owns OSL's Tor routing boundary. Pointer construction and
IPC remain in `crates/ipc`.

## Tor integration

T1-70 chose a supervised Arti SOCKS5 proxy subprocess. OSL-owned HTTP(S)
traffic will use `socks5h://127.0.0.1:9150` through Reqwest `Proxy::all`, so
the proxy resolves destination names and HTTPS is covered too. When Tor is
selected, an unavailable proxy must fail closed; traffic must not fall back to
the clearnet.

Embedding `arti-client` is intentionally out of scope: it requires Rust 1.91
and edition 2024, while this workspace is pinned to Rust 1.88 and edition
2021. `arti-hyper` is not an acceptable integration boundary.
