use hashbrown::{HashMap, HashSet};
use sqruff_lib_core::dialects::syntax::SyntaxKind;
use sqruff_lib_core::lint_fix::LintFix;
use sqruff_lib_core::parser::segments::{ErasedSegment, SegmentBuilder};

use crate::core::config::Value;
use crate::core::rules::context::RuleContext;
use crate::core::rules::crawlers::{Crawler, RootOnlyCrawler};
use crate::core::rules::{Erased, ErasedRule, LintResult, Rule, RuleGroups};

/// Expands groups that already span several lines.
///
/// With `source_line_breaks = ignore` the layout is rebuilt from the tokens,
/// so a group is either short enough to stay on one line or it is broken at
/// every one of its boundaries. Other rules decide that a group must span
/// lines (a long line, a comment, one select target per line); this rule then
/// finishes the job, so the result never depends on which boundary happened
/// to be broken first.
#[derive(Debug, Default, Clone)]
pub struct RuleLT16;

struct Expansion<'a> {
    context: &'a RuleContext<'a>,
    raws: Vec<ErasedSegment>,
    positions: HashMap<usize, usize>,
    anchored: HashSet<usize>,
    results: Vec<LintResult>,
}

impl Expansion<'_> {
    fn position(&self, segment: &ErasedSegment) -> Option<usize> {
        self.positions.get(&segment.addr()).copied()
    }

    /// Positions of the first and last raw segment of `segments` that hold text.
    fn text_span(&self, segments: &[ErasedSegment]) -> Option<(usize, usize)> {
        let mut span: Option<(usize, usize)> = None;

        for segment in segments {
            for raw in segment.get_raw_segments() {
                if raw.is_whitespace() || raw.is_meta() || raw.raw().is_empty() {
                    continue;
                }
                let Some(position) = self.position(&raw) else {
                    continue;
                };
                span = Some(match span {
                    Some((first, _)) => (first, position),
                    None => (position, position),
                });
            }
        }

        span
    }

    fn spans_lines(&self, segments: &[ErasedSegment]) -> bool {
        self.text_span(segments).is_some_and(|(first, last)| {
            self.raws[first..last]
                .iter()
                .any(|raw| raw.is_type(SyntaxKind::Newline))
        })
    }

    /// Require a line break between the raw segment at `position` and the text before it.
    fn break_before(&mut self, position: usize) {
        for raw in self.raws[..position].iter().rev() {
            if raw.is_type(SyntaxKind::Newline) {
                return;
            }
            if !(raw.is_whitespace() || raw.is_meta() || raw.raw().is_empty()) {
                break;
            }
        }

        let anchor = self.raws[position].clone();
        if position == 0 || !self.anchored.insert(anchor.addr()) {
            return;
        }

        let newline = SegmentBuilder::newline(self.context.tables.next_id(), "\n");
        self.results.push(LintResult::new(
            anchor.clone().into(),
            vec![LintFix::create_before(anchor, vec![newline])],
            None,
            None,
        ));
    }

    /// Require a line break after the raw segment at `position`. A trailing
    /// inline comment stays on the line and already ends it.
    fn break_after(&mut self, position: usize) {
        for (offset, raw) in self.raws[position + 1..].iter().enumerate() {
            if raw.is_type(SyntaxKind::Newline) || raw.is_type(SyntaxKind::InlineComment) {
                return;
            }
            if raw.is_whitespace() || raw.is_meta() || raw.raw().is_empty() {
                continue;
            }

            self.break_before(position + 1 + offset);
            return;
        }
    }

    fn expand_case(&mut self, node: &ErasedSegment) {
        if !self.spans_lines(node.segments()) {
            return;
        }

        for child in node.segments() {
            let is_branch = child.is_type(SyntaxKind::WhenClause)
                || child.is_type(SyntaxKind::ElseClause)
                || child.is_keyword("END");
            if !is_branch {
                continue;
            }
            if let Some((first, _)) = self.text_span(std::slice::from_ref(child)) {
                self.break_before(first);
            }
        }
    }

    fn expand_delimited(&mut self, node: &ErasedSegment) {
        let children = node.segments();
        let has_wide_item = children
            .split(|child| child.is_type(SyntaxKind::Comma))
            .any(|item| self.spans_lines(item));
        if !has_wide_item {
            return;
        }

        for child in children {
            let position = self.position(child);

            if child.is_type(SyntaxKind::Comma) || child.is_type(SyntaxKind::StartBracket) {
                if let Some(position) = position {
                    self.break_after(position);
                }
            } else if child.is_type(SyntaxKind::EndBracket)
                && let Some(position) = position
            {
                self.break_before(position);
            }
        }
    }

    fn visit(&mut self, node: &ErasedSegment) {
        if node.is_type(SyntaxKind::CaseExpression) {
            self.expand_case(node);
        }

        // One select target per line is LT09's decision.
        let is_delimited = !node.is_type(SyntaxKind::SelectClause)
            && node
                .segments()
                .iter()
                .any(|child| child.is_type(SyntaxKind::Comma));
        if is_delimited {
            self.expand_delimited(node);
        }

        for child in node.segments() {
            if !child.segments().is_empty() {
                self.visit(child);
            }
        }
    }
}

impl Rule for RuleLT16 {
    fn load_from_config(&self, _config: &HashMap<String, Value>) -> Result<ErasedRule, String> {
        Ok(RuleLT16.erased())
    }

    fn name(&self) -> &'static str {
        "layout.multiline_groups"
    }

    fn description(&self) -> &'static str {
        "Groups spanning several lines should break at every boundary."
    }

    fn long_description(&self) -> &'static str {
        r#"
Only active with `source_line_breaks = ignore`.

**Anti-pattern**

A list whose items span several lines keeps some of them on a shared line,
and a multi-line `CASE` keeps its first branch on the `CASE` line.

```sql
WITH a AS (
    SELECT 1
), b AS (
    SELECT 2
)

SELECT
    CASE WHEN x = 1 THEN 'one' -- first
        ELSE 'other'
    END
FROM a
```

**Best practice**

Start every item of such a list, and every branch of such a `CASE`, on its own line.

```sql
WITH a AS (
    SELECT 1
),
b AS (
    SELECT 2
)

SELECT
    CASE
        WHEN x = 1 THEN 'one' -- first
        ELSE 'other'
    END
FROM a
```
"#
    }

    fn groups(&self) -> &'static [RuleGroups] {
        &[RuleGroups::All, RuleGroups::Layout]
    }

    fn eval(&self, context: &RuleContext) -> Vec<LintResult> {
        let ignores_source_line_breaks = context
            .config
            .get("source_line_breaks", "core")
            .as_string()
            .is_some_and(|value| value == "ignore");
        if !ignores_source_line_breaks {
            return Vec::new();
        }

        let raws = context.segment.get_raw_segments();
        let positions = raws
            .iter()
            .enumerate()
            .map(|(position, raw)| (raw.addr(), position))
            .collect();
        let mut expansion = Expansion {
            context,
            raws,
            positions,
            anchored: HashSet::new(),
            results: Vec::new(),
        };

        expansion.visit(&context.segment);
        expansion.results
    }

    fn is_fix_compatible(&self) -> bool {
        true
    }

    fn crawl_behaviour(&self) -> Crawler {
        RootOnlyCrawler.into()
    }
}
