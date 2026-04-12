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

/// URL patterns to exclude: support/help pages, phone/dial links, and
/// other non-actionable meeting-adjacent URLs that calendar providers embed.
static NOISE_URL_PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();

fn noise_url_patterns() -> &'static [Regex] {
    NOISE_URL_PATTERNS.get_or_init(|| {
        [
            // Google Meet telephony page (tel.meet)
            r#"^https?://tel\.meet/"#,
            // Google support / help pages
            r#"^https?://support\.google\.com/"#,
            // Generic support/help subdomains or paths
            r#"^https?://[^/]*support\.[^/]+/"#,
            r#"^https?://[^/]*help\.[^/]+/"#,
            r#"(?i)/support(/|$|\?)"#,
            r#"(?i)/help(/|$|\?)"#,
            r#"(?i)/faq(/|$|\?)"#,
            // Zoom phone/dial pages
            r#"^https?://[^/]*zoom\.us/[^\s]*\btel\b"#,
            // Teams phone pages
            r#"^https?://[^/]*teams\.microsoft\.com/[^\s]*\bdial\b"#,
            // Webex phone pages
            r#"^https?://[^/]*webex\.com/[^\s]*\btel\b"#,
            // Generic phone/dial URIs embedded as https links
            r#"(?i)[?&](tel|phone|dial)="#,
        ]
        .iter()
        .map(|p| Regex::new(p).expect("valid noise pattern"))
        .collect()
    })
}

/// Returns true if the URL looks like a support, help, or telephony link
/// that should NOT be opened automatically.
fn is_noise_url(url: &str) -> bool {
    noise_url_patterns().iter().any(|re| re.is_match(url))
}

/// Extract unique URLs from free-form text, in order of first appearance.
/// Filters out noise URLs (support pages, phone/dial links, etc.).
pub fn extract_urls_from_text(text: &str) -> Vec<String> {
    let re = url_regex();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for m in re.find_iter(text) {
        let url = trim_trailing_punctuation(m.as_str());
        if is_noise_url(url) {
            continue;
        }
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
    let trimmed = s.trim_end_matches(['.', ',', ';', ':', '!', '?']);
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

    // --- noise URL filtering tests ---

    #[test]
    fn filters_google_meet_tel_page() {
        let urls = extract_urls_from_text(
            "More phone numbers: https://tel.meet/ccd-bkcu-gct?pin=3768534798617&hs=7",
        );
        assert!(urls.is_empty());
    }

    #[test]
    fn filters_google_support_page() {
        let urls = extract_urls_from_text(
            "Learn more about Meet at: https://support.google.com/a/users/answer/9282720",
        );
        assert!(urls.is_empty());
    }

    #[test]
    fn filters_generic_help_subdomain() {
        let urls = extract_urls_from_text("See https://help.zoom.us/article/123");
        assert!(urls.is_empty());
    }

    #[test]
    fn filters_path_based_support() {
        let urls = extract_urls_from_text("Visit https://example.com/support/article");
        assert!(urls.is_empty());
    }

    #[test]
    fn filters_path_based_help() {
        let urls = extract_urls_from_text("Go to https://example.com/help/topic");
        assert!(urls.is_empty());
    }

    #[test]
    fn filters_faq_path() {
        let urls = extract_urls_from_text("See https://example.com/faq/meetings");
        assert!(urls.is_empty());
    }

    #[test]
    fn keeps_real_meeting_links() {
        let text = "Join: https://meet.google.com/abc-defg-hij\n\
                     Zoom: https://zoom.us/j/123456\n\
                     Teams: https://teams.microsoft.com/l/meetup-join/abc";
        let urls = extract_urls_from_text(text);
        assert_eq!(urls.len(), 3);
    }

    #[test]
    fn keeps_doc_links_filters_noise() {
        let text = "Agenda: https://docs.google.com/doc/d/abc\n\
                     Help: https://support.google.com/a/users/answer/9282720\n\
                     Dial in: https://tel.meet/xyz?pin=123";
        let urls = extract_urls_from_text(text);
        assert_eq!(urls, vec!["https://docs.google.com/doc/d/abc"]);
    }

    #[test]
    fn collect_event_urls_filters_noise_from_notes() {
        let urls = collect_event_urls(
            Some(
                "Join: https://meet.google.com/abc\n\
                 Slides: https://slides.google.com/xyz\n\
                 Support: https://support.google.com/help/123\n\
                 Phone: https://tel.meet/abc?pin=999",
            ),
            None,
            Some("https://meet.google.com/abc"),
        );
        // conference_url deduped, noise filtered, only slides remain
        assert_eq!(urls, vec!["https://slides.google.com/xyz"]);
    }

    #[test]
    fn noise_detection_is_case_insensitive_for_paths() {
        let urls = extract_urls_from_text("Visit https://example.com/Help/Article");
        assert!(urls.is_empty());
        let urls = extract_urls_from_text("Visit https://example.com/FAQ");
        assert!(urls.is_empty());
    }
}
