use std::{ops::Range, sync::Arc};

use hemtt_common::config::{LintConfig, ProjectConfig};
use hemtt_workspace::{
    lint::{AnyLintRunner, Lint, LintRunner},
    reporting::{Code, Codes, Diagnostic, Processed},
};

use crate::{Item, Value};

crate::lint!(LintC01InvalidValue);

impl Lint<()> for LintC01InvalidValue {
    fn ident(&self) -> &str {
        "invalid_value"
    }

    fn sort(&self) -> u32 {
        10
    }

    fn description(&self) -> &str {
        "Reports on any values in the config that could not be parsed into a valid config value."
    }

    fn documentation(&self) -> &str {
r#"### Example

**Incorrect**
```hpp
class MyClass {
    data = 1.0.0; // invalid value, should be quoted
};
```
**Correct**
```hpp
class MyClass {
    data = "1.0.0";
};
```

### Explanation

Arma configs only support Strings, Numbers, and Arrays. While other tools would guess that `1.0.0` is a string (often called auto-quote), this behaviour can introduce unintentional mistakes and is not supported by HEMTT.
"#
    }

    fn default_config(&self) -> LintConfig {
        LintConfig::error()
    }

    fn runners(&self) -> Vec<Box<dyn AnyLintRunner<()>>> {
        vec![Box::new(RunnerValue), Box::new(RunnerItem)]
    }
}

struct RunnerValue;

impl LintRunner<()> for RunnerValue {
    type Target = Value;
    fn run(
        &self,
        _project: Option<&ProjectConfig>,
        _config: &LintConfig,
        processed: Option<&Processed>,
        target: &Value,
        _data: &(),
    ) -> Codes {
        let Some(processed) = processed else {
            return vec![];
        };
        if let Value::Invalid(invalid) = target {
            vec![if processed
                .mapping(invalid.start)
                .is_some_and(hemtt_workspace::reporting::Mapping::was_macro)
            {
                Arc::new(CodeC01InvalidValueMacro::new(invalid.clone(), processed))
            } else {
                Arc::new(CodeC01InvalidValue::new(invalid.clone(), processed))
            }]
        } else {
            vec![]
        }
    }
}

struct RunnerItem;
impl LintRunner<()> for RunnerItem {
    type Target = Item;
    fn run(
        &self,
        _project: Option<&ProjectConfig>,
        _config: &LintConfig,
        processed: Option<&Processed>,
        target: &Item,
        _data: &(),
    ) -> Codes {
        let Some(processed) = processed else {
            return vec![];
        };
        if let Item::Invalid(invalid) = target {
            vec![if processed
                .mapping(invalid.start)
                .is_some_and(hemtt_workspace::reporting::Mapping::was_macro)
            {
                Arc::new(CodeC01InvalidValueMacro::new(invalid.clone(), processed))
            } else {
                Arc::new(CodeC01InvalidValue::new(invalid.clone(), processed))
            }]
        } else {
            vec![]
        }
    }
}

#[allow(clippy::module_name_repetitions)]
pub struct CodeC01InvalidValue {
    span: Range<usize>,
    diagnostic: Option<Diagnostic>,
    value: String,
}

impl Code for CodeC01InvalidValue {
    fn ident(&self) -> &'static str {
        "L-C01"
    }

    fn link(&self) -> Option<&str> {
        Some("/analysis/config.html#invalid_value")
    }

    fn message(&self) -> String {
        "property's value could not be parsed".to_string()
    }

    fn label_message(&self) -> String {
        "invalid value".to_string()
    }

    fn help(&self) -> Option<String> {
        match self.value.as_str() {
            "true" | "false" => Some("use quotes `\"`, or 0 for false and 1 for true".to_string()),
            _ => {
                if self.value.starts_with('\'') && self.value.ends_with('\'') {
                    Some("quotes need to be `\"` instead of `'`".to_string())
                } else {
                    Some("use quotes `\"` around the value".to_string())
                }
            }
        }
    }

    fn diagnostic(&self) -> Option<Diagnostic> {
        self.diagnostic.clone()
    }
}

impl CodeC01InvalidValue {
    pub fn new(span: Range<usize>, processed: &Processed) -> Self {
        Self {
            value: processed.as_str()[span.clone()].to_string(),
            span,
            diagnostic: None,
        }
        .generate_processed(processed)
    }

    fn generate_processed(mut self, processed: &Processed) -> Self {
        self.diagnostic = Diagnostic::new_for_processed(&self, self.span.clone(), processed);
        self
    }
}

pub struct CodeC01InvalidValueMacro {
    span: Range<usize>,
    diagnostic: Option<Diagnostic>,
}

impl Code for CodeC01InvalidValueMacro {
    fn ident(&self) -> &'static str {
        "L-C01M"
    }

    fn link(&self) -> Option<&str> {
        Some("/analysis/config.html#invalid_value")
    }

    fn message(&self) -> String {
        "macro's result could not be parsed".to_string()
    }

    fn label_message(&self) -> String {
        "invalid macro result".to_string()
    }

    fn help(&self) -> Option<String> {
        Some("perhaps this macro has a `Q_` variant or you need `QUOTE(..)`".to_string())
    }

    fn diagnostic(&self) -> Option<Diagnostic> {
        self.diagnostic.clone()
    }
}

impl CodeC01InvalidValueMacro {
    pub fn new(span: Range<usize>, processed: &Processed) -> Self {
        Self {
            span,
            diagnostic: None,
        }
        .generate_processed(processed)
    }

    fn generate_processed(mut self, processed: &Processed) -> Self {
        self.diagnostic = Diagnostic::new_for_processed(&self, self.span.clone(), processed);
        if let Some(diag) = &mut self.diagnostic {
            diag.notes.push(format!(
                "The processed output was:\n{} ",
                &processed.as_str()[self.span.start..self.span.end]
            ));
        }
        self
    }
}