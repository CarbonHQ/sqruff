//! Source line-break normalisation for `source_line_breaks = ignore`.
//!
//! The fixed layout must depend only on tokens, comments and blank lines, so
//! every other line break is removed before the layout rules run. Two kinds of
//! line break cannot be removed: blank lines, which are deliberate grouping,
//! and the break an inline comment needs after it. A bracket that contains
//! such a break has to span several lines anyway, so it is expanded in full:
//! one line break after its opening bracket, after each of its commas and
//! before its closing bracket. That keeps the result independent of where the
//! author happened to wrap the rest of the bracket.

use sqruff_lib_core::dialects::syntax::SyntaxKind;
use sqruff_lib_core::parser::segments::ErasedSegment;

/// The separation between a token and the one before it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Gap {
    None,
    Space,
    Line,
    Blank,
}

struct Token<'a> {
    raw: &'a str,
    gap: Gap,
    /// Index of the innermost bracket pair containing this token.
    bracket: Option<usize>,
    /// For an opening or closing bracket, the pair it belongs to.
    own_bracket: Option<usize>,
    is_inline_comment: bool,
}

fn is_opening(raw: &str) -> bool {
    matches!(raw, "(" | "[")
}

fn is_closing(raw: &str) -> bool {
    matches!(raw, ")" | "]")
}

/// Join every line break that is neither a blank line nor required by a
/// comment, expanding brackets that still have to span several lines.
/// Token text, including multi-line literals and comments, is copied unchanged.
pub(crate) fn join_source_lines(segments: &[ErasedSegment]) -> String {
    let mut tokens: Vec<Token<'_>> = Vec::new();
    let mut leading = String::new();
    let mut trailing = String::new();
    // Parent of each bracket pair, and whether it must span several lines.
    let mut parents: Vec<Option<usize>> = Vec::new();
    let mut expanded: Vec<bool> = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    let mut whitespace = String::new();
    let mut after_inline_comment = false;

    for segment in segments {
        let raw = segment.raw();
        if raw.is_empty() {
            continue;
        }

        if segment.is_whitespace() {
            whitespace.push_str(raw);
            continue;
        }

        let line_breaks = whitespace.matches('\n').count();
        let is_comment = segment.is_comment();
        let gap = if tokens.is_empty() {
            // Leading whitespace carries the indentation of embedded SQL.
            leading = std::mem::take(&mut whitespace);
            Gap::None
        } else if line_breaks > 1 {
            Gap::Blank
        } else if after_inline_comment || (is_comment && line_breaks > 0) {
            Gap::Line
        } else if whitespace.is_empty() {
            Gap::None
        } else {
            Gap::Space
        };

        let code = !is_comment;
        let closes = code && is_closing(raw);
        let own_bracket = if closes {
            open.pop()
        } else if code && is_opening(raw) {
            parents.push(open.last().copied());
            expanded.push(false);
            Some(parents.len() - 1)
        } else {
            None
        };
        let bracket = if closes {
            own_bracket
        } else {
            open.last().copied()
        };

        // A break that has to stay makes every bracket around it multi-line.
        // The break before a closing bracket belongs to that bracket too.
        if matches!(gap, Gap::Line | Gap::Blank) {
            let mut current = bracket;
            while let Some(index) = current {
                if expanded[index] {
                    break;
                }
                expanded[index] = true;
                current = parents[index];
            }
        }

        if code && is_opening(raw) {
            open.push(own_bracket.unwrap());
        }

        tokens.push(Token {
            raw,
            gap,
            bracket,
            own_bracket,
            is_inline_comment: segment.get_type() == SyntaxKind::InlineComment,
        });
        whitespace.clear();
        after_inline_comment = segment.get_type() == SyntaxKind::InlineComment;
    }

    trailing.push_str(&whitespace);

    let is_expanded = |bracket: Option<usize>| bracket.is_some_and(|index| expanded[index]);
    let mut output = String::with_capacity(leading.len() + trailing.len() + tokens.len() * 8);
    output.push_str(&leading);

    for (index, token) in tokens.iter().enumerate() {
        let previous = index.checked_sub(1).map(|index| &tokens[index]);
        let stays_on_line = token.is_inline_comment && token.gap != Gap::Line;
        let expands = previous.is_some_and(|previous| {
            let after_opening = is_opening(previous.raw) && is_expanded(previous.own_bracket);
            let after_comma = previous.raw == "," && is_expanded(previous.bracket);
            let before_closing = is_closing(token.raw) && is_expanded(token.own_bracket);

            after_opening || after_comma || before_closing
        });

        match token.gap {
            Gap::Blank => output.push_str("\n\n"),
            Gap::Line => output.push('\n'),
            _ if expands && !stays_on_line => output.push('\n'),
            Gap::Space => output.push(' '),
            Gap::None => {}
        }

        output.push_str(token.raw);
    }

    output.push_str(&trailing);
    output
}
