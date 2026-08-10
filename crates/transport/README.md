# Transport

`crates/transport` owns OSL's Tor routing boundary. Pointer construction and
IPC remain in `crates/ipc`.

## Tor integration

The hub supervises the bundled `osl-tor-sidecar`. The sidecar binds an
OS-chosen loopback port and reports that exact address in its newline-delimited
JSON status stream. This crate reads and validates the `listening` event, then
routes OSL-owned HTTP(S) traffic through that owned address with Reqwest
`Proxy::all`; destination names are resolved through SOCKS and HTTPS is covered
too. No well-known browser proxy port participates in this route. When Tor is
selected, an unavailable or malformed sidecar must fail closed.
