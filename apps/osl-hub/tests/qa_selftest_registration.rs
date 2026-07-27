#![cfg(all(feature = "core", feature = "discord-qa-shell"))]

#[test]
fn dual_feature_build_exports_the_qa_selftest_request_module() {
    use osl_privacy_hub::qa_selftest_request::{parse_request, ParsedRequest, Verb, FORMAT_LEGACY};

    let ParsedRequest::Accepted(request) = parse_request("") else {
        panic!("the externally exported QA parser refused its legacy send trigger");
    };
    assert_eq!(request.verb, Verb::Send);
    assert_eq!(request.format, FORMAT_LEGACY);
}
