use std::{
    collections::HashSet,
    ffi::OsStr,
    path::{Component, PathBuf},
};

use oxc_ast::AstKind;
use oxc_diagnostics::{
    miette::{self, Diagnostic},
    thiserror::Error,
};
use oxc_macros::declare_oxc_lint;
use oxc_semantic::{AstNode, ModuleRecord};
use oxc_span::{CompactStr, Span};

use crate::{context::LintContext, rule::Rule};

#[derive(Debug, Error, Diagnostic)]
#[error("oxc(no-export-all): Do not export from modules using export-all syntax")]
#[diagnostic(
    severity(warning),
    help(
        "Avoid re-exporting * from a module, it leads to unused imports and prevents treeshaking.\n{1}"
    )
)]
struct NoExportAllDiagnostic(#[label] pub Span, String);

#[derive(Debug, Clone)]
pub struct NoExportAll {
    show_dependency_size: bool,
    /// Whether or not dependencies that are type-only should be ignored
    ignore_types: bool,
}

impl Default for NoExportAll {
    fn default() -> Self {
        Self { show_dependency_size: false, ignore_types: false }
    }
}

declare_oxc_lint!(
    /// ### What it does
    ///
    /// Prevent indirect exports that re-export all bindings from other modules
    ///
    /// ### Why is this bad?
    ///
    /// Avoid re-exporting * from a module, it leads to unused imports and prevents treeshaking.
    ///
    /// ### Example
    /// ```javascript
    /// ```
    NoExportAll,
    nursery
);

impl Rule for NoExportAll {
    fn from_configuration(value: serde_json::Value) -> Self {
        let obj = value.get(0);
        Self {
            show_dependency_size: obj
                .and_then(|v| v.get("showDependencySize"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            ignore_types: obj
                .and_then(|v| v.get("ignoreTypes"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        }
    }

    fn run<'a>(&self, node: &AstNode<'a>, ctx: &LintContext<'a>) {
        if let AstKind::ExportAllDeclaration(expr) = node.kind() {
            let mut num_dependencies_for_help_message: String = "".to_string();
            if self.show_dependency_size {
                let module_record = ctx.semantic().module_record();

                // Get the module_record for the re-exported module
                let reexported_module_record = module_record
                    .loaded_modules
                    .get(&expr.source.value.clone().into_compact_str())
                    .unwrap();

                // Walk the dependencies introduced by the re-exported module
                let mut state = State::default();
                self.walk_deps(&mut state, &reexported_module_record);

                let num_loaded_internal_dependencies = state
                    .traversed
                    .clone()
                    .into_iter()
                    .filter(|traversed_path| {
                        // Ignore this file
                        if traversed_path == ctx.file_path() {
                            return false;
                        }

                        // Ignore node_modules
                        if is_path_in_node_modules(traversed_path) {
                            return false;
                        }

                        true
                    })
                    .count();

                num_dependencies_for_help_message = format!(
                    "Number of dependencies introduced from this re-export: {}",
                    num_loaded_internal_dependencies.to_string()
                );
            }
            ctx.diagnostic(NoExportAllDiagnostic(expr.span, num_dependencies_for_help_message));
        };
    }
}

#[derive(Debug, Default)]
struct State {
    traversed: HashSet<PathBuf>,
    stack: Vec<(CompactStr, PathBuf)>,
}

fn is_path_in_node_modules(p: &PathBuf) -> bool {
    p.components().any(|c| matches!(c, Component::Normal(p) if p == OsStr::new("node_modules")))
}

impl NoExportAll {
    fn walk_deps(&self, state: &mut State, module_record: &ModuleRecord) {
        let path = &module_record.resolved_absolute_path;
        if is_path_in_node_modules(path) {
            return;
        }

        for module_record_ref in &module_record.loaded_modules {
            let resolved_absolute_path = &module_record_ref.resolved_absolute_path;
            if self.ignore_types {
                let was_imported_as_type = &module_record
                    .import_entries
                    .iter()
                    .filter(|entry| entry.module_request.name() == module_record_ref.key())
                    .all(|entry| entry.is_type);
                if *was_imported_as_type {
                    continue;
                }
            }
            if !state.traversed.insert(resolved_absolute_path.clone()) {
                continue;
            }
            state.stack.push((module_record_ref.key().clone(), resolved_absolute_path.clone()));
            self.walk_deps(state, module_record_ref.value());
            state.stack.pop();
        }
    }
}

#[test]
fn test() {
    use serde_json::json;

    use crate::tester::Tester;

    let pass = vec![(r#"export { ExtfieldModel } from "./models";"#, None)];

    let fail = vec![
        (r#"export * as models from "./models";"#, Some(json!([{"showDependencySize":true}]))),
        (r#"export * from "./models";"#, Some(json!([{"showDependencySize":true}]))),
    ];

    Tester::new(NoExportAll::NAME, pass, fail)
        .change_rule_path("export-star/entry.js")
        .with_import_plugin(true)
        .test_and_snapshot();
}
