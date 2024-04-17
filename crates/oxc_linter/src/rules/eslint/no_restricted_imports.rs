use oxc_diagnostics::{
    miette::{self, Diagnostic},
    thiserror::Error,
};
use oxc_macros::declare_oxc_lint;
use oxc_span::{CompactStr, Span};

use crate::{context::LintContext, rule::Rule};

#[derive(Debug, Error, Diagnostic)]
#[error("eslint(no-restricted-imports): Disallow specified modules when loaded by import")]
#[diagnostic(severity(warning), help("{0}"))]
struct NoRestrictedImportsDiagnostic(String, #[label] pub Span);

/// TODO
#[derive(Debug, Default, Clone)]
pub struct NoRestrictedImports {
    paths: Vec<PathConfig>,
}

#[derive(Debug, Clone)]
struct PathConfig {
    path: CompactStr,
    message: CompactStr,
}

declare_oxc_lint!(
  /// ### What it does
  ///
  /// Disallow specified modules when loaded by import
  ///
  /// ### Example
  ///
  /// TODO
  ///
  NoRestrictedImports,
  restriction
);

impl Rule for NoRestrictedImports {
    fn from_configuration(value: serde_json::Value) -> Self {
        let obj = value;
        Self {
            paths: obj
                .get("paths")
                .and_then(serde_json::Value::as_array)
                .unwrap_or(&vec![])
                .iter()
                .map(|p| {
                    let path = CompactStr::from(p["path"].as_str().unwrap());
                    let message: CompactStr = CompactStr::from(p["message"].as_str().unwrap_or(""));
                    PathConfig { path, message }
                })
                .collect(),
        }
    }

    fn run_once(&self, ctx: &LintContext<'_>) {
        let module_record = ctx.semantic().module_record();
        for import_entry in &module_record.import_entries {
            let module_request = import_entry.module_request.name();
            if let Some(fail) = self.paths.iter().find(|p| p.path == module_request) {
                ctx.diagnostic(NoRestrictedImportsDiagnostic(
                    fail.message.to_string(),
                    import_entry.module_request.span(),
                ));
            }
        }
    }
}

#[test]
fn test() {
    use serde_json::json;

    use crate::tester::Tester;

    let pass = vec![(r#"import './malformed.js'"#, Some(json!({ "paths": [] })))];

    let fail = vec![
        (
            r#"import moment from "moment""#,
            Some(json!({ "paths": [{ "path": "moment", "message": "Do not use 'moment'" }] })),
        ),
        (
            r#"import moment from "moment-timezone""#,
            Some(
                json!({ "paths": [{ "path": "moment", "message": "Do not use 'moment'" }, { "path": "moment-timezone", "message": "Do not use 'moment-timezone'" }] }),
            ),
        ),
    ];

    Tester::new(NoRestrictedImports::NAME, pass, fail).test_and_snapshot();
}
