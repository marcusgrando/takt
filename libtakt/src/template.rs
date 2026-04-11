//! Event-variable template substitution.
//!
//! Replaces `{{event.field}}` tokens in free-form strings with values from a
//! `CalendarEvent`. Permissive by design: when no event context is present,
//! `{{event.*}}` tokens are replaced with empty strings. Unknown `{{event.xxx}}`
//! tokens are left as-is so typos stay visible.

use crate::models::CalendarEvent;

/// Substitute `{{event.field}}` tokens in `text` using `event`.
/// Returns an owned String with all substitutions applied.
///
/// Supported fields:
/// - `{{event.title}}`
/// - `{{event.start}}`
/// - `{{event.end}}`
/// - `{{event.notes}}`
/// - `{{event.location}}`
/// - `{{event.url}}`
/// - `{{event.conference_url}}`
/// - `{{event.calendar_id}}`
pub fn substitute_event_vars(text: &str, event: Option<&CalendarEvent>) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{event.") {
        result.push_str(&rest[..start]);
        let after_open = &rest[start + 2..];
        if let Some(end) = after_open.find("}}") {
            let token = &after_open[..end];
            let replacement = resolve(token, event);
            match replacement {
                Some(value) => result.push_str(&value),
                None => {
                    // Unknown token: leave the original `{{...}}` intact.
                    result.push_str("{{");
                    result.push_str(token);
                    result.push_str("}}");
                }
            }
            rest = &after_open[end + 2..];
        } else {
            // Unterminated `{{event.`: append the rest literally and stop.
            result.push_str(&rest[start..]);
            return result;
        }
    }
    result.push_str(rest);
    result
}

fn resolve(token: &str, event: Option<&CalendarEvent>) -> Option<String> {
    let field = token.strip_prefix("event.")?;
    let e = match event {
        Some(e) => e,
        None => {
            // Permissive mode: known fields resolve to empty strings.
            return match field {
                "title" | "start" | "end" | "notes" | "location"
                | "url" | "conference_url" | "calendar_id" => Some(String::new()),
                _ => None,
            };
        }
    };
    match field {
        "title" => Some(e.title.clone()),
        "start" => Some(e.start.clone()),
        "end" => Some(e.end.clone()),
        "notes" => Some(e.notes.clone().unwrap_or_default()),
        "location" => Some(e.location.clone().unwrap_or_default()),
        "url" => Some(e.url.clone().unwrap_or_default()),
        "conference_url" => Some(e.conference_url.clone().unwrap_or_default()),
        "calendar_id" => Some(e.calendar_id.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CalendarEvent;

    fn sample_event() -> CalendarEvent {
        CalendarEvent {
            id: "evt-1".into(),
            title: "Team standup".into(),
            start: "2026-04-12T09:00:00Z".into(),
            end: "2026-04-12T09:30:00Z".into(),
            notes: Some("Agenda".into()),
            location: None,
            url: None,
            conference_url: Some("https://meet.test/abc".into()),
            calendar_id: "cal-1".into(),
        }
    }

    #[test]
    fn substitutes_title() {
        let r = substitute_event_vars("Meeting: {{event.title}}", Some(&sample_event()));
        assert_eq!(r, "Meeting: Team standup");
    }

    #[test]
    fn substitutes_multiple_vars() {
        let r = substitute_event_vars(
            "{{event.title}} at {{event.start}}",
            Some(&sample_event()),
        );
        assert_eq!(r, "Team standup at 2026-04-12T09:00:00Z");
    }

    #[test]
    fn substitutes_conference_url() {
        let r = substitute_event_vars("Join: {{event.conference_url}}", Some(&sample_event()));
        assert_eq!(r, "Join: https://meet.test/abc");
    }

    #[test]
    fn empty_for_none_optional_field() {
        let mut e = sample_event();
        e.location = None;
        let r = substitute_event_vars("[{{event.location}}]", Some(&e));
        assert_eq!(r, "[]");
    }

    #[test]
    fn unknown_field_left_intact() {
        let r = substitute_event_vars("{{event.bogus}}", Some(&sample_event()));
        assert_eq!(r, "{{event.bogus}}");
    }

    #[test]
    fn no_event_context_substitutes_empty() {
        let r = substitute_event_vars("Hello {{event.title}}!", None);
        assert_eq!(r, "Hello !");
    }

    #[test]
    fn no_event_context_unknown_field_left_intact() {
        let r = substitute_event_vars("{{event.bogus}}", None);
        assert_eq!(r, "{{event.bogus}}");
    }

    #[test]
    fn text_without_placeholders_unchanged() {
        let r = substitute_event_vars("plain text", Some(&sample_event()));
        assert_eq!(r, "plain text");
    }

    #[test]
    fn unterminated_placeholder_preserved() {
        let r = substitute_event_vars("start {{event.title", Some(&sample_event()));
        assert_eq!(r, "start {{event.title");
    }
}
