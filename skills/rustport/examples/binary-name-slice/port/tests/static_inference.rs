//! Expectations recovered from assembly, NOT original runtime golden outputs.
use openmmd_name_slice::{validate_name as v, NameError::*};

#[test]
fn empty_policy_and_error_precedence() {
    assert_eq!(v("", false), Err(Empty));
    assert_eq!(v("", true), Ok(()));
    assert_eq!(v(" ;", false), Err(Unsupported));
    assert_eq!(v(" ", false), Err(EdgeWhitespace));
}
#[test]
fn byte_limit() {
    assert_eq!(v(&"a".repeat(4096), false), Ok(()));
    assert_eq!(v(&"a".repeat(4097), false), Err(Unsupported));
    assert_eq!(v(&"骨".repeat(1365), false), Ok(()));
    assert_eq!(v(&"骨".repeat(1366), false), Err(Unsupported));
}
#[test]
fn syntax_and_unicode() {
    for n in [
        "bone//comment",
        "a;b",
        "a{b",
        "a}b",
        "\0",
        "\n",
        "\u{7f}",
        "\u{9f}",
    ] {
        assert_eq!(v(n, false), Err(Unsupported), "{n:?}");
    }
    for n in ["骨", "a/b", "a b", "a\\b", "a\u{a0}b", "😀"] {
        assert_eq!(v(n, false), Ok(()), "{n:?}");
    }
    for n in [" bone", "bone ", "\u{a0}bone", "bone\u{3000}"] {
        assert_eq!(v(n, false), Err(EdgeWhitespace), "{n:?}");
    }
}
#[test]
#[ignore = "No original runtime oracle: native probes timed out; no differential fixtures exist"]
fn original_differential_acceptance() {
    panic!("Blocked: capture original results before implementing this acceptance test");
}
