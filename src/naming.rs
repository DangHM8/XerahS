//! Filename template rendering. OWNED BY: Codex.
//! See docs/CODEX_BRIEF.md. Implement the body; keep the signature.

use crate::model::NameContext;

/// Expand tokens in `template` and append `.<ext>` from `ctx.ext`.
/// Tokens: %Y %m %d %H %M %S %n %name %w %h %title (see brief).
pub fn render_filename(template: &str, ctx: &NameContext) -> String {
    let _ = (template, ctx);
    todo!("expand tokens with chrono::Local::now(), sanitize, append ext")
}
