//! URL extraction for calendar event notes.
//!
//! Used by `Action::OpenEventLinks` to pull http(s) URLs out of event notes
//! and the event location field, deduplicating against the conference URL.

use regex::Regex;
use std::sync::OnceLock;

static URL_REGEX: OnceLock<Regex> = OnceLock::new();

fn url_regex() -> &'static Regex {
    URL_REGEX.get_or_init(|| {
        // Matches http:// or https:// followed by non-whitespace characters,
        // excluding common trailing punctuation.
        Regex::new(r#"https?://[^\s<>"'\]\)]+"#).expect("valid regex")
    })
}

/// Extract unique URLs from free-form text, in order of first appearance.
pub fn extract_urls_from_text(text: &str) -> Vec<String> {
    let re = url_regex();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for m in re.find_iter(text) {
        let url = trim_trailing_punctuation(m.as_str());
        if seen.insert(url.to_string()) {
            out.push(url.to_string());
        }
    }
    out
}

/// Extract URLs from notes + location, dedup against `conference_url`, preserve order.
pub fn collect_event_urls(
    notes: Option<&str>,
    location: Option<&str>,
    conference_url: Option<&str>,
) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    if let Some(conf) = conference_url {
        seen.insert(conf.to_string());
    }
    let mut out = Vec::new();
    for text in [notes, location].into_iter().flatten() {
        for url in extract_urls_from_text(text) {
            if seen.insert(url.clone()) {
                out.push(url);
            }
        }
    }
    out
}

fn trim_trailing_punctuation(s: &str) -> &str {
    let trimmed = s.trim_end_matches(|c: char| matches!(c, '.' | ',' | ';' | ':' | '!' | '?'));
    trimmed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_single_url() {
        let urls = extract_urls_from_text("See https://example.com for details");
        assert_eq!(urls, vec!["https://example.com"]);
    }

    #[test]
    fn extracts_multiple_urls_in_order() {
        let urls = extract_urls_from_text("First: https://a.test\nSecond: https://b.test");
        assert_eq!(urls, vec!["https://a.test", "https://b.test"]);
    }

    #[test]
    fn deduplicates() {
        let urls = extract_urls_from_text("https://x.test then https://x.test again");
        assert_eq!(urls, vec!["https://x.test"]);
    }

    #[test]
    fn trims_trailing_punctuation() {
        let urls = extract_urls_from_text("Visit https://example.com.");
        assert_eq!(urls, vec!["https://example.com"]);
    }

    #[test]
    fn handles_empty_text() {
        let urls = extract_urls_from_text("");
        assert!(urls.is_empty());
    }

    #[test]
    fn handles_text_with_no_urls() {
        let urls = extract_urls_from_text("plain text, no links");
        assert!(urls.is_empty());
    }

    #[test]
    fn collect_event_urls_dedupes_against_conference() {
        let urls = collect_event_urls(
            Some("Call link: https://meet.google.com/abc and slides https://docs.test"),
            None,
            Some("https://meet.google.com/abc"),
        );
        assert_eq!(urls, vec!["https://docs.test"]);
    }

    #[test]
    fn collect_event_urls_includes_location_links() {
        let urls = collect_event_urls(
            Some("Agenda https://a.test"),
            Some("Physical: https://maps.test/room42"),
            None,
        );
        assert_eq!(urls, vec!["https://a.test", "https://maps.test/room42"]);
    }

    #[test]
    fn collect_event_urls_handles_all_none() {
        let urls = collect_event_urls(None, None, None);
        assert!(urls.is_empty());
    }
}
