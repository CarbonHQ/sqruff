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

This is CarbonHQ's Sqruff fork for [native SQL formatting in Oxfmt](https://github.com/CarbonHQ/oxc/blob/sql/npm/oxfmt/README.md#sql-formatting). It is based on **Sqruff 0.41.0**, at upstream commit [`aa4c5730d697794ffaf72a4c187d64522e5820c7`](https://github.com/quarylabs/sqruff/commit/aa4c5730d697794ffaf72a4c187d64522e5820c7).

The [source patch commit](https://github.com/CarbonHQ/sqruff/commit/cafbdc311f9ac963880a1f383e85d18f31670a6c) modifies seven source files to fix parsing, preserve SQL meaning, make repeated formatting stable, and reduce repeated syntax-tree searches. Oxc pins `sqruff-lib`, `sqruff-lib-core`, and `sqruff-lib-dialects` to that immutable commit. npm users install prebuilt Oxfmt bindings and do not need a separate Sqruff executable or Rust installation.

### Parsing and SQL preservation

- **Include the input index in parser-cache keys.** Cached match spans refer to indices in the current input slice. Fixes and nested parsing can produce segments with the same source and working positions at different indices; reusing a cached span for the wrong index gives an incorrect match. The key now includes that index. A regression test covers repeated positions at distinct indices. See [`context.rs`](crates/lib-core/src/parser/context.rs) and [`match_algorithms.rs`](crates/lib-core/src/parser/match_algorithms.rs).
- **Parse PostgreSQL `OVERLAY` expressions.** Support `OVERLAY(string PLACING replacement FROM start [FOR length])`, including casts and arithmetic in its arguments. Valid PostgreSQL queries using this syntax previously failed to parse. See [`postgres.rs`](crates/lib-dialects/src/postgres.rs).
- **Accept array expressions in PostgreSQL `ANY` / `ALL` / `SOME` comparisons.** Support array literals and array-returning function calls while retaining quantified subqueries. Expression matching comes before selectable matching so greedy SELECT error recovery does not hide a valid array expression. This fixes valid comparisons such as `value = ANY(ARRAY[1, 2])`. See [`postgres.rs`](crates/lib-dialects/src/postgres.rs).
- **Preserve the boundary between `VARIADIC` and an array literal.** Leave `VARIADIC` in the function-contents grammar rather than inside the typed-array-literal node. The array node's spacing rules could otherwise join `VARIADIC ARRAY[...]` into the identifier `VARIADICARRAY`, changing the SQL's meaning. See [`postgres.rs`](crates/lib-dialects/src/postgres.rs).

### Stable formatting

- **Keep LT09 from undoing LT05's line wrapping.** Before moving a single SELECT target onto the SELECT line, account for indentation, SELECT modifiers, and `max_line_length`. If the combined line would be too long, keep the target on its own line. Previously LT05 could wrap the target and LT09 could move it back, causing successive formatter runs to alternate between layouts. See [`lt09.rs`](crates/lib/src/rules/layout/lt09.rs).

### Performance on large SQL files

- **Index comment-clause tokens once per LT05 rule pass.** The `ignore_comment_clauses` handling previously searched the syntax tree for each candidate token on a long line. Build comment-token membership and raw-token index maps once instead, retaining the existing comment-clause exemption while avoiding repeated tree walks. See [`lt05.rs`](crates/lib/src/rules/layout/lt05.rs).
- **Share ancestor paths within each reflow pass.** Lazily build an index of raw-token ancestor paths and reuse it across rebreak lookups and long lines. The previous implementation repeatedly searched the whole tree for these paths. The cache belongs to one immutable root and one pass, keeping it out of the syntax tree to avoid reference-count cycles; an equivalent segment with a different ID still uses the original lookup as a fallback. See [`rebreak.rs`](crates/lib/src/utils/reflow/rebreak.rs) and [`reindent.rs`](crates/lib/src/utils/reflow/reindent.rs).

### Integration and maintenance

Oxfmt-specific choices remain in Oxc's adapter: running layout rules only, reusing configured linter instances, and setting a finite parse-node budget of 1,000,000 while retaining Sqruff's depth limit. These are separate from the source patches in this fork.

Integration regressions live in Oxc's [SQL formatter tests](https://github.com/CarbonHQ/oxc/blob/sql/crates/oxc_formatter_sql/src/lib.rs) and [Node API tests](https://github.com/CarbonHQ/oxc/blob/sql/apps/oxfmt/test/api/sql.test.ts). The parser-cache regression also lives beside the implementation in this repository. Equivalent upstream fixes should replace these patches when adopting a newer Sqruff release; update the three pinned crates together and rerun the integration and package-install checks.

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
