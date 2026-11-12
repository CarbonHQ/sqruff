use hashbrown::{HashMap, HashSet};
use sqruff_lib_core::dialects::syntax::SyntaxKind;
use sqruff_lib_core::lint_fix::LintFix;
use sqruff_lib_core::parser::segments::{ErasedSegment, SegmentBuilder};

use crate::core::config::Value;
use crate::core::rules::context::RuleContext;
use crate::core::rules::crawlers::{Crawler, RootOnlyCrawler};
use crate::core::rules::{Erased, ErasedRule, LintResult, Rule, RuleGroups};
use crate::utils::reflow::rebreak::LinePosition;

/// Places the line breaks that follow from structure alone.
///
/// With `source_line_breaks = ignore` the layout is rebuilt from the tokens.
/// Some breaks are always there: clause keywords without a syntax type of
/// their own, `AND` and `OR`, children configured with a strict line position.
/// Others follow from a group spanning several lines, which other rules decide
/// (a long line, a comment, one select target per line): such a group is then
/// broken at every one of its boundaries, so the result never depends on which
/// boundary happened to be broken first.
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
        // The brackets around a list are not part of its first and last item.
        let has_wide_item = children
            .split(|child| {
                child.is_type(SyntaxKind::Comma)
                    || child.is_type(SyntaxKind::StartBracket)
                    || child.is_type(SyntaxKind::EndBracket)
            })
            .any(|item| self.spans_lines(item));
        if has_wide_item {
            self.break_list(node);
        }
    }

    /// Break after the opening bracket and every comma, and before the closing bracket.
    fn break_list(&mut self, node: &ErasedSegment) {
        for child in node.segments() {
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

    /// Positions of the first and last raw segment of one child that hold text.
    fn child_span(&self, child: &ErasedSegment) -> Option<(usize, usize)> {
        if child.segments().is_empty() {
            self.position(child).map(|position| (position, position))
        } else {
            self.text_span(std::slice::from_ref(child))
        }
    }

    /// Children whose type is configured with a strict line position always
    /// take it. LT03 and LT04 own operators and commas, which are raw tokens.
    fn place_strict_children(&mut self, node: &ErasedSegment) {
        for child in node.segments() {
            if child.segments().is_empty() {
                continue;
            }
            let Some(position) = self
                .context
                .config
                .reflow()
                .line_position_for(child.get_type())
                .filter(|position| position.is_strict())
            else {
                continue;
            };
            let Some((first, last)) = self.child_span(child) else {
                continue;
            };

            if matches!(
                position.position(),
                LinePosition::Leading | LinePosition::Alone
            ) {
                self.break_before(first);
            }
            if matches!(
                position.position(),
                LinePosition::Trailing | LinePosition::Alone
            ) {
                self.break_after(last);
            }
        }
    }

    /// Clause keywords that have no syntax type of their own to configure.
    /// They start their line, and the ones that introduce a list also end it,
    /// like the configurable clause keywords do with `alone`.
    fn place_clause_keywords(&mut self, node: &ErasedSegment) {
        let children = node.segments();
        let keywords: Vec<(usize, &ErasedSegment)> = children
            .iter()
            .enumerate()
            .filter(|(_, child)| child.is_type(SyntaxKind::Keyword))
            .collect();

        for (order, &(index, keyword)) in keywords.iter().enumerate() {
            let Some(position) = self.position(keyword) else {
                continue;
            };
            let next_keyword = keywords.get(order + 1).map(|&(_, next)| next);
            let is_first_keyword = order == 0;

            let starts_statement = is_first_keyword
                && (node.is_type(SyntaxKind::InsertStatement)
                    || node.is_type(SyntaxKind::UpdateStatement)
                    || node.is_type(SyntaxKind::DeleteStatement)
                    || node.is_type(SyntaxKind::MergeStatement));

            let (before, after) = if keyword.is_keyword("RETURNING") {
                (true, true)
            } else if starts_statement {
                // After a `WITH` clause or an opening bracket, like `SELECT`.
                (true, false)
            } else if node.is_type(SyntaxKind::InsertStatement) {
                let starts_conflict = keyword.is_keyword("ON")
                    && next_keyword.is_some_and(|next| next.is_keyword("CONFLICT"));
                let ends_target_keywords = keyword.is_keyword("INTO")
                    && children[..index]
                        .iter()
                        .any(|child| child.is_keyword("INSERT"));

                (starts_conflict, ends_target_keywords)
            } else if node.is_type(SyntaxKind::ConflictAction) {
                let sets = keyword.is_keyword("SET");
                (sets, sets)
            } else if node.is_type(SyntaxKind::SetStatement) {
                (false, is_first_keyword && keyword.is_keyword("SET"))
            } else if node.is_type(SyntaxKind::CreateIndexStatement) {
                let filters = keyword.is_keyword("WHERE");
                (filters, filters)
            } else if node.is_type(SyntaxKind::CreateStatisticsStatement) {
                let sources = keyword.is_keyword("FROM");
                (sources, sources)
            } else if node.is_type(SyntaxKind::WithCompoundStatement) {
                // `WITH` or `WITH RECURSIVE`, then one CTE per line.
                let ends_with_keywords = (is_first_keyword
                    && keyword.is_keyword("WITH")
                    && !next_keyword.is_some_and(|next| next.is_keyword("RECURSIVE")))
                    || keyword.is_keyword("RECURSIVE");

                (false, ends_with_keywords)
            } else {
                (false, false)
            };

            if before {
                self.break_before(position);
            }
            if after {
                self.break_after(position);
            }
        }
    }

    /// `AND` and `OR` always start a line, so a condition reads one term per line.
    fn place_logical_operators(&mut self, node: &ErasedSegment) {
        for child in node.segments() {
            let is_logical = child.is_type(SyntaxKind::BinaryOperator)
                && (child.raw().eq_ignore_ascii_case("AND")
                    || child.raw().eq_ignore_ascii_case("OR"));
            if !is_logical {
                continue;
            }
            if let Some(position) = self.position(child) {
                self.break_before(position);
            }
        }
    }

    /// `ALTER TABLE` actions start their own line. Switches such as
    /// `ENABLE ROW LEVEL SECURITY` are short and stay with the table.
    fn place_alter_table_actions(&mut self, node: &ErasedSegment) {
        for child in node.segments() {
            if !child.is_type(SyntaxKind::AlterTableActionSegment) {
                continue;
            }
            let is_switch = child
                .segments()
                .iter()
                .find(|segment| segment.is_type(SyntaxKind::Keyword))
                .is_some_and(|keyword| {
                    ["ENABLE", "DISABLE", "FORCE", "NO"]
                        .iter()
                        .any(|switch| keyword.is_keyword(switch))
                });
            if is_switch {
                continue;
            }
            if let Some((first, _)) = self.child_span(child) {
                self.break_before(first);
            }
        }
    }

    /// A column list that does not fit on the `CREATE TABLE` line takes one
    /// line per column or constraint.
    fn expand_table_columns(&mut self, node: &ErasedSegment) {
        for child in node.segments() {
            if child.is_type(SyntaxKind::Bracketed) && self.spans_lines(child.segments()) {
                self.break_list(child);
            }
        }
    }

    /// A join whose condition spans lines starts that condition on its own
    /// line, so its `AND` and `OR` terms line up under `ON`.
    fn place_join_condition(&mut self, node: &ErasedSegment) {
        for child in node.segments() {
            if !child.is_type(SyntaxKind::JoinOnCondition) || !self.spans_lines(child.segments()) {
                continue;
            }
            if let Some((first, _)) = self.child_span(child) {
                self.break_before(first);
            }
        }
    }

    fn visit(&mut self, node: &ErasedSegment) {
        if node.is_type(SyntaxKind::JoinClause) {
            self.place_join_condition(node);
        }
        if node.is_type(SyntaxKind::AlterTableStatement) {
            self.place_alter_table_actions(node);
        }
        if node.is_type(SyntaxKind::CreateTableStatement) {
            self.expand_table_columns(node);
        }

        self.place_strict_children(node);
        self.place_clause_keywords(node);
        self.place_logical_operators(node);

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
        "Structural line breaks should be in place."
    }

    fn long_description(&self) -> &'static str {
        r#"
Only active with `source_line_breaks = ignore`.

**Always on their own line**

- Children whose syntax type has a strict `line_position`, for example
  `when_clause` and `else_clause` with `leading:strict`.
- `AND` and `OR`.
- `INSERT INTO`, `ON CONFLICT`, `RETURNING`, `SET` in
  `ON CONFLICT ... DO UPDATE` and in a `SET` statement, `WITH`, the `WHERE` of
  `CREATE INDEX`, and `ALTER TABLE` actions other than switches such as
  `ENABLE ROW LEVEL SECURITY`.

**Once a group spans several lines**

- Every item of a comma-separated list that has a multi-line item.
- Every column of a `CREATE TABLE`.
- Every branch of a `CASE`, and its `END`.
- The `ON` of a join.

**Anti-pattern**

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
FROM a WHERE x = 1 AND y = 2
```

**Best practice**

```sql
WITH
a AS (
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
WHERE
    x = 1
    AND y = 2
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
