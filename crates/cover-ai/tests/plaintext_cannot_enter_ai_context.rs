#[test]
fn plaintext_cannot_enter_ai_context() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/plaintext_cannot_enter_ai_context.rs");
}
