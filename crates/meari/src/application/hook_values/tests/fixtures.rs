//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;

pub(super) fn fails<T>(result: Result<T, Error>, expected: Error) {
    match result {
        Err(actual) => assert_eq!(actual, expected),
        Ok(_) => panic!("expected {expected:?}"),
    }
}

pub(super) fn readers_fail(candidate: &Transaction, value: &Value, expected: Error) {
    fails(candidate.kind(value), expected);
    fails(candidate.field(value, "field"), expected);
    fails(candidate.element(value, 0), expected);
    fails(candidate.read_number(value), expected);
    fails(candidate.read_boolean(value), expected);
    fails(candidate.read_text(value), expected);
}
