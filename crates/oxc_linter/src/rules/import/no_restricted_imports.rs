use oxc_diagnostics::{
    miette::{self, Diagnostic},
    thiserror::Error,
};
use oxc_macros::declare_oxc_lint;
use oxc_span::Span;
use oxc_syntax::module_record::ImportImportName;

use crate::{context::LintContext, rule::Rule};

#[derive(Debug, Error, Diagnostic)]
#[error("eslint-plugin-import(no-restricted-imports): TODO")]
#[diagnostic(severity(warning), help("TODO"))]
struct DefaultDiagnostic(String, #[label] pub Span);

/// TODO
#[derive(Debug, Default, Clone)]
pub struct Default {

};

declare_oxc_lint!(
  /// ### What it does
  ///
  /// Disallow specified modules when loaded by import
  ///
  /// ### Example
  ///
  /// TODO
  ///
  Default,
  nursery
);

impl Rule for Default {
  fn from_configuration(value: serde_json::Value) -> Self {
    let obj = value.get(0);
    Self {
        max_depth: obj
            .and_then(|v| v.get("maxDepth"))
            .and_then(serde_json::Value::as_number)
            .and_then(serde_json::Number::as_u64)
            .map_or(u32::MAX, |n| n as u32),
        ignore_types: obj
            .and_then(|v| v.get("ignoreTypes"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or_default(),
        ignore_external: obj
            .and_then(|v| v.get("ignoreExternal"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or_default(),
        allow_unsafe_dynamic_cyclic_dependency: obj
            .and_then(|v| v.get("allowUnsafeDynamicCyclicDependency"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or_default(),
    }
}

    fn run_once(&self, ctx: &LintContext<'_>) {
        let module_record = ctx.semantic().module_record();
        for import_entry in &module_record.import_entries {
            let ImportImportName::Default(default_span) = import_entry.import_name else {
                continue;
            };

            let specifier = import_entry.module_request.name();
            let Some(remote_module_record_ref) = module_record.loaded_modules.get(specifier) else {
                continue;
            };
            if remote_module_record_ref.not_esm {
                continue;
            }
            if remote_module_record_ref.export_default.is_none()
                && !remote_module_record_ref.exported_bindings.contains_key("default")
            {
                ctx.diagnostic(DefaultDiagnostic(specifier.to_string(), default_span));
            }
        }
    }
}

#[test]
fn test() {
    use crate::tester::Tester;

    let pass = vec!["import './malformed.js'"];

    let fail = vec![];

    Tester::new(Default::NAME, pass, fail)
        .change_rule_path("index.js")
        .with_import_plugin(true)
        .test_and_snapshot();
}
