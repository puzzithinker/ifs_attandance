//! QR / URL parsing for 中介人一戶通 links.

use crate::model::AgentIdentity;
use thiserror::Error;
use url::Url;

/// Errors from [`parse_qr_url`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseError {
    #[error("empty input")]
    Empty,
    #[error("invalid QR: {reason}")]
    Invalid { reason: String },
}

impl ParseError {
    fn invalid(reason: impl Into<String>) -> Self {
        Self::Invalid {
            reason: reason.into(),
        }
    }
}

/// Trim input and extract `categoryCode` + `licenseNo`.
///
/// Both query parameters are required and must be non-empty after URL-decode and trim.
/// This is intentionally stricter than the Python app, which inserted empty strings.
///
/// Accepts:
/// - Full URLs with a query string
/// - Absolute URLs without a standard host if they still parse
/// - Bare query strings like `categoryCode=A&licenseNo=1` (with or without leading `?`)
pub fn parse_qr_url(input: &str) -> Result<AgentIdentity, ParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(ParseError::Empty);
    }

    let pairs = extract_query_pairs(trimmed)?;

    let category = first_non_empty_param(&pairs, "categoryCode").ok_or_else(|| {
        ParseError::invalid("missing or empty categoryCode")
    })?;
    let license_no = first_non_empty_param(&pairs, "licenseNo").ok_or_else(|| {
        ParseError::invalid("missing or empty licenseNo")
    })?;

    Ok(AgentIdentity::new(category, license_no))
}

fn extract_query_pairs(input: &str) -> Result<Vec<(String, String)>, ParseError> {
    // Try as full URL first.
    if let Ok(url) = Url::parse(input) {
        let pairs: Vec<(String, String)> = url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        if !pairs.is_empty() || url.query().is_some() {
            return Ok(pairs);
        }
        // URL with no query — fall through to bare-query attempt only if it looks like one.
    }

    // Bare query: "a=1&b=2" or "?a=1&b=2"
    let query = input.strip_prefix('?').unwrap_or(input);
    if query.contains('=') && !query.contains("://") {
        // url::form_urlencoded handles application/x-www-form-urlencoded (+ and %XX).
        let pairs: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        if pairs.is_empty() {
            return Err(ParseError::invalid("no query parameters"));
        }
        return Ok(pairs);
    }

    // Retry with a dummy base for scheme-relative or path-like scanner dumps.
    if let Ok(url) = Url::parse(&format!("https://qr.local/{}", input.trim_start_matches('/'))) {
        let pairs: Vec<(String, String)> = url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        if !pairs.is_empty() {
            return Ok(pairs);
        }
    }

    Err(ParseError::invalid(
        "could not parse categoryCode/licenseNo from input",
    ))
}

