pub mod error;
pub use error::SemaError;

pub mod types;
pub use types::Ty;

pub mod scope;

pub mod eval;
pub use eval::{ConstEval, ConstValue};

pub mod resolve;

pub mod check;

use crate::ast::item::SourceFile;

/// Run the full semantic analysis pipeline on a parsed source file.
///
/// Pass 1: Name resolution — populates the symbol table.
/// Pass 2: Type checking — validates expression types and statement semantics.
///
/// Returns the final symbol table and all accumulated errors from both passes.
pub fn analyze(file: &SourceFile) -> (scope::SymbolTable, Vec<SemaError>) {
    let mut errors = Vec::new();

    // Pass 1: Name resolution.
    let mut resolver = resolve::Resolver::new();
    resolver.collect_declarations(file);
    let (table, scope_map, mut resolve_errors) = resolver.finish();
    errors.append(&mut resolve_errors);

    // Pass 2: Type checking.
    let mut checker = check::TypeChecker::new(&table, &scope_map);
    checker.check_file(file);
    errors.append(&mut checker.into_errors());

    (table, errors)
}
