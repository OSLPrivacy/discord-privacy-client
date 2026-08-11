// Keep the audit gate in the product package's test directory while allowing
// the small transport crate to run it without compiling the desktop/webview
// dependency tree. The test itself uses only std and inspects shipping source.
include!("../../../apps/osl-hub/tests/task_5046a_whole_product_tor_release_block.rs");
