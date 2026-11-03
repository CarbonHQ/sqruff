<p align="center">
  <a href="https://quary.dev">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://utfs.io/f/30765a8e-3dd9-4dc3-b905-11de822e71e4-yajpew.png">
      <img src="https://utfs.io/f/30765a8e-3dd9-4dc3-b905-11de822e71e4-yajpew.png" height="128">
    </picture>
    <h1 align="center">sqruff</h1>
  </a>
</p>

<p align="center">
  <a aria-label="Quary logo" href="https://quary.dev/">
    <img src="https://img.shields.io/badge/MADE%20BY%20Quary-000000.svg?style=for-the-badge&logo=Quary&labelColor=000">
  </a>
  <a aria-label="CodSpeed" href="https://codspeed.io/quarylabs/sqruff?utm_source=badge">
    <img src="https://img.shields.io/endpoint?url=https://codspeed.io/badge.json?utm_source=badge" alt="CodSpeed Badge"/>
  </a>
</p>

`sqruff` is a SQL linter and formatter written in Rust.

- Linting: advanced, configurable SQL linting
- Formatting: automated, configurable formatting
- Speed: fast and efficient
- Portability: easy to integrate into dev workflows

Try it in the browser: https://playground.quary.dev

## Table of contents

1. [About this fork](#about-this-fork)
2. [Quickstart](#quickstart)
3. [Editor integrations](#editor-integrations)
4. [Documentation](#documentation)

## About this fork

[CarbonHQ's `oxfmt` branch](https://github.com/CarbonHQ/sqruff/tree/oxfmt)
supplies the Rust SQL backend for [our Oxfmt fork](https://github.com/CarbonHQ/oxc/tree/sql).
It follows upstream Sqruff; Oxfmt pins these crates by Git revision rather than
vendoring a source copy.

The retained changes address SQL found in CarbonHQ's PostgreSQL monorepo:

- Measure embedded SQL using printed interpolation widths and host indentation,
  without rebuilding the cached linter for each query. Width checks and candidate
  break selection both use display columns, including Unicode and tabs.
- Expand function arguments when breaking an arithmetic/concatenation operator
  would still leave an overlong operand. Give comparison continuations their own
  implicit indentation scope, so they do not align with surrounding AND/OR clauses.
  Scalar-subquery parentheses own their contents instead of accumulating a second
  comparison indent.
  Word comparisons keep spaces between their keywords while symbolic operators
  such as `<=` remain joined.
- Preserve wrapped policy USING/WITH CHECK indentation while letting inline
  parentheses own their scope. Keep the header indent through USING expressions
  attached to wrapped role lines. Indent function-level SET contents independently
  of following clauses, including ALTER FUNCTION settings. Keep existing AS body
  placement when it fits the line width.
- Keep configured inline GRANT/REVOKE privilege lists together only when they fit,
  so long lists wrap without alternating between formatter runs.
- Join fitting relation/alias lines using printed widths and host indentation,
  preserving comment boundaries and wrapping longer lines. Include LOCAL/SESSION
  modifiers in session SET indentation so wrapped settings remain consistent.
- Keep table aliases at the relation's indentation without changing SELECT alias
  continuations, and give ON CONFLICT target predicates a normal WHERE scope.
- Treat statistics targets, foreign-key reference/action clauses, and policy
  predicates as layout structures. This keeps the first statistics target aligned
  with the rest, favors clause boundaries over splitting small column lists,
  and prevents policy header indentation from leaking into USING parentheses.
- Keep upsert SET/WHERE clauses at statement level and indent their contents,
  matching ordinary UPDATE clauses without an extra DO UPDATE nesting level.
- Parse quantified comparisons with PostgreSQL array constructors and `OVERLAY`
  arguments containing expressions, so valid application SQL can be formatted.
- Key SELECT-target parse caches by their input to prevent incorrect reuse across
  embedded query shapes, including incomplete nested CTEs.
- Find long-line comment clauses by token identity rather than byte/character
  offsets, preserving comment text and avoiding repeated scans.
- Keep long single-target SELECT formatting stable across repeated runs.
- Add indentation scopes for INSERT targets and VALUES rows, FILTER conditions,
  partial-index WHERE conditions, CREATE STATISTICS columns/tables, SET values,
  role options, extension/storage options, DROP DATABASE options, GRANT privileges,
  and policy clauses. LT02 can then enforce the configured width instead of
  flattening continuation lines to level zero.
- Preserve indentation markers when flattening transparent bracket content, so
  nested grammar scopes reach the indentation engine.
- Apply required implicit-indent line collapse before other indentation fixes,
  using original source anchors. This avoids unpositioned whitespace anchors and
  conflicting fixes.
- Align raw SQL using positions after earlier layout fixes, so explicit source
  alignment and SELECT-target line breaks converge within one formatting call.
- Honor `ignore_words` and `ignore_words_regex` for datatype capitalization
  (CP05), matching the other capitalization rules.

Regression cases cover parsing, expected formatting, and second-pass stability,
including `implicit_indents = forbid`, `allow`, and `require`. Upstream already
includes the earlier VARIADIC boundary fix and faster depth-map lookups; those
implementations are reused here.

Changes were developed with Codex assistance and remain the human contributor's
responsibility to review and test.

## Quickstart

### Install (macOS)

```bash
brew install sqruff
```

For other platforms (pip, cargo, binary downloads), see the [installation guide](https://playground.quary.dev/docs/getting-started/installation/).

### Lint a project

```bash
sqruff lint . --dialect postgres
```

### Configure a project

Create a `.sqruff` file in your project root:

```ini
[sqruff]
dialect = postgres
```

Then run without the `--dialect` flag:

```bash
sqruff lint .
```

## Editor integrations

`sqruff` ships an LSP server (`sqruff lsp`) usable from any editor. This repo
maintains two integrations:

- VS Code: [`editors/code`](editors/code)
- Zed: [`editors/zed`](editors/zed)

## Documentation

Full documentation: [playground.quary.dev/docs](https://playground.quary.dev/docs/)

Key entry points:

- [Installation](https://playground.quary.dev/docs/getting-started/installation/)
- [Usage](https://playground.quary.dev/docs/usage/lint/)
- [Configuration](https://playground.quary.dev/docs/usage/configuration/)
- [Rules](https://playground.quary.dev/docs/reference/rules/)
- [CLI reference](https://playground.quary.dev/docs/reference/cli/)
