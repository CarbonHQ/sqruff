//! Layout-independence harness for the formatter.
//!
//! `canon <config> format <file>` prints the formatted file.
//! `canon <config> explode <file>` prints the file with every token on its own
//! line, keeping blank lines and comments.
//! `canon <config> check <files...>` reports, per file, whether formatting the
//! file and formatting its exploded form agree, and how many passes each
//! needs to reach a fixed point.

use std::process::ExitCode;

use sqruff_lib::core::config::FluffConfig;
use sqruff_lib::core::linter::core::Linter;
use sqruff_lib_core::dialects::syntax::SyntaxKind;
use sqruff_lib_core::parser::segments::{LayoutContext, Tables};

const MAX_PASSES: usize = 4;

fn format_once(linter: &Linter, sql: &str) -> Result<String, String> {
    let layout = LayoutContext {
        tab_width: 2,
        ..LayoutContext::default()
    };
    let result = linter
        .lint_string_with_layout(sql, None, true, layout)
        .map_err(|error| error.to_string())?;

    if result.has_parse_or_templating_errors() {
        return Err("parse error".to_owned());
    }

    Ok(result.fix_string())
}

/// Format until stable. Returns the output and the number of passes that changed it.
fn format_stable(linter: &Linter, sql: &str) -> Result<(String, usize), String> {
    let mut current = sql.to_owned();

    for pass in 0..MAX_PASSES {
        let next = format_once(linter, &current)?;
        if next == current {
            return Ok((current, pass));
        }
        current = next;
    }

    Ok((current, MAX_PASSES))
}

/// Put every token on its own line, keeping blank lines and comments as they
/// are. This is the layout furthest from what the formatter produces.
fn explode(linter: &Linter, sql: &str) -> String {
    let tables = Tables::default();
    let lexer = linter.config().get_dialect().lexer();
    let (tokens, _) = lexer.lex(&tables, sql);
    let mut output = String::with_capacity(sql.len());
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

        let is_trailing_comment = token.get_type() == SyntaxKind::InlineComment && line_breaks == 0;

        if !output.is_empty() {
            if line_breaks > 1 {
                output.push_str("\n\n");
            } else if is_trailing_comment {
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

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [config, mode, files @ ..] = args.as_slice() else {
        eprintln!("usage: canon <config> <format|explode|check> <files...>");
        return ExitCode::FAILURE;
    };

    let source = std::fs::read_to_string(config).expect("config");
    let linter = Linter::new(FluffConfig::from_source(&source, None), None, None, true).unwrap();
    let mut failures = 0;

    for file in files {
        let sql = std::fs::read_to_string(file).expect("sql file");

        match mode.as_str() {
            "format" => match format_once(&linter, &sql) {
                Ok(output) => print!("{output}"),
                Err(error) => {
                    eprintln!("{file}: {error}");
                    failures += 1;
                }
            },
            "explode" => print!("{}", explode(&linter, &sql)),
            "check" => {
                let original = format_stable(&linter, &sql);
                let collapsed = format_stable(&linter, &explode(&linter, &sql));

                match (original, collapsed) {
                    (Ok((a, a_passes)), Ok((b, b_passes))) => {
                        let differing = a
                            .lines()
                            .zip(b.lines())
                            .filter(|(left, right)| left != right)
                            .count()
                            + a.lines().count().abs_diff(b.lines().count());
                        let ok = a == b && a_passes <= 1 && b_passes <= 1;
                        if !ok {
                            failures += 1;
                        }
                        println!(
                            "{}\t{file}\tpasses={a_passes}/{b_passes}\tlines={}\tdiffering~{differing}",
                            if ok { "ok" } else { "FAIL" },
                            a.lines().count()
                        );
                    }
                    (a, b) => {
                        failures += 1;
                        println!("ERROR\t{file}\t{:?} {:?}", a.err(), b.err());
                    }
                }
            }
            _ => {
                eprintln!("unknown mode {mode}");
                return ExitCode::FAILURE;
            }
        }
    }

    if failures > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