/// First value for `key` that is non-empty after trim (query keys are case-sensitive).
fn first_non_empty_param(pairs: &[(String, String)], key: &str) -> Option<String> {
    pairs
        .iter()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| v.trim())
        .find(|v| !v.is_empty())
        .map(|v| v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(cat: &str, lic: &str) -> AgentIdentity {
        AgentIdentity::new(cat, lic)
    }

    #[test]
    fn empty_and_whitespace_are_empty_error() {
        assert_eq!(parse_qr_url(""), Err(ParseError::Empty));
        assert_eq!(parse_qr_url("   "), Err(ParseError::Empty));
        assert_eq!(parse_qr_url("\t\n"), Err(ParseError::Empty));
    }

    #[test]
    fn valid_full_url() {
        let url = "https://example.hk/portal?categoryCode=IA&licenseNo=12345678";
        assert_eq!(
            parse_qr_url(url).unwrap(),
            id("IA", "12345678")
        );
    }

    #[test]
    fn valid_url_with_extra_params() {
        let url = "https://x.test/a?foo=1&categoryCode=BR&licenseNo=9&bar=2";
        assert_eq!(parse_qr_url(url).unwrap(), id("BR", "9"));
    }

    #[test]
    fn bare_query_string() {
        assert_eq!(
            parse_qr_url("categoryCode=IA&licenseNo=ABC").unwrap(),
            id("IA", "ABC")
        );
        assert_eq!(
            parse_qr_url("?categoryCode=IA&licenseNo=ABC").unwrap(),
            id("IA", "ABC")
        );
    }

    #[test]
    fn trims_outer_whitespace() {
        let url = "  https://example.hk/?categoryCode=IA&licenseNo=1  ";
        assert_eq!(parse_qr_url(url).unwrap(), id("IA", "1"));
    }

    #[test]
    fn trims_param_values() {
        let url = "https://example.hk/?categoryCode=%20IA%20&licenseNo=%201%20";
        assert_eq!(parse_qr_url(url).unwrap(), id("IA", "1"));
    }

    #[test]
    fn url_encoded_values() {
        // + is space in form-urlencoded query
        let url = "https://example.hk/?categoryCode=I%2BA&licenseNo=12%2B34";
        let got = parse_qr_url(url).unwrap();
        assert_eq!(got.category, "I+A");
        assert_eq!(got.license_no, "12+34");
    }

    #[test]
    fn first_value_wins_for_duplicate_keys() {
        let url = "https://example.hk/?categoryCode=FIRST&categoryCode=SECOND&licenseNo=1&licenseNo=2";
        assert_eq!(parse_qr_url(url).unwrap(), id("FIRST", "1"));
    }

    #[test]
    fn missing_category_is_invalid() {
        let err = parse_qr_url("https://example.hk/?licenseNo=1").unwrap_err();
        match err {
            ParseError::Invalid { reason } => assert!(reason.contains("categoryCode")),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn missing_license_is_invalid() {
        let err = parse_qr_url("https://example.hk/?categoryCode=IA").unwrap_err();
        match err {
            ParseError::Invalid { reason } => assert!(reason.contains("licenseNo")),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn empty_category_param_is_invalid() {
        // Python would insert category=''; we reject.
        let err = parse_qr_url("https://example.hk/?categoryCode=&licenseNo=1").unwrap_err();
        assert!(matches!(err, ParseError::Invalid { .. }));
    }

    #[test]
    fn empty_license_param_is_invalid() {
        let err = parse_qr_url("https://example.hk/?categoryCode=IA&licenseNo=").unwrap_err();
        assert!(matches!(err, ParseError::Invalid { .. }));
    }

    #[test]
    fn whitespace_only_params_are_invalid() {
        let err = parse_qr_url("https://example.hk/?categoryCode=%20%20&licenseNo=1").unwrap_err();
        assert!(matches!(err, ParseError::Invalid { .. }));
        let err = parse_qr_url("https://example.hk/?categoryCode=IA&licenseNo=%20").unwrap_err();
        assert!(matches!(err, ParseError::Invalid { .. }));
    }

    #[test]
    fn python_empty_defaults_regression_no_identity() {
        // Inputs that Python parse_qs would turn into empty strings and still insert.
        let cases = [
            "https://example.hk/",
            "https://example.hk/?foo=bar",
            "not a url at all",
            "garbage",
            "https://example.hk/?categoryCode=&licenseNo=",
        ];
        for c in cases {
            assert!(
                parse_qr_url(c).is_err(),
                "expected error for {c:?}"
            );
        }
    }

    #[test]
    fn fragment_does_not_supply_params() {
        // Query is empty; fragment is not used for category/license.
        let err = parse_qr_url("https://example.hk/#categoryCode=IA&licenseNo=1").unwrap_err();
        assert!(matches!(err, ParseError::Invalid { .. }));
    }

    #[test]
    fn different_categories_same_license_are_distinct() {
        // Documents the Python bug fix at the parse/identity layer.
        let a = parse_qr_url("https://x/?categoryCode=IA&licenseNo=999").unwrap();
        let b = parse_qr_url("https://x/?categoryCode=BR&licenseNo=999").unwrap();
        assert_ne!(a, b);
        assert_eq!(a.license_no, b.license_no);
    }

    #[test]
    fn http_and_https_both_work() {
        assert_eq!(
            parse_qr_url("http://h.example/?categoryCode=IA&licenseNo=1").unwrap(),
            id("IA", "1")
        );
    }

    #[test]
    fn unicode_category_and_license() {
        let u = "https://x/?categoryCode=%E4%B8%AD&licenseNo=%E7%B7%A8%E8%99%9F1";
        let got = parse_qr_url(u).unwrap();
        assert_eq!(got.category, "中");
        assert_eq!(got.license_no, "編號1");
    }

    #[test]
    fn plus_as_space_in_query() {
        // application/x-www-form-urlencoded: + → space, then trim → empty → invalid
        let err = parse_qr_url("https://x/?categoryCode=+++&licenseNo=1").unwrap_err();
        assert!(matches!(err, ParseError::Invalid { .. }));
    }

    #[test]
    fn order_independent_params() {
        assert_eq!(
            parse_qr_url("https://x/?licenseNo=99&categoryCode=IA").unwrap(),
            id("IA", "99")
        );
    }

    #[test]
    fn parse_error_display_is_informative() {
        let e = ParseError::Invalid {
            reason: "missing or empty categoryCode".into(),
        };
        let s = e.to_string();
        assert!(s.contains("categoryCode"));
        assert_eq!(ParseError::Empty.to_string(), "empty input");
    }

    #[test]
    fn only_category_whitespace_is_invalid() {
        let err = parse_qr_url("categoryCode=   &licenseNo=OK").unwrap_err();
        assert!(matches!(err, ParseError::Invalid { .. }));
    }
}
