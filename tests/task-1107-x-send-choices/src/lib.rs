#[path = "../../../apps/osl-hub/src/x_send.rs"]
mod x_send;

// Compile the X preparation boundary independently of the desktop application's
// optional local-model dependency. The module's focused test verifies every
// accepted choice and its fail-closed unknown-choice branch.
