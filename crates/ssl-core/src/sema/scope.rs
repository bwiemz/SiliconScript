use std::collections::HashMap;

use crate::span::Span;
use super::types::Ty;
use super::error::SemaError;

// ---------------------------------------------------------------------------
// ID types
// ---------------------------------------------------------------------------

/// Opaque handle to a scope node in the arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeId(pub u32);

/// Opaque handle to a symbol entry in the arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymbolId(pub u32);

// ---------------------------------------------------------------------------
// Scope kind
// ---------------------------------------------------------------------------

/// Describes the syntactic construct that introduced a scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    File,
    Module,
    Function,
    Block,
    Fsm,
    Pipeline,
    Test,
}

// ---------------------------------------------------------------------------
// Symbol kind
// ---------------------------------------------------------------------------

/// Describes what kind of binding a symbol represents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    Signal,
    Port,
    Const,
    Let,
    Var,
    TypeAlias,
    Module,
    Struct,
    Enum,
    EnumVariant,
    Interface,
    Fn,
    Fsm,
    Pipeline,
    GenericParam,
    LoopVar,
}

// ---------------------------------------------------------------------------
// Symbol
// ---------------------------------------------------------------------------

/// A fully-resolved symbol entry stored in the arena.
#[derive(Debug, Clone)]
pub struct Symbol {
    /// The declared name.
    pub name: String,
    /// What kind of binding this is.
    pub kind: SymbolKind,
    /// The resolved type.
    pub ty: Ty,
    /// Source location of the declaration.
    pub span: Span,
    /// The scope in which this symbol was declared.
    pub scope: ScopeId,
    /// Whether the symbol is mutable at the hardware level.
    /// Set to `true` for `Signal`, `Var`, and `LoopVar`.
    pub mutable: bool,
    /// Port direction — only meaningful for `SymbolKind::Port`.
    /// Callers should set this after calling `define()`.
    pub direction: Option<crate::ast::types::Direction>,
}

// ---------------------------------------------------------------------------
// Internal scope node
// ---------------------------------------------------------------------------

struct Scope {
    parent: Option<ScopeId>,
    kind: ScopeKind,
    /// Maps name → SymbolId for symbols declared directly in this scope.
    symbols: HashMap<String, SymbolId>,
}

// ---------------------------------------------------------------------------
// SymbolTable
// ---------------------------------------------------------------------------

/// Arena-based symbol table with hierarchical scoped lookup.
///
/// Scopes and symbols are stored in flat `Vec`s; handles (`ScopeId` /
/// `SymbolId`) are indices into those vecs.  The root scope (index 0) is a
/// `ScopeKind::File` scope created automatically.
pub struct SymbolTable {
    scopes: Vec<Scope>,
    symbols: Vec<Symbol>,
}

impl SymbolTable {
    /// Create a new, empty symbol table.  The root `File` scope is created
    /// automatically at `ScopeId(0)`.
    pub fn new() -> Self {
        let root = Scope {
            parent: None,
            kind: ScopeKind::File,
            symbols: HashMap::new(),
        };
        SymbolTable {
            scopes: vec![root],
            symbols: Vec::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Scope management
    // -----------------------------------------------------------------------

    /// Return the root file scope.
    pub fn root_scope(&self) -> ScopeId {
        ScopeId(0)
    }

    /// Create a new child scope nested inside `parent` and return its id.
    pub fn push_scope(&mut self, parent: ScopeId, kind: ScopeKind) -> ScopeId {
        let id = ScopeId(self.scopes.len() as u32);
        self.scopes.push(Scope {
            parent: Some(parent),
            kind,
            symbols: HashMap::new(),
        });
        id
    }

    /// Return the `ScopeKind` of the given scope.
    pub fn scope_kind(&self, scope: ScopeId) -> ScopeKind {
        self.scopes[scope.0 as usize].kind
    }

    /// Return the parent of `scope`, or `None` for the root.
    pub fn parent_scope(&self, scope: ScopeId) -> Option<ScopeId> {
        self.scopes[scope.0 as usize].parent
    }

    // -----------------------------------------------------------------------
    // Symbol management
    // -----------------------------------------------------------------------

    /// Declare a new symbol in `scope`.
    ///
    /// Returns `Err(SemaError::DuplicateDefinition)` if a symbol with the same
    /// name already exists in **this exact scope** (shadowing a parent is fine).
    pub fn define(
        &mut self,
        scope: ScopeId,
        name: &str,
        kind: SymbolKind,
        ty: Ty,
        span: Span,
    ) -> Result<SymbolId, SemaError> {
        // Check for duplicate in the same scope only.
        if let Some(&existing_id) = self.scopes[scope.0 as usize].symbols.get(name) {
            let first = self.symbols[existing_id.0 as usize].span;
            return Err(SemaError::DuplicateDefinition {
                name: name.to_owned(),
                first,
                second: span,
            });
        }

        let mutable = matches!(kind, SymbolKind::Signal | SymbolKind::Var | SymbolKind::LoopVar);

        let sym_id = SymbolId(self.symbols.len() as u32);
        self.symbols.push(Symbol {
            name: name.to_owned(),
            kind,
            ty,
            span,
            scope,
            mutable,
            direction: None,
        });
        self.scopes[scope.0 as usize]
            .symbols
            .insert(name.to_owned(), sym_id);
        Ok(sym_id)
    }

    // -----------------------------------------------------------------------
    // Lookup
    // -----------------------------------------------------------------------

    /// Look up `name` starting in `scope` and walking up through parent scopes.
    ///
    /// Returns the innermost (closest) matching symbol, enabling child scopes
    /// to shadow parent declarations.
    pub fn lookup(&self, scope: ScopeId, name: &str) -> Option<&Symbol> {
        let mut current = Some(scope);
        while let Some(id) = current {
            let s = &self.scopes[id.0 as usize];
            if let Some(&sym_id) = s.symbols.get(name) {
                return Some(&self.symbols[sym_id.0 as usize]);
            }
            current = s.parent;
        }
        None
    }

    /// Look up `name` in `scope` **only** — does not walk the parent chain.
    pub fn lookup_local(&self, scope: ScopeId, name: &str) -> Option<&Symbol> {
        let s = &self.scopes[scope.0 as usize];
        s.symbols
            .get(name)
            .map(|&id| &self.symbols[id.0 as usize])
    }

    // -----------------------------------------------------------------------
    // Direct access by id
    // -----------------------------------------------------------------------

    /// Retrieve a symbol by its id.
    ///
    /// # Panics
    /// Panics if `id` is out of range (i.e. was not returned by `define`).
    pub fn get_symbol(&self, id: SymbolId) -> &Symbol {
        &self.symbols[id.0 as usize]
    }

    /// Retrieve a mutable reference to a symbol by its id.
    ///
    /// # Panics
    /// Panics if `id` is out of range.
    pub fn get_symbol_mut(&mut self, id: SymbolId) -> &mut Symbol {
        &mut self.symbols[id.0 as usize]
    }

    // -----------------------------------------------------------------------
    // Iteration helpers
    // -----------------------------------------------------------------------

    /// Return all symbols declared **directly** in `scope` (no parent walk).
    pub fn local_symbols(&self, scope: ScopeId) -> Vec<&Symbol> {
        let s = &self.scopes[scope.0 as usize];
        s.symbols
            .values()
            .map(|&id| &self.symbols[id.0 as usize])
            .collect()
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}
