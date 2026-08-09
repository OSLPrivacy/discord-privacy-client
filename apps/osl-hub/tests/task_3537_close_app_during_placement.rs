// The decision boundary is intentionally dependency-free.  Compiling the
// production file here keeps the task check focused on the close-before-mark
// behavior rather than requiring a native desktop/WebView build.
#[path = "../src/placement_close.rs"]
mod placement_close;
