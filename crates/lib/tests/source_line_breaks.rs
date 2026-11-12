//! `source_line_breaks = ignore`: the fixed layout depends only on tokens,
//! comments and blank lines, and is reached in a single pass.

use sqruff_lib::core::config::FluffConfig;
use sqruff_lib::core::linter::core::Linter;
use sqruff_lib_core::dialects::syntax::SyntaxKind;
use sqruff_lib_core::parser::segments::Tables;

const CONFIG: &str = "
[sqruff]
dialect = postgres
rules = layout
exclude_rules = LT08
max_line_length = 80
source_line_breaks = ignore

[sqruff:indentation]
tab_space_size = 2
implicit_indents = allow
indented_joins = True
indented_ctes = True
trailing_comments = keep

[sqruff:rules:layout.newlines]
maximum_empty_lines_between_statements = 1
minimum_empty_lines_between_statements = 1

[sqruff:rules:layout.select_targets]
single_target_policy = new_line

[sqruff:layout:type:comma]
line_position = trailing:attached

[sqruff:layout:type:when_clause]
line_position = leading

[sqruff:layout:type:else_clause]
line_position = leading

[sqruff:layout:type:select_clause]
keyword_line_position = leading

[sqruff:layout:type:from_clause]
keyword_line_position = alone

[sqruff:layout:type:join_clause]
keyword_line_position = leading

[sqruff:layout:type:where_clause]
keyword_line_position = alone
";

fn linter() -> Linter {
    Linter::new(FluffConfig::from_source(CONFIG, None), None, None, true).unwrap()
}

fn fix(linter: &Linter, sql: &str) -> String {
    linter.lint_string(sql, None, true).unwrap().fix_string()
}

/// Put every token on its own line, keeping blank lines and comments in place.
fn one_token_per_line(linter: &Linter, sql: &str) -> String {
    let tables = Tables::default();
    let (tokens, errors) = linter.config().get_dialect().lexer().lex(&tables, sql);
    assert!(errors.is_empty());

    let mut output = String::new();
    let mut line_breaks = 0;
    let mut had_space = false;

    for token in &tokens {
        let raw = token.raw();
        if raw.is_empty() {
            continue;
        }

        if token.is_whitespace() {
            line_breaks += raw.matches('\n').count();
            had_space = true;
            continue;
        }

        let trails = token.get_type() == SyntaxKind::InlineComment && line_breaks == 0;

        if !output.is_empty() {
            if line_breaks > 1 {
                output.push_str("\n\n");
            } else if trails {
                output.push(' ');
            } else if had_space {
                output.push('\n');
            }
        }

        output.push_str(raw);
        line_breaks = 0;
        had_space = false;
    }

    output.push('\n');
    output
}

/// The expected text is the fixed point for itself, for the single-line
/// source and for the source with every token on its own line.
fn assert_canonical(expected: &str) {
    let linter = linter();
    let exploded = one_token_per_line(&linter, expected);

    assert_eq!(fix(&linter, expected), expected, "not stable");
    assert_eq!(fix(&linter, &exploded), expected, "depends on line breaks");
}

#[test]
fn clauses_and_ctes_start_their_own_lines() {
    assert_canonical(
        "\
with
  a as (
    select
      1 as x
  ),
  b as (
    select
      2 as y
  )
select
  a.x,
  b.y
from
  a
  inner join b on a.x = b.y
where
  a.x = 1;
",
    );
}

#[test]
fn case_stays_inline_until_it_has_to_span_lines() {
    assert_canonical(
        "\
select
  case when a then 'x' else 'y' end as short,
  case
    when a then 'first' -- why
    else 'second'
  end as commented
from
  t;
",
    );
}

#[test]
fn brackets_holding_a_comment_expand_in_full() {
    assert_canonical(
        "\
insert into t (a, b, c) values (
  1,
  'x', -- reason
  'y'
),
(2, 'p', 'q');
",
    );
}

#[test]
fn comment_lines_follow_the_next_code_line() {
    assert_canonical(
        "\
select
  -- first
  a,
  -- second
  b
from
  t
where
  -- reason
  a = 1;
",
    );
}

#[test]
fn trailing_comments_stay_and_only_the_code_is_wrapped() {
    assert_canonical(
        "\
select
  concat(
    'a rather long string literal',
    another_column_name,
    'and one more literal'
  ), -- the comment stays on this line however long it is
  b
from
  t;
",
    );
}

#[test]
fn blank_lines_between_statements_survive() {
    assert_canonical(
        "\
-- group one
create table t (a integer, b text);

-- group two
create index t_a_idx on t using btree (a);
",
    );
}

#[test]
fn long_ddl_breaks_at_its_clauses() {
    assert_canonical(
        "\
create trigger a_rather_long_trigger_name_for_updates
  before update on a_schema.a_rather_long_table_name for each row execute function f();

create policy a_rather_long_policy_name on a_schema.a_rather_long_table_name
  as restrictive to a_member_role
using ((org_id = current_org()));

alter table only a_schema.a_rather_long_table_name
  add constraint a_rather_long_constraint_name unique (org_id, name);
",
    );
}

#[test]
fn operators_and_commas_never_move_across_comments() {
    assert_canonical(
        "\
select
  a
from
  t
where
  (a = 1)
  or -- reason
  (b = 2);
",
    );
}
