#[test]
fn plaintext_cannot_enter_ai_context() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/plaintext_cannot_enter_ai_context.rs");
}

/// T13-C5's specified sabotage -- "feed a plaintext message in; the type-level
/// test fails to compile" -- which had no gate until now. It is only expressible
/// because `cover_history` is a declared module: an undeclared file cannot be
/// named by a `use`, so for as long as the module was reachable solely through a
/// `#[path]` recompile, the barrier it exists to enforce was asserted in prose
/// and graded by nothing.
#[test]
fn plaintext_cannot_enter_cover_history() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/plaintext_cannot_enter_cover_history.rs");
}
