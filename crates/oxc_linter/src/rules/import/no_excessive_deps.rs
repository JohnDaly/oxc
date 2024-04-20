#![allow(clippy::cast_possible_truncation)]

use std::{
    collections::HashSet,
    ffi::OsStr,
    path::{Component, PathBuf},
};

use itertools::Itertools;
use oxc_diagnostics::{
    miette::{self, Diagnostic},
    thiserror::Error,
};
use oxc_macros::declare_oxc_lint;
use oxc_span::CompactStr;
use oxc_syntax::module_record::ModuleRecord;

use crate::{context::LintContext, rule::Rule};

#[derive(Debug, Error, Diagnostic)]
#[error("eslint-plugin-import(no-excessive-deps): Module has excessive dependencies")]
#[diagnostic(
    severity(warning),
    help("{0} has {1} dependencies, exceeding the maximum limit of {2}\n{3}")
)]
struct NoExcessiveDepsDiagnostic(String, String, String, String);

#[derive(Debug, Clone)]
pub struct NoExcessiveDeps {
    /// maximum number of dependencies that a module can have
    max_deps: u32,
    /// Whether or not dependencies that are type-only should be ignored
    ignore_types: bool,
    /// Whether or not the dependency list should be printed in output
    should_print_deps: bool,
}

impl Default for NoExcessiveDeps {
    fn default() -> Self {
        Self { max_deps: u32::MAX, ignore_types: false, should_print_deps: true }
    }
}

declare_oxc_lint!(
    /// ### What it does
    ///
    /// Ensures that a module doesn't depend on an excessive number of modules.
    ///
    /// ### Why is this bad?
    ///
    /// Large dependency graphs are a sign that code is poorly modularized, which can make it harder to reason about
    ///
    /// ```
    NoExcessiveDeps,
    nursery
);

fn is_path_in_node_modules(p: &PathBuf) -> bool {
    p.components().any(|c| matches!(c, Component::Normal(p) if p == OsStr::new("node_modules")))
}

impl Rule for NoExcessiveDeps {
    fn from_configuration(value: serde_json::Value) -> Self {
        let obj = value.get(0);
        Self {
            max_deps: obj
                .and_then(|v| v.get("maxDeps"))
                .and_then(serde_json::Value::as_number)
                .and_then(serde_json::Number::as_u64)
                .map_or(u32::MAX, |n| n as u32),
            ignore_types: obj
                .and_then(|v| v.get("ignoreTypes"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            should_print_deps: obj
                .and_then(|v| v.get("shouldPrintDeps"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true),
        }
    }

    fn run_once(&self, ctx: &LintContext<'_>) {
        let module_record = ctx.semantic().module_record();

        let cwd = std::env::current_dir().unwrap();

        let mut state = State::default();
        self.walk_deps(&mut state, module_record);

        let num_internal_deps = state
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
        if num_internal_deps > usize::try_from(self.max_deps).unwrap() {
            let file_path_string: String = ctx
                .file_path()
                .strip_prefix(&cwd)
                .unwrap_or(ctx.file_path())
                .to_string_lossy()
                .replace('\\', "/");
            let loaded_modules_string: String = num_internal_deps.to_string();
            let max_deps_string: String = self.max_deps.to_string();
            let mut deps_string: String = "".to_string();
            if self.should_print_deps {
                deps_string = state
                    .traversed
                    .into_iter()
                    .map(|p| {
                        let path =
                            p.strip_prefix(&cwd).unwrap_or(&p).to_string_lossy().replace('\\', "/");
                        path
                    })
                    .sorted()
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            ctx.diagnostic(NoExcessiveDepsDiagnostic(
                file_path_string,
                loaded_modules_string,
                max_deps_string,
                deps_string,
            ))
        }
    }
}

#[derive(Debug, Default)]
struct State {
    traversed: HashSet<PathBuf>,
    stack: Vec<(CompactStr, PathBuf)>,
}

impl NoExcessiveDeps {
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

    let pass = vec![
        (r#"import flatDeps from "./flat-deps/entry.js""#, None),
        (r#"import nestedDeps from "./nested-deps/entry.js""#, None),
        (
            r#"import nestedDeps from "./type-only-deps/entry.ts""#,
            Some(json!([{"maxDeps": 1, "ignoreTypes": true}])),
        ),
        (
            r#"import nestedDeps from "./some-type-deps/entry.ts""#,
            Some(json!([{"maxDeps": 2, "ignoreTypes": true}])),
        ),
        (
            r#"import nestedDeps from "./some-type-deps/entry.ts""#,
            Some(json!([{"maxDeps": 2, "ignoreTypes": true}])),
        ),
    ];

    let fail = vec![
        (r#"import flatDeps from "./flat-deps/entry.js""#, Some(json!([{"maxDeps": 3}]))),
        (r#"import nestedDeps from "./nested-deps/entry.js""#, Some(json!([{"maxDeps": 3}]))),
        (
            r#"import nestedDeps from "./type-only-deps/entry.ts""#,
            Some(json!([{"maxDeps": 1, "ignoreTypes": false}])),
        ),
        (
            r#"import nestedDeps from "./some-type-deps/entry.ts""#,
            Some(json!([{"maxDeps": 2, "ignoreTypes": false}])),
        ),
        (
            r#"import nestedDeps from "./some-type-deps/entry.ts""#,
            Some(json!([{"maxDeps": 1, "ignoreTypes": true}])),
        ),
    ];

    Tester::new(NoExcessiveDeps::NAME, pass, fail)
        .change_rule_path("no-excessive-deps/entry.js")
        .with_import_plugin(true)
        .test_and_snapshot();
}
