use hashbrown::HashMap;
use regex::Regex;
use sqruff_lib_core::dialects::syntax::{SyntaxKind, SyntaxSet};

use super::cp01::handle_segment;
use crate::core::config::Value;
use crate::core::rules::context::RuleContext;
use crate::core::rules::crawlers::{Crawler, SegmentSeekerCrawler};
use crate::core::rules::{Erased, ErasedRule, LintResult, Rule, RuleGroups};

#[derive(Debug, Default, Clone)]
pub struct RuleCP05 {
    extended_capitalisation_policy: String,
    ignore_words: Vec<String>,
    ignore_words_regex: Vec<Regex>,
}

impl Rule for RuleCP05 {
    fn load_from_config(&self, config: &HashMap<String, Value>) -> Result<ErasedRule, String> {
        let strings = |key: &str| -> Result<Vec<String>, String> {
            let invalid = || format!("{key} must contain strings");
            match config.get(key) {
                None | Some(Value::None) => Ok(Vec::new()),
                Some(Value::String(csv)) => Ok(csv.split(',').map(str::to_owned).collect()),
                Some(Value::Array(values)) => values
                    .iter()
                    .map(|v| v.as_string().map(str::to_owned).ok_or_else(invalid))
                    .collect(),
                _ => Err(invalid()),
            }
        };
        Ok(RuleCP05 {
            extended_capitalisation_policy: config
                .get("extended_capitalisation_policy")
                .and_then(Value::as_string)
                .ok_or("extended_capitalisation_policy must be a string")?
                .to_owned(),
            ignore_words: strings("ignore_words")?
                .iter()
                .map(|s| s.to_lowercase())
                .collect(),
            ignore_words_regex: strings("ignore_words_regex")?
                .iter()
                .map(|s| Regex::new(s).map_err(|err| err.to_string()))
                .collect::<Result<_, _>>()?,
        }
        .erased())
    }

    fn name(&self) -> &'static str {
        "capitalisation.types"
    }

    fn description(&self) -> &'static str {
        "Inconsistent capitalisation of datatypes."
    }

    fn long_description(&self) -> &'static str {
        r#"
**Anti-pattern**

In this example, `int` and `unsigned` are in lower-case whereas `VARCHAR` is in upper-case.

```sql
CREATE TABLE t (
    a int unsigned,
    b VARCHAR(15)
);
```

**Best practice**

Ensure all datatypes are consistently upper or lower case

```sql
CREATE TABLE t (
    a INT UNSIGNED,
    b VARCHAR(15)
);
```
"#
    }

    fn groups(&self) -> &'static [RuleGroups] {
        &[
            RuleGroups::All,
            RuleGroups::Core,
            RuleGroups::Capitalisation,
        ]
    }

    fn eval(&self, context: &RuleContext) -> Vec<LintResult> {
        let mut results = Vec::new();

        if context.segment.is_type(SyntaxKind::PrimitiveType)
            || context.segment.is_type(SyntaxKind::DatetimeTypeIdentifier)
            || context.segment.is_type(SyntaxKind::DataType)
        {
            for seg in context.segment.segments() {
                if seg.is_type(SyntaxKind::Symbol)
                    || seg.is_type(SyntaxKind::Identifier)
                    || seg.is_type(SyntaxKind::QuotedIdentifier)
                    || seg.is_type(SyntaxKind::QuotedLiteral)
                    || !seg.segments().is_empty()
                {
                    continue;
                }

                if self.ignore_words.contains(&seg.raw().to_lowercase())
                    || self
                        .ignore_words_regex
                        .iter()
                        .any(|pattern| pattern.is_match(seg.raw().as_ref()))
                {
                    continue;
                }

                results.push(handle_segment(
                    "Datatypes",
                    &self.extended_capitalisation_policy,
                    "extended_capitalisation_policy",
                    seg.clone(),
                    context,
                ));
            }
        }

        results
    }

    fn is_fix_compatible(&self) -> bool {
        true
    }

    fn crawl_behaviour(&self) -> Crawler {
        SegmentSeekerCrawler::new(
            const {
                SyntaxSet::new(&[
                    SyntaxKind::DataTypeIdentifier,
                    SyntaxKind::PrimitiveType,
                    SyntaxKind::DatetimeTypeIdentifier,
                    SyntaxKind::DataType,
                ])
            },
        )
        .into()
    }
}
