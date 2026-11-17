//! Fitting text into columns by display width (wide characters count as two).

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Keep the end: `…/proj/target`. Paths are most recognizable by their tail.
pub fn truncate_left(text: &str, width: usize, ellipsis: &str) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    let budget = width.saturating_sub(ellipsis.width());
    let mut used = 0;
    let mut start = text.len();
    for (i, c) in text.char_indices().rev() {
        let w = c.width().unwrap_or(0);
        if used + w > budget {
            break;
        }
        used += w;
        start = i;
    }
    format!("{ellipsis}{}", &text[start..])
}

/// Keep the start: `devbox-codespa…`.
pub fn truncate_right(text: &str, width: usize, ellipsis: &str) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    let budget = width.saturating_sub(ellipsis.width());
    let mut used = 0;
    let mut end = 0;
    for (i, c) in text.char_indices() {
        let w = c.width().unwrap_or(0);
        if used + w > budget {
            break;
        }
        used += w;
        end = i + c.len_utf8();
    }
    format!("{}{ellipsis}", &text[..end])
}
