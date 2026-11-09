//! Filename template rendering. OWNED BY: Codex.
//! See docs/CODEX_BRIEF.md. Implement the body; keep the signature.

use crate::model::NameContext;

/// Expand tokens in `template` and append `.<ext>` from `ctx.ext`.
/// Tokens: %Y %m %d %H %M %S %n %name %w %h %title (see brief).
pub fn render_filename(template: &str, ctx: &NameContext) -> String {
    render_at(template, ctx, &chrono::Local::now())
}

fn render_at<T: chrono::TimeZone>(
    template: &str,
    ctx: &NameContext,
    now: &chrono::DateTime<T>,
) -> String
where
    T::Offset: std::fmt::Display,
{
    let mut name = String::new();
    let mut rest = template;
    while !rest.is_empty() {
        if let Some(tail) = rest.strip_prefix("%name") {
            name.push_str(&sanitize(&ctx.preset_name));
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix("%title") {
            name.push_str(&sanitize(ctx.window_title.as_deref().unwrap_or("")));
            rest = tail;
        } else if let Some(tail) = rest.strip_prefix('%') {
            match tail.chars().next() {
                Some(token) => {
                    match token {
                        'Y' | 'm' | 'd' | 'H' | 'M' | 'S' => {
                            name.push_str(&now.format(&format!("%{token}")).to_string());
                        }
                        'n' => name.push_str(&format!("{:03}", ctx.counter)),
                        'w' => name.push_str(&ctx.width.to_string()),
                        'h' => name.push_str(&ctx.height.to_string()),
                        _ => {
                            name.push('%');
                            name.push(token);
                        }
                    }
                    rest = &tail[token.len_utf8()..];
                }
                None => {
                    name.push('%');
                    rest = tail;
                }
            }
        } else {
            let ch = rest.chars().next().unwrap();
            name.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }
    name.push('.');
    name.push_str(&ctx.ext);
    let mut name = sanitize(&name).trim_end_matches([' ', '.']).to_owned();
    // Windows device names are reserved even when followed by an extension.
    let stem = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end_matches(' ')
        .to_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(|suffix| {
                matches!(
                    suffix,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        });
    if name.is_empty() || reserved {
        name.insert(0, '_');
    }
    name
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .filter(|ch| {
            !ch.is_control() && !matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn context() -> NameContext {
        NameContext {
            preset_name: "Region".into(),
            counter: 7,
            window_title: Some("Editor".into()),
            width: 640,
            height: 480,
            ext: "png".into(),
        }
    }

    #[test]
    fn expands_tokens() {
        let now = chrono::Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        assert_eq!(
            render_at("%name_%Y-%m-%d_%H-%M-%S_%n_%w-%h_%title", &context(), &now),
            "Region_2026-01-02_03-04-05_007_640-480_Editor.png"
        );
    }

    #[test]
    fn strips_illegal_characters() {
        let mut ctx = context();
        ctx.preset_name = "A/\\:*?\"<>|\n\0B".into();
        ctx.window_title = Some("Ảnh: <test>".into());
        assert_eq!(render_filename("%name/%title?", &ctx), "ABẢnh test.png");
        assert_eq!(render_filename("CON", &ctx), "_CON.png");
    }

    #[test]
    fn unknown_tokens_pass_through() {
        assert_eq!(
            render_filename("%x_%ext_%é_%", &context()),
            "%x_%ext_%é_%.png"
        );
    }

    #[test]
    fn replacement_values_are_not_expanded() {
        let mut ctx = context();
        ctx.preset_name = "%n".into();
        ctx.counter = 1234;
        ctx.window_title = None;
        assert_eq!(render_filename("%name_%n_%title", &ctx), "%n_1234_.png");
    }
}
