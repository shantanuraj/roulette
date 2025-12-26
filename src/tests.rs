use super::*;

fn test_keys() -> Vec<String> {
    vec![
        "2022-01-01_00-00-00_UTC.jpg",
        "2023-06-15_12-30-00_UTC.jpg",
        "2024-01-01_00-00-00_UTC.jpg",
        "2024-06-15_12-30-00_UTC.jpg",
        "2025-01-01_00-00-00_UTC.jpg",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

#[test]
fn parse_valid_json() {
    let json = r#"{"2024-01-01_UTC.jpg": "abc.jpg", "2023-01-01_UTC.jpg": "def.jpg"}"#;
    let map = ImageMap::parse(json).unwrap();
    assert_eq!(map.sorted_keys.len(), 2);
    assert_eq!(map.sorted_keys[0], "2023-01-01_UTC.jpg");
    assert_eq!(map.sorted_keys[1], "2024-01-01_UTC.jpg");
}

#[test]
fn parse_invalid_json() {
    assert!(ImageMap::parse("not json").is_err());
}

#[test]
fn filter_after_year() {
    let keys = test_keys();
    let filtered = filter_after(&keys, "2024");
    assert_eq!(filtered.len(), 3);
    assert!(filtered.iter().all(|k| k.as_str() >= "2024"));
}

#[test]
fn filter_after_year_month() {
    let keys = test_keys();
    let filtered = filter_after(&keys, "2024-06");
    assert_eq!(filtered.len(), 2);
}

#[test]
fn filter_after_full_date() {
    let keys = test_keys();
    let filtered = filter_after(&keys, "2024-06-15");
    assert_eq!(filtered.len(), 2);
}

#[test]
fn filter_after_future_returns_empty() {
    let keys = test_keys();
    let filtered = filter_after(&keys, "2030");
    assert!(filtered.is_empty());
}

#[test]
fn filter_after_past_returns_all() {
    let keys = test_keys();
    let filtered = filter_after(&keys, "2000");
    assert_eq!(filtered.len(), keys.len());
}

#[test]
fn select_uniform_empty() {
    assert!(select_uniform(&[]).is_none());
}

#[test]
fn select_uniform_returns_valid_key() {
    let keys = test_keys();
    let selected = select_uniform(&keys).unwrap();
    assert!(keys.iter().any(|k| k == selected));
}

#[test]
fn select_biased_empty() {
    assert!(select_biased(&[]).is_none());
}

#[test]
fn select_biased_returns_valid_key() {
    let keys = test_keys();
    let selected = select_biased(&keys).unwrap();
    assert!(keys.iter().any(|k| k == selected));
}

#[test]
fn hash_deterministic() {
    let content = r#"{"a": "b"}"#;
    assert_eq!(hash_content(content), hash_content(content));
}

#[test]
fn hash_differs_for_different_content() {
    assert_ne!(hash_content(r#"{"a": "b"}"#), hash_content(r#"{"a": "c"}"#));
}

#[test]
fn parse_duration_seconds() {
    assert_eq!(parse_duration("60s"), Some(60));
}

#[test]
fn parse_duration_minutes() {
    assert_eq!(parse_duration("5m"), Some(300));
}

#[test]
fn parse_duration_hours() {
    assert_eq!(parse_duration("1h"), Some(3600));
}

#[test]
fn parse_duration_days() {
    assert_eq!(parse_duration("7d"), Some(604800));
}

#[test]
fn parse_duration_invalid() {
    assert_eq!(parse_duration("invalid"), None);
    assert_eq!(parse_duration("10x"), None);
}
