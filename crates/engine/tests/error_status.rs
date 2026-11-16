//! The error-to-status table.
//!
//! SCOUT section implemented: SCOUT-NATIVE §6. The mapping itself lives in
//! GN4's route (`src/routes/search.rs::engine_error`), so it is mirrored
//! here rather than exported by the crate: this file is a guard, not an
//! API. Both functions below are exhaustive matches, which is the point —
//! adding a variant to [`gn_search_engine::SearchError`] stops compiling until
//! somebody decides what status it means and what code it carries.

use gn_search_engine::SearchError;

/// The status GN4 answers with for this error (SCOUT-NATIVE §6, the table
/// in `src/search/mod.rs:62-68`).
fn gn4_status(error: &SearchError) -> u16 {
    match error {
        SearchError::InvalidQuery { .. } => 422,
        SearchError::Unreachable(_) => 502,
        SearchError::Misbehaving(_) => 502,
        SearchError::Backend(_) => 502,
        SearchError::Internal(_) => 500,
    }
}

/// The machine-readable code GN4 puts in the body (SCOUT-NATIVE §6).
fn gn4_code(error: &SearchError) -> &'static str {
    match error {
        SearchError::InvalidQuery { .. } => "invalid_query",
        SearchError::Unreachable(_) => "backend_unreachable",
        SearchError::Misbehaving(_) => "backend_misbehaving",
        SearchError::Backend(_) => "backend_error",
        SearchError::Internal(_) => "internal_error",
    }
}

/// The whole table: 422 / 502 / 502 / 502 / 500, one row per variant.
#[test]
fn every_variant_maps_to_the_status_gn4_expects() {
    let cases = [
        (
            SearchError::InvalidQuery {
                message: "Syntax: <expression> AND <expression>".to_string(),
                xapian_error_type: Some("QueryParserError".to_string()),
            },
            422,
            "invalid_query",
        ),
        (
            SearchError::Unreachable("xapian index at /x is not a directory".to_string()),
            502,
            "backend_unreachable",
        ),
        (
            SearchError::Misbehaving("xapian doc data is not a search hit: x".to_string()),
            502,
            "backend_misbehaving",
        ),
        (
            SearchError::Backend("gn3 search returned HTTP 500".to_string()),
            502,
            "backend_error",
        ),
        (
            SearchError::Internal("native search task failed to join: x".to_string()),
            500,
            "internal_error",
        ),
    ];
    for (error, status, code) in cases {
        assert_eq!(gn4_status(&error), status, "status for {error:?}");
        assert_eq!(gn4_code(&error), code, "code for {error:?}");
    }
}

/// The diagnostic text is reachable without matching the variant, because
/// GN4 puts exactly this string in the response body's `detail`
/// (SCOUT-NATIVE §6).
#[test]
fn message_is_the_response_detail() {
    let error = SearchError::InvalidQuery {
        message: "InvalidOperationError: Query is invalid (invalid Hs/Mm location field)"
            .to_string(),
        xapian_error_type: Some("InvalidOperationError".to_string()),
    };
    assert_eq!(
        error.message(),
        "InvalidOperationError: Query is invalid (invalid Hs/Mm location field)"
    );
    assert_eq!(error.xapian_error_type(), Some("InvalidOperationError"));
    assert_eq!(
        error.to_string(),
        "InvalidOperationError: Query is invalid (invalid Hs/Mm location field) \
         (InvalidOperationError)"
    );

    let unreachable = SearchError::Unreachable("index is missing".to_string());
    assert_eq!(unreachable.message(), "index is missing");
    assert_eq!(unreachable.xapian_error_type(), None);
    assert_eq!(unreachable.to_string(), "index is missing");
}

/// A variant without an exception class still displays without one.
#[test]
fn invalid_query_without_a_class_displays_the_message_alone() {
    let error = SearchError::InvalidQuery {
        message: "Unknown range operation".to_string(),
        xapian_error_type: None,
    };
    assert_eq!(error.to_string(), "Unknown range operation");
    assert_eq!(gn4_status(&error), 422);
}

/// Every variant is a `std::error::Error`, so GN4 can keep using it where
/// its own error trait is expected.
#[test]
fn variants_are_std_errors() {
    fn assert_error<E: std::error::Error>(_: &E) {}
    assert_error(&SearchError::Internal("x".to_string()));
    assert_error(&SearchError::Backend("x".to_string()));
}
