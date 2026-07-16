//! AST → scope graph + multi-slot symbols (architecture §4).
//! Declares value/type names, keeps lexical and storage identities separate, and records scopes
//! keyed by `(module scope, span start)` for the checker's reserve-then-fill pass.
//! The checker owns type construction and semantic diagnostics.

use crate::binder::declaration::{
    source_declaration_occurrences, DeclId, DeclarationKind, DeclarationSite, DeclarationTable,
    TypeFragmentKind, TypeGroupFragment, TypeGroupId, TypeGroupTable, ValueStorageId,
};
use crate::binder::namespace::{
    bind_namespace_metadata, CompilationUnit, NamespaceTable, SourceUnitKey,
};
use crate::binder::scope::{Scope, ScopeGraph, ScopeId, ScopeKind};
use crate::binder::symbol::{Symbol, SymbolId, SymbolTable};
use crate::span::Span;
use oxc_ast::ast::{
    ArrowFunctionExpression, BindingPattern, BlockStatement, Class, ClassElement, Declaration,
    Expression, ForStatement, ForStatementInit, ForStatementLeft, FormalParameters, Function,
    FunctionBody, FunctionType, Program, Statement, SwitchStatement, TryStatement,
    VariableDeclarationKind, VariableDeclarator,
};
use rustc_hash::FxHashMap;

/// The binder's output for one file: the scope graph, the symbol table, the
/// module scope id, source declarations, and the per-function scope map.
pub struct Binder {
    pub graph: ScopeGraph,
    pub symbols: SymbolTable,
    /// Every admitted source declaration in one unified lexical identity space.
    pub declarations: DeclarationTable,
    /// Ordered same-name type groups used by every production type-space lookup.
    pub type_groups: TypeGroupTable,
    /// Namespace/global/merge metadata and admitted attached value-member identities.
    pub namespaces: NamespaceTable,
    /// The **user** module scope. M28: its parent is [`Binder::prelude_module`], so a
    /// user reference falls through to the prelude names and a user declaration
    /// shadows them by ordinary innermost-first resolution (no duplicate-name
    /// diagnostics — the two units are distinct scopes).
    pub module: ScopeId,
    /// The **prelude** root scope (M28) — the compilation unit holding the built-in
    /// utility aliases, bound BEFORE the user program. Its parent is `None`.
    pub prelude_module: ScopeId,
    /// One empty project-wide dormant global-augmentation target.
    pub compilation_global: ScopeId,
    /// Number of value storage slots (`ValueStorageId`s run
    /// `0..decl_count`). Includes variable bindings, function declaration names,
    /// and function parameters.
    pub decl_count: u32,
    /// Number of type groups bound from the trusted prelude. User groups form the
    /// dense suffix, allowing two immutable publication epochs.
    pub prelude_type_group_count: u32,
    /// Maps a function/arrow node to its parameter scope. Keyed by `(module scope,
    /// span start)` because offsets are unique only within one file (backlog 58).
    pub fn_scopes: FxHashMap<(ScopeId, u32), ScopeId>,
    /// Maps function declarations to their value declaration id.
    pub fn_decl_ids: FxHashMap<(ScopeId, u32), ValueStorageId>,
    /// Maps a `{ … }` block to its lexical scope (M7), keyed like `fn_scopes` so
    /// branch-local declarations stay local and cross-file offsets do not collide.
    pub block_scopes: FxHashMap<(ScopeId, u32), ScopeId>,
}

impl Binder {
    /// Resolve a value-space binding, skipping same-named type-only symbols while
    /// walking parents.
    pub(crate) fn resolve_value(&self, scope: ScopeId, name: &str) -> Option<SymbolId> {
        resolve_value_symbol(&self.graph, &self.symbols, scope, name)
    }

    /// Resolve a type-space binding, skipping same-named value-only symbols while
    /// walking parents.
    pub(crate) fn resolve_type(&self, scope: ScopeId, name: &str) -> Option<SymbolId> {
        resolve_type_symbol(&self.graph, &self.symbols, scope, name)
    }
}

fn resolve_value_symbol(
    graph: &ScopeGraph,
    symbols: &SymbolTable,
    scope: ScopeId,
    name: &str,
) -> Option<SymbolId> {
    let mut current = Some(scope);
    while let Some(id) = current {
        let current_scope = graph.get(id)?;
        if let Some(symbol_id) = current_scope.lookup_local(name) {
            let symbol = symbols.get(symbol_id)?;
            if symbol.value.is_some() {
                return Some(symbol_id);
            }
            if symbol.blocks_value_lookup {
                return None;
            }
        }
        current = current_scope.parent;
    }
    None
}

fn resolve_type_symbol(
    graph: &ScopeGraph,
    symbols: &SymbolTable,
    scope: ScopeId,
    name: &str,
) -> Option<SymbolId> {
    let mut current = Some(scope);
    while let Some(id) = current {
        let current_scope = graph.get(id)?;
        if let Some(symbol_id) = current_scope.lookup_local(name) {
            let symbol = symbols.get(symbol_id)?;
            if symbol.ty.is_some() || symbol.blocks_type_lookup {
                return Some(symbol_id);
            }
        }
        current = current_scope.parent;
    }
    None
}

/// Mutable binder state threaded through the recursive walk.
pub(crate) struct ImportedSymbol {
    name: String,
    value: Option<ImportedValueSlot>,
    ty: Option<ImportedTypeSlot>,
    value_barrier: bool,
    type_barrier: bool,
    site: Span,
}

impl ImportedSymbol {
    pub(crate) fn new(
        name: String,
        value: Option<ValueStorageId>,
        ty: Option<TypeGroupId>,
        value_barrier: bool,
        type_barrier: bool,
        site: Span,
    ) -> Self {
        ImportedSymbol {
            name,
            value: value.map(ImportedValueSlot::Existing),
            ty: ty.map(ImportedTypeSlot::Existing),
            value_barrier,
            type_barrier,
            site,
        }
    }

    pub(crate) fn placeholder_type(name: String, site: Span) -> Self {
        ImportedSymbol {
            name,
            value: None,
            ty: None,
            value_barrier: false,
            type_barrier: true,
            site,
        }
    }

    pub(crate) fn placeholder_value_and_type(name: String, site: Span) -> Self {
        ImportedSymbol {
            name,
            value: Some(ImportedValueSlot::Placeholder),
            ty: None,
            value_barrier: false,
            type_barrier: true,
            site,
        }
    }
}

pub(crate) enum ImportedValueSlot {
    Existing(ValueStorageId),
    Placeholder,
}

pub(crate) enum ImportedTypeSlot {
    Existing(TypeGroupId),
}

pub(crate) struct ImportPlaceholder {
    pub(crate) value: Option<ValueStorageId>,
}

pub(crate) struct BindState {
    pub(crate) graph: ScopeGraph,
    pub(crate) symbols: SymbolTable,
    pub(crate) declarations: DeclarationTable,
    pub(crate) type_groups: TypeGroupTable,
    pub(crate) namespaces: NamespaceTable,
    pub(crate) declarations_by_site: FxHashMap<(ScopeId, u32, DeclarationKind), DeclId>,
    /// Stable source ownership for every module scope, including the prelude.
    module_sources: FxHashMap<ScopeId, SourceUnitKey>,
    fn_scopes: FxHashMap<(ScopeId, u32), ScopeId>,
    fn_decl_ids: FxHashMap<(ScopeId, u32), ValueStorageId>,
    /// Per-block lexical scopes (M7), keyed by `(module scope, block span start)`.
    block_scopes: FxHashMap<(ScopeId, u32), ScopeId>,
    /// The module scope currently being bound — the disambiguating half of the
    /// scope-map keys (backlog 58). Set before each module's body is walked.
    pub(crate) current_module: ScopeId,
    /// Running checker storage counter for value declarations.
    pub(crate) next_value_storage: u32,
}

impl BindState {
    fn record_source_declarations(&mut self, program: &Program<'_>) {
        for occurrence in source_declaration_occurrences(program) {
            let site = DeclarationSite {
                module: self.current_module,
                scope: None,
                declaration_span: occurrence.declaration_span,
                binding_span: occurrence.binding_span,
            };
            let id = self.declarations.push(occurrence.kind, site);
            let previous = self.declarations_by_site.insert(
                (
                    self.current_module,
                    occurrence.binding_span.start,
                    occurrence.kind,
                ),
                id,
            );
            debug_assert!(previous.is_none(), "one declaration per binding leaf");
        }
    }

    pub(crate) fn source_decl_at(&self, span_start: u32, kind: DeclarationKind) -> Option<DeclId> {
        self.declarations_by_site
            .get(&(self.current_module, span_start, kind))
            .copied()
    }

    pub(crate) fn attach_declaration_scope(
        &mut self,
        span_start: u32,
        kind: DeclarationKind,
        scope: ScopeId,
    ) -> DeclId {
        let declaration = self
            .source_decl_at(span_start, kind)
            .expect("semantic binding attaches to a source-prewalk declaration");
        let site = &mut self
            .declarations
            .get_mut(declaration)
            .expect("source declaration exists")
            .site;
        match site.scope {
            Some(existing) => assert_eq!(existing, scope, "declaration scope is stable"),
            None => site.scope = Some(scope),
        }
        declaration
    }

    fn attach_pattern_scope(
        &mut self,
        pattern: &BindingPattern<'_>,
        kind: DeclarationKind,
        scope: ScopeId,
    ) {
        for identifier in pattern.get_binding_identifiers() {
            self.attach_declaration_scope(identifier.span.start, kind, scope);
        }
    }

    fn fresh_value_storage(&mut self) -> ValueStorageId {
        let id = ValueStorageId(self.next_value_storage);
        self.next_value_storage += 1;
        id
    }

    fn attach_value_storage(&mut self, declaration: DeclId, storage: ValueStorageId) {
        self.declarations
            .get_mut(declaration)
            .expect("fresh lexical declaration exists")
            .value_storage = Some(storage);
    }

    pub(crate) fn attach_symbol_declaration(&mut self, symbol: SymbolId, declaration: DeclId) {
        let source_key = |id: DeclId| {
            self.declarations
                .get(id)
                .map(|declaration| {
                    (
                        self.module_sources
                            .get(&declaration.site.module)
                            .copied()
                            .unwrap_or(SourceUnitKey(u32::MAX)),
                        declaration.site.declaration_span.start,
                        declaration.site.binding_span.start,
                        declaration.id.0,
                    )
                })
                .unwrap_or((SourceUnitKey(u32::MAX), u32::MAX, u32::MAX, u32::MAX))
        };
        if let Some(row) = self.symbols.get_mut(symbol) {
            if !row.declarations.contains(&declaration) {
                row.declarations.push(declaration);
                row.declarations.sort_by_key(|id| source_key(*id));
            }
        }
    }
}

/// Build the scope graph and symbol table for the **prelude + user** pair (M28).
/// The prelude binds first and becomes the user module's parent, giving normal
/// shadowing without duplicate-name diagnostics. Each unit still declares all
/// top-level type names before bodies for the checker's reserve-then-fill pass.
pub fn bind_module_with_prelude(prelude: &Program<'_>, program: &Program<'_>) -> Binder {
    let mut builder = ProjectBinderBuilder::new(prelude);
    let unit = CompilationUnit::implementation(SourceUnitKey::SINGLE_SOURCE, program);
    let (module, _) = builder.add_module(program, &[], unit);
    builder.finish(module)
}

/// Incremental binder for one serial project graph (M29 slice 1).
pub(crate) struct ProjectBinderBuilder {
    state: BindState,
    prelude_module: ScopeId,
    compilation_global: ScopeId,
    prelude_type_group_count: u32,
}

impl ProjectBinderBuilder {
    /// Bind the prelude first so its checker storage keeps the low id ranges.
    pub(crate) fn new(prelude: &Program<'_>) -> Self {
        let mut state = BindState {
            graph: ScopeGraph::new(),
            symbols: SymbolTable::new(),
            declarations: DeclarationTable::default(),
            type_groups: TypeGroupTable::default(),
            namespaces: NamespaceTable::default(),
            declarations_by_site: FxHashMap::default(),
            module_sources: FxHashMap::default(),
            fn_scopes: FxHashMap::default(),
            fn_decl_ids: FxHashMap::default(),
            block_scopes: FxHashMap::default(),
            current_module: ScopeId(0),
            next_value_storage: 0,
        };

        let prelude_module = state.graph.push(Scope::new(ScopeKind::Module, None));
        state.current_module = prelude_module;
        state
            .module_sources
            .insert(prelude_module, SourceUnitKey::PRELUDE);
        state.record_source_declarations(prelude);
        bind_statements(&mut state, prelude_module, &prelude.body);
        let prelude_type_group_count =
            u32::try_from(state.type_groups.len()).expect("prelude type group count fits u32");
        let compilation_global = state.graph.push(Scope::new(
            ScopeKind::CompilationGlobal,
            Some(prelude_module),
        ));

        ProjectBinderBuilder {
            state,
            prelude_module,
            compilation_global,
            prelude_type_group_count,
        }
    }

    /// Add one project module. Imported symbols are declared before local names so
    /// declarations in this file can reference imports during reserve/fill.
    pub(crate) fn add_module(
        &mut self,
        program: &Program<'_>,
        imports: &[ImportedSymbol],
        unit: CompilationUnit,
    ) -> (ScopeId, Vec<ImportPlaceholder>) {
        let module = self
            .state
            .graph
            .push(Scope::new(ScopeKind::Module, Some(self.prelude_module)));
        self.state.current_module = module;
        self.state.module_sources.insert(module, unit.source);
        self.state.record_source_declarations(program);
        let mut placeholders = Vec::new();
        for import in imports {
            placeholders.push(declare_import(&mut self.state, module, import));
        }
        bind_statements(&mut self.state, module, &program.body);
        bind_namespace_metadata(
            &mut self.state,
            module,
            program,
            unit,
            self.compilation_global,
        );
        (module, placeholders)
    }

    pub(crate) fn finish(self, module: ScopeId) -> Binder {
        Binder {
            graph: self.state.graph,
            symbols: self.state.symbols,
            declarations: self.state.declarations,
            type_groups: self.state.type_groups,
            namespaces: self.state.namespaces,
            module,
            prelude_module: self.prelude_module,
            compilation_global: self.compilation_global,
            decl_count: self.state.next_value_storage,
            prelude_type_group_count: self.prelude_type_group_count,
            fn_scopes: self.state.fn_scopes,
            fn_decl_ids: self.state.fn_decl_ids,
            block_scopes: self.state.block_scopes,
        }
    }

    /// Return only slots declared directly by this module, never inherited ones.
    /// Export lists use this to avoid leaking the ambient prelude across modules.
    pub(crate) fn local_symbol_slots(
        &self,
        scope: ScopeId,
        name: &str,
    ) -> (Option<ValueStorageId>, Option<TypeGroupId>) {
        self.state
            .graph
            .get(scope)
            .and_then(|scope| scope.lookup_local(name))
            .and_then(|symbol_id| self.state.symbols.get(symbol_id))
            .map(|symbol| (symbol.value, symbol.ty))
            .unwrap_or((None, None))
    }

    /// Whether a local imported name blocks parent value lookup after its source
    /// erased a value export. Re-export lists preserve this provenance.
    pub(crate) fn local_value_lookup_barrier(&self, scope: ScopeId, name: &str) -> bool {
        self.state
            .graph
            .get(scope)
            .and_then(|scope| scope.lookup_local(name))
            .and_then(|symbol_id| self.state.symbols.get(symbol_id))
            .is_some_and(|symbol| symbol.blocks_value_lookup)
    }

    pub(crate) fn local_type_lookup_barrier(&self, scope: ScopeId, name: &str) -> bool {
        self.state
            .graph
            .get(scope)
            .and_then(|scope| scope.lookup_local(name))
            .and_then(|symbol_id| self.state.symbols.get(symbol_id))
            .is_some_and(|symbol| symbol.blocks_type_lookup)
    }
}

/// Declare top-level type names before body walks so self/sibling references
/// resolve; the checker reserves each `TypeId` and fills it later.
fn bind_type_declarations(state: &mut BindState, scope: ScopeId, statements: &[Statement<'_>]) {
    for stmt in statements {
        bind_type_declaration_statement(state, scope, stmt);
    }
}

fn bind_type_declaration_statement(state: &mut BindState, scope: ScopeId, stmt: &Statement<'_>) {
    match stmt {
        Statement::TSTypeAliasDeclaration(alias) => {
            bind_source_type(
                state,
                scope,
                alias.id.name.as_str(),
                alias.id.span.start,
                DeclarationKind::TypeAlias,
                TypeFragmentKind::TypeAlias,
            );
        }
        Statement::TSInterfaceDeclaration(iface) => {
            bind_source_type(
                state,
                scope,
                iface.id.name.as_str(),
                iface.id.span.start,
                DeclarationKind::Interface,
                TypeFragmentKind::Interface,
            );
        }
        // Class type-side names are reserved up front so self/sibling type references resolve.
        Statement::ClassDeclaration(class) => {
            if let Some(id) = &class.id {
                bind_source_type(
                    state,
                    scope,
                    id.name.as_str(),
                    id.span.start,
                    DeclarationKind::Class,
                    TypeFragmentKind::Class,
                );
            }
        }
        Statement::ExportNamedDeclaration(export) => {
            if let Some(decl) = &export.declaration {
                bind_type_declaration(state, scope, decl);
            }
        }
        _ => {}
    }
}

fn bind_type_declaration(state: &mut BindState, scope: ScopeId, decl: &Declaration<'_>) {
    match decl {
        Declaration::TSTypeAliasDeclaration(alias) => {
            bind_source_type(
                state,
                scope,
                alias.id.name.as_str(),
                alias.id.span.start,
                DeclarationKind::TypeAlias,
                TypeFragmentKind::TypeAlias,
            );
        }
        Declaration::TSInterfaceDeclaration(iface) => {
            bind_source_type(
                state,
                scope,
                iface.id.name.as_str(),
                iface.id.span.start,
                DeclarationKind::Interface,
                TypeFragmentKind::Interface,
            );
        }
        Declaration::ClassDeclaration(class) => {
            if let Some(id) = &class.id {
                bind_source_type(
                    state,
                    scope,
                    id.name.as_str(),
                    id.span.start,
                    DeclarationKind::Class,
                    TypeFragmentKind::Class,
                );
            }
        }
        _ => {}
    }
}

fn bind_source_type(
    state: &mut BindState,
    scope: ScopeId,
    name: &str,
    binding_start: u32,
    declaration_kind: DeclarationKind,
    fragment_kind: TypeFragmentKind,
) {
    let declaration = state.attach_declaration_scope(binding_start, declaration_kind, scope);
    let source = state
        .module_sources
        .get(&state.current_module)
        .copied()
        .expect("current module has stable source ownership");
    declare_type(state, scope, name, declaration, fragment_kind, source);
}

/// Bind a list of statements into `scope`.
fn bind_statements(state: &mut BindState, scope: ScopeId, statements: &[Statement<'_>]) {
    bind_type_declarations(state, scope, statements);
    for stmt in statements {
        bind_statement(state, scope, stmt);
    }
}

/// Bind one statement into `scope` (declarations) and recurse into its
/// expressions/bodies for nested functions.
fn bind_statement(state: &mut BindState, scope: ScopeId, stmt: &Statement<'_>) {
    match stmt {
        Statement::VariableDeclaration(decl) => {
            for declarator in &decl.declarations {
                bind_declarator(state, scope, decl.kind, declarator);
            }
        }
        Statement::FunctionDeclaration(func) => {
            bind_function_declaration(state, scope, func);
        }
        // Class value-side names live in the constructor slot; the body still needs scopes.
        Statement::ClassDeclaration(class) => {
            bind_class_declaration(state, scope, class);
        }
        Statement::ExportNamedDeclaration(export) => {
            if let Some(decl) = &export.declaration {
                bind_declaration(state, scope, decl);
            }
        }
        Statement::ExpressionStatement(expr_stmt) => {
            bind_expression(state, scope, &expr_stmt.expression);
        }
        Statement::ReturnStatement(ret) => {
            if let Some(arg) = &ret.argument {
                bind_expression(state, scope, arg);
            }
        }
        // Bind tests/branches so nested functions and branch-local block scopes are visible.
        Statement::IfStatement(if_stmt) => {
            bind_expression(state, scope, &if_stmt.test);
            bind_statement(state, scope, &if_stmt.consequent);
            if let Some(alternate) = &if_stmt.alternate {
                bind_statement(state, scope, alternate);
            }
        }
        // Blocks always get lexical scopes; this keeps branch-local names local.
        Statement::BlockStatement(block) => {
            bind_block(state, scope, block);
        }
        // Switch clauses share the enclosing scope unless they contain an explicit block.
        Statement::SwitchStatement(switch) => {
            bind_switch(state, scope, switch);
        }
        // Loop conditions/bodies are walked so nested functions and body-local blocks bind.
        Statement::WhileStatement(while_stmt) => {
            bind_expression(state, scope, &while_stmt.test);
            bind_statement(state, scope, &while_stmt.body);
        }
        // A `do … while` has no head binding; walk the body and the condition.
        Statement::DoWhileStatement(do_stmt) => {
            bind_statement(state, scope, &do_stmt.body);
            bind_expression(state, scope, &do_stmt.test);
        }
        // C-style `for (init; test; update) body` — the init declaration lives in a
        // per-loop head scope shared by the test/update/body.
        Statement::ForStatement(for_stmt) => bind_for(state, scope, for_stmt),
        // `for-in`/`for-of` — the iteration variable lives in a per-loop head scope; the
        // source is evaluated in the enclosing scope.
        Statement::ForInStatement(for_in) => bind_for_in_of(
            state,
            scope,
            &for_in.left,
            &for_in.right,
            &for_in.body,
            for_in.span.start,
        ),
        Statement::ForOfStatement(for_of) => bind_for_in_of(
            state,
            scope,
            &for_of.left,
            &for_of.right,
            &for_of.body,
            for_of.span.start,
        ),
        // `label: <stmt>` — the label is not a binding; descend into the body so a
        // labeled loop gets its head scope and a labeled block binds normally.
        Statement::LabeledStatement(labeled) => {
            bind_statement(state, scope, &labeled.body);
        }
        // `try`/`catch`/`finally` — each block gets its own lexical scope so the
        // checker walks it (WU4); the catch parameter is declared in a dedicated
        // catch scope so references resolve (its type is left to the checker).
        Statement::TryStatement(try_stmt) => bind_try(state, scope, try_stmt),
        // Other statements declare no names in the subset; their sub-expressions (if
        // any) are not in the subset either.
        _ => {}
    }
}

/// Bind a `try`/`catch`/`finally`. The try and finally blocks bind like ordinary
/// blocks. The catch clause gets a dedicated block scope holding the caught
/// parameter (so references inside the handler resolve), with the handler body
/// nested inside it as its own block.
fn bind_try(state: &mut BindState, parent: ScopeId, try_stmt: &TryStatement<'_>) {
    bind_block(state, parent, &try_stmt.block);
    if let Some(handler) = &try_stmt.handler {
        let catch_scope = state.graph.push(Scope::new(ScopeKind::Block, Some(parent)));
        state
            .block_scopes
            .insert((state.current_module, handler.span.start), catch_scope);
        if let Some(param) = &handler.param {
            state.attach_pattern_scope(
                &param.pattern,
                DeclarationKind::CatchParameter,
                catch_scope,
            );
            if let Some((name, binding_start)) = binding_name_and_start(&param.pattern) {
                let (declaration, storage) = bind_source_value(
                    state,
                    catch_scope,
                    name,
                    binding_start,
                    DeclarationKind::CatchParameter,
                );
                declare_value(state, catch_scope, name, storage, declaration);
            }
        }
        bind_block(state, catch_scope, &handler.body);
    }
    if let Some(finalizer) = &try_stmt.finalizer {
        bind_block(state, parent, finalizer);
    }
}

fn bind_declaration(state: &mut BindState, scope: ScopeId, decl: &Declaration<'_>) {
    match decl {
        Declaration::VariableDeclaration(var) => {
            for declarator in &var.declarations {
                bind_declarator(state, scope, var.kind, declarator);
            }
        }
        Declaration::FunctionDeclaration(func) => {
            bind_function_declaration(state, scope, func);
        }
        Declaration::ClassDeclaration(class) => {
            bind_class_declaration(state, scope, class);
        }
        _ => {}
    }
}

/// Bind a `{ … }` block into its own [`ScopeKind::Block`] child scope under
/// `parent`, recording it under `(module scope, block span start)` so the checker
/// descends into the matching scope. The block's statements are bound inside it.
fn bind_block(state: &mut BindState, parent: ScopeId, block: &BlockStatement<'_>) {
    let block_scope = state.graph.push(Scope::new(ScopeKind::Block, Some(parent)));
    state
        .block_scopes
        .insert((state.current_module, block.span.start), block_scope);
    bind_statements(state, block_scope, &block.body);
}

/// Bind a `switch`: the whole case block is ONE lexical scope (per ECMAScript the
/// CaseBlock is a single block environment), keyed by the switch span so the
/// checker can enter it. The discriminant is evaluated in the enclosing scope
/// (before the case block); every clause's test and consequent binds into the
/// shared switch-local scope, so a block-scoped declaration in a case does not
/// leak past the switch, yet remains visible across clauses. Explicit nested
/// `{ }` blocks inside a clause still create their own child scope via `bind_block`.
fn bind_switch(state: &mut BindState, scope: ScopeId, switch: &SwitchStatement<'_>) {
    bind_expression(state, scope, &switch.discriminant);
    let switch_scope = state.graph.push(Scope::new(ScopeKind::Block, Some(scope)));
    state
        .block_scopes
        .insert((state.current_module, switch.span.start), switch_scope);
    for case in &switch.cases {
        bind_type_declarations(state, switch_scope, &case.consequent);
    }
    for case in &switch.cases {
        // Case tests resolve in the switch-local scope (tsc: a test can name an
        // earlier clause's `let`; it reports only the deferred TS2454, not TS2304).
        if let Some(test) = &case.test {
            bind_expression(state, switch_scope, test);
        }
        for statement in &case.consequent {
            bind_statement(state, switch_scope, statement);
        }
    }
}

/// Bind a C-style `for` head into a fresh [`ScopeKind::Block`] head scope (keyed by
/// the loop statement's span start, like [`bind_block`]) so a `for (let i…)`
/// initializer is scoped to the loop, then bind the test/update/body inside it.
fn bind_for(state: &mut BindState, parent: ScopeId, for_stmt: &ForStatement<'_>) {
    let head = state.graph.push(Scope::new(ScopeKind::Block, Some(parent)));
    state
        .block_scopes
        .insert((state.current_module, for_stmt.span.start), head);
    if let Some(init) = &for_stmt.init {
        match init {
            ForStatementInit::VariableDeclaration(decl) => {
                for declarator in &decl.declarations {
                    bind_declarator(state, head, decl.kind, declarator);
                }
            }
            other => {
                if let Some(expr) = other.as_expression() {
                    bind_expression(state, head, expr);
                }
            }
        }
    }
    if let Some(test) = &for_stmt.test {
        bind_expression(state, head, test);
    }
    if let Some(update) = &for_stmt.update {
        bind_expression(state, head, update);
    }
    bind_statement(state, head, &for_stmt.body);
}

/// Bind a `for-in`/`for-of` head: a fresh head scope holds the iteration variable,
/// the source is bound in the enclosing scope (it is evaluated there), and the body
/// is bound inside the head scope.
fn bind_for_in_of(
    state: &mut BindState,
    parent: ScopeId,
    left: &ForStatementLeft<'_>,
    right: &Expression<'_>,
    body: &Statement<'_>,
    span_start: u32,
) {
    let head = state.graph.push(Scope::new(ScopeKind::Block, Some(parent)));
    state
        .block_scopes
        .insert((state.current_module, span_start), head);
    bind_expression(state, parent, right);
    if let ForStatementLeft::VariableDeclaration(decl) = left {
        for declarator in &decl.declarations {
            bind_declarator(state, head, decl.kind, declarator);
        }
    }
    bind_statement(state, head, body);
}

/// Bind a variable declarator: a `var` name targets its nearest function/module,
/// while the initializer remains in the original lexical scope.
pub(super) fn bind_declarator(
    state: &mut BindState,
    scope: ScopeId,
    kind: VariableDeclarationKind,
    declarator: &VariableDeclarator<'_>,
) {
    let declaration_scope = if kind.is_var() {
        state.graph.var_scope(scope).unwrap_or(scope)
    } else {
        scope
    };
    state.attach_pattern_scope(&declarator.id, DeclarationKind::Variable, declaration_scope);
    if let Some((name, binding_start)) = binding_name_and_start(&declarator.id) {
        let (declaration, storage) = bind_source_value(
            state,
            declaration_scope,
            name,
            binding_start,
            DeclarationKind::Variable,
        );
        declare_value(state, declaration_scope, name, storage, declaration);
    }
    if let Some(init) = &declarator.init {
        bind_expression(state, scope, init);
    }
}

/// Bind a function declaration: declare its name (value space) in `scope`, then
/// bind the function itself (its own scope + parameters + body).
pub(super) fn bind_function_declaration(
    state: &mut BindState,
    scope: ScopeId,
    func: &Function<'_>,
) {
    if let Some(id) = &func.id {
        let (declaration, storage) = bind_source_value(
            state,
            scope,
            id.name.as_str(),
            id.span.start,
            DeclarationKind::Function,
        );
        state
            .fn_decl_ids
            .insert((state.current_module, func.span.start), storage);
        declare_function_value(state, scope, id.name.as_str(), storage, declaration);
    }
    bind_function(state, scope, func);
}

/// Bind a class declaration: declare the constructor-side value name, then bind
/// the body. Anonymous class bodies are still walked for nested scopes.
pub(super) fn bind_class_declaration(state: &mut BindState, scope: ScopeId, class: &Class<'_>) {
    if let Some(id) = &class.id {
        let declaration =
            state.attach_declaration_scope(id.span.start, DeclarationKind::Class, scope);
        let storage = state.fresh_value_storage();
        state.attach_value_storage(declaration, storage);
        declare_value(state, scope, id.name.as_str(), storage, declaration);
    }
    bind_class(state, scope, class);
}

/// Bind class-body scopes. The checker owns `extends`/`super`, abstract flags,
/// accessor merging, parameter properties, and deferred `implements` handling.
fn bind_class(state: &mut BindState, parent: ScopeId, class: &Class<'_>) {
    for element in &class.body.body {
        match element {
            // Method-like elements need a function scope even when the body is absent.
            ClassElement::MethodDefinition(method) => {
                bind_function(state, parent, &method.value);
            }
            // A field: walk its initializer for nested functions (the field's type
            // itself is an annotation, which holds no value bindings).
            ClassElement::PropertyDefinition(prop) => {
                if let Some(init) = &prop.value {
                    bind_expression(state, parent, init);
                }
            }
            // Static blocks, accessor properties, and index signatures are out of
            // the M11 subset — no value bindings.
            _ => {}
        }
    }
}

/// Bind a function/arrow scope, record it by `(module scope, span start)`, and
/// declare parameters with fresh value-storage ids for the checker to fill.
fn bind_function(state: &mut BindState, parent: ScopeId, func: &Function<'_>) {
    let fn_scope = state
        .graph
        .push(Scope::new(ScopeKind::Function, Some(parent)));
    state
        .fn_scopes
        .insert((state.current_module, func.span.start), fn_scope);
    if matches!(
        func.r#type,
        FunctionType::FunctionExpression | FunctionType::TSEmptyBodyFunctionExpression
    ) {
        if let Some(id) = &func.id {
            state.attach_declaration_scope(id.span.start, DeclarationKind::Function, fn_scope);
        }
    }

    bind_parameters(state, fn_scope, &func.params);

    for param in &func.params.items {
        if let Some(init) = &param.initializer {
            bind_expression(state, fn_scope, init);
        }
    }
    if let Some(body) = &func.body {
        bind_function_body(state, fn_scope, body);
    }
}

/// Bind an arrow's own scope, mirroring [`bind_function`]. An arrow always has a
/// body (an expression body or a block); the body is bound inside the arrow's
/// function scope.
fn bind_arrow(state: &mut BindState, parent: ScopeId, arrow: &ArrowFunctionExpression<'_>) {
    let fn_scope = state
        .graph
        .push(Scope::new(ScopeKind::Function, Some(parent)));
    state
        .fn_scopes
        .insert((state.current_module, arrow.span.start), fn_scope);

    bind_parameters(state, fn_scope, &arrow.params);

    for param in &arrow.params.items {
        if let Some(init) = &param.initializer {
            bind_expression(state, fn_scope, init);
        }
    }
    bind_function_body(state, fn_scope, &arrow.body);
}

fn bind_parameters(state: &mut BindState, fn_scope: ScopeId, params: &FormalParameters<'_>) {
    for param in &params.items {
        state.attach_pattern_scope(&param.pattern, DeclarationKind::Parameter, fn_scope);
        if let Some((name, binding_start)) = binding_name_and_start(&param.pattern) {
            let (declaration, storage) = bind_source_value(
                state,
                fn_scope,
                name,
                binding_start,
                DeclarationKind::Parameter,
            );
            declare_value(state, fn_scope, name, storage, declaration);
        }
    }
    if let Some(rest) = &params.rest {
        state.attach_pattern_scope(&rest.rest.argument, DeclarationKind::Parameter, fn_scope);
        if let Some((name, binding_start)) = binding_name_and_start(&rest.rest.argument) {
            let (declaration, storage) = bind_source_value(
                state,
                fn_scope,
                name,
                binding_start,
                DeclarationKind::Parameter,
            );
            declare_value(state, fn_scope, name, storage, declaration);
        }
    }
}

/// Bind a function body's statements into the function scope. An expression-body
/// arrow is parsed as a block holding a single `return <expr>`, so walking the
/// statements covers both forms.
fn bind_function_body(state: &mut BindState, fn_scope: ScopeId, body: &FunctionBody<'_>) {
    bind_statements(state, fn_scope, &body.statements);
}

/// Recurse into expression shapes that can contain nested scopes or initializers.
fn bind_expression(state: &mut BindState, scope: ScopeId, expr: &Expression<'_>) {
    match expr {
        Expression::FunctionExpression(func) => bind_function(state, scope, func),
        Expression::ArrowFunctionExpression(arrow) => bind_arrow(state, scope, arrow),
        // Class expressions still need method scopes even when their instance type is unnamed.
        Expression::ClassExpression(class) => bind_class(state, scope, class),
        // M11: `new C(args)` — bind the callee and each argument for nested
        // functions, mirroring the call-expression arm.
        Expression::NewExpression(new_expr) => {
            bind_expression(state, scope, &new_expr.callee);
            for arg in &new_expr.arguments {
                if let Some(arg_expr) = arg.as_expression() {
                    bind_expression(state, scope, arg_expr);
                }
            }
        }
        Expression::CallExpression(call) => {
            bind_expression(state, scope, &call.callee);
            for arg in &call.arguments {
                if let Some(arg_expr) = arg.as_expression() {
                    bind_expression(state, scope, arg_expr);
                }
            }
        }
        Expression::AssignmentExpression(assign) => {
            bind_expression(state, scope, &assign.right);
        }
        Expression::StaticMemberExpression(member) => {
            bind_expression(state, scope, &member.object);
        }
        Expression::ObjectExpression(obj) => {
            for member in &obj.properties {
                if let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(prop) = member {
                    bind_expression(state, scope, &prop.value);
                }
            }
        }
        Expression::ArrayExpression(array) => {
            for element in &array.elements {
                if let Some(expression) = element.as_expression() {
                    bind_expression(state, scope, expression);
                }
            }
        }
        Expression::ParenthesizedExpression(paren) => {
            bind_expression(state, scope, &paren.expression);
        }
        Expression::TSAsExpression(assertion) => {
            bind_expression(state, scope, &assertion.expression);
        }
        Expression::TSTypeAssertion(assertion) => {
            bind_expression(state, scope, &assertion.expression);
        }
        // Literals, identifiers, and other expression shapes hold no nested
        // function in the M3 subset.
        _ => {}
    }
}

fn bind_source_value(
    state: &mut BindState,
    scope: ScopeId,
    _name: &str,
    binding_start: u32,
    kind: DeclarationKind,
) -> (DeclId, ValueStorageId) {
    let declaration = state.attach_declaration_scope(binding_start, kind, scope);
    let storage = state.fresh_value_storage();
    state.attach_value_storage(declaration, storage);
    (declaration, storage)
}

/// Declare a value-space binding `name` in `scope`, merging into an existing
/// symbol if the name is already present (so the multi-slot symbol carries the
/// value slot under the same id — architecture §4.1). Redeclaration in the same
/// space (`TK2451`) is deferred (mvp-plan); the later binding wins.
fn declare_value(
    state: &mut BindState,
    scope: ScopeId,
    name: &str,
    storage: ValueStorageId,
    declaration: DeclId,
) {
    if let Some(existing) = state.graph.get(scope).and_then(|s| s.lookup_local(name)) {
        if let Some(symbol) = state.symbols.get_mut(existing) {
            symbol.value = Some(storage);
        }
        state.attach_symbol_declaration(existing, declaration);
        return;
    }
    let mut symbol = Symbol::new(name);
    symbol.value = Some(storage);
    let symbol_id: SymbolId = state.symbols.push(symbol);
    state.graph.declare(scope, name, symbol_id);
    state.attach_symbol_declaration(symbol_id, declaration);
}

fn declare_function_value(
    state: &mut BindState,
    scope: ScopeId,
    name: &str,
    storage: ValueStorageId,
    declaration: DeclId,
) {
    if let Some(existing) = state.graph.get(scope).and_then(|s| s.lookup_local(name)) {
        if let Some(symbol) = state.symbols.get_mut(existing) {
            symbol.value = Some(storage);
            symbol.function_values.push(storage);
        }
        state.attach_symbol_declaration(existing, declaration);
        return;
    }
    let mut symbol = Symbol::new(name);
    symbol.value = Some(storage);
    symbol.function_values.push(storage);
    let symbol_id: SymbolId = state.symbols.push(symbol);
    state.graph.declare(scope, name, symbol_id);
    state.attach_symbol_declaration(symbol_id, declaration);
}

/// Retain one source fragment in its stable production type group.
pub(super) fn declare_type(
    state: &mut BindState,
    target_scope: ScopeId,
    name: &str,
    declaration: DeclId,
    kind: TypeFragmentKind,
    source: SourceUnitKey,
) {
    let site = state
        .declarations
        .get(declaration)
        .expect("fresh type declaration exists")
        .site;
    let fragment_scope = site.scope.expect("type declaration has a lexical scope");
    if let Some(existing) = state
        .graph
        .get(target_scope)
        .and_then(|s| s.lookup_local(name))
    {
        let group = match state
            .symbols
            .get(existing)
            .and_then(|symbol| symbol.owns_type_group.then_some(symbol.ty).flatten())
        {
            Some(group) => group,
            None => state.type_groups.push(name),
        };
        state
            .type_groups
            .get_mut(group)
            .expect("allocated type group exists")
            .fragments
            .push(TypeGroupFragment {
                declaration,
                source,
                scope: fragment_scope,
                site,
                kind,
            });
        state
            .type_groups
            .get_mut(group)
            .expect("allocated type group exists")
            .fragments
            .sort_by_key(|fragment| {
                (
                    fragment.source,
                    fragment.site.declaration_span.start,
                    fragment.declaration.0,
                )
            });
        let lexical = state
            .declarations
            .get_mut(declaration)
            .expect("fresh type declaration exists");
        lexical.type_group = Some(group);
        let symbol = state
            .symbols
            .get_mut(existing)
            .expect("resolved symbol exists");
        symbol.ty = Some(group);
        symbol.owns_type_group = true;
        symbol.blocks_type_lookup = false;
        state.attach_symbol_declaration(existing, declaration);
        return;
    }
    let group = state.type_groups.push(name);
    state
        .type_groups
        .get_mut(group)
        .expect("allocated type group exists")
        .fragments
        .push(TypeGroupFragment {
            declaration,
            source,
            scope: fragment_scope,
            site,
            kind,
        });
    let lexical = state
        .declarations
        .get_mut(declaration)
        .expect("fresh type declaration exists");
    lexical.type_group = Some(group);
    let mut symbol = Symbol::new(name);
    symbol.ty = Some(group);
    symbol.owns_type_group = true;
    let symbol_id: SymbolId = state.symbols.push(symbol);
    state.graph.declare(target_scope, name, symbol_id);
    state.attach_symbol_declaration(symbol_id, declaration);
}

fn declare_import(
    state: &mut BindState,
    scope: ScopeId,
    import: &ImportedSymbol,
) -> ImportPlaceholder {
    let (value_decl, value_placeholder) = match &import.value {
        Some(ImportedValueSlot::Existing(storage)) => (Some(*storage), None),
        Some(ImportedValueSlot::Placeholder) => {
            let storage = state.fresh_value_storage();
            (Some(storage), Some(storage))
        }
        None => (None, None),
    };
    let type_group = import
        .ty
        .as_ref()
        .map(|ImportedTypeSlot::Existing(group)| *group);
    let declaration =
        state.attach_declaration_scope(import.site.start, DeclarationKind::Import, scope);
    let lexical = state
        .declarations
        .get_mut(declaration)
        .expect("fresh import declaration exists");
    lexical.value_storage = value_decl;
    lexical.type_group = type_group;
    if let Some(existing) = state
        .graph
        .get(scope)
        .and_then(|s| s.lookup_local(&import.name))
    {
        if let Some(symbol) = state.symbols.get_mut(existing) {
            symbol.value = value_decl;
            symbol.ty = type_group;
            symbol.owns_type_group = false;
            symbol.blocks_value_lookup = import.value_barrier;
            symbol.blocks_type_lookup = import.type_barrier;
        }
        state.attach_symbol_declaration(existing, declaration);
        return ImportPlaceholder {
            value: value_placeholder,
        };
    }
    let mut symbol = Symbol::new(&import.name);
    symbol.value = value_decl;
    symbol.ty = type_group;
    symbol.owns_type_group = false;
    symbol.blocks_value_lookup = import.value_barrier;
    symbol.blocks_type_lookup = import.type_barrier;
    let symbol_id: SymbolId = state.symbols.push(symbol);
    state.graph.declare(scope, &import.name, symbol_id);
    state.attach_symbol_declaration(symbol_id, declaration);
    ImportPlaceholder {
        value: value_placeholder,
    }
}

/// The bound name of a binding pattern, if it is a plain identifier. Returns
/// `None` for destructuring patterns (out of the M3 subset).
fn binding_name_and_start<'a>(pattern: &'a BindingPattern<'a>) -> Option<(&'a str, u32)> {
    match pattern {
        BindingPattern::BindingIdentifier(ident) => Some((ident.name.as_str(), ident.span.start)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    fn bind(src: &str) -> Binder {
        let prelude_alloc = Allocator::default();
        let alloc = Allocator::default();
        let prelude = Parser::new(&prelude_alloc, "", SourceType::ts()).parse();
        let parsed = Parser::new(&alloc, src, SourceType::ts()).parse();
        assert!(!parsed.panicked, "parse failed: {src}");
        bind_module_with_prelude(&prelude.program, &parsed.program)
    }

    #[test]
    fn lexical_declarations_and_storage_identities_do_not_alias() {
        fn lexical(_: DeclId) {}
        fn value_storage(_: ValueStorageId) {}
        fn type_group(_: crate::binder::declaration::TypeGroupId) {}

        let source = "const value = 0; function f(param: number, ...rest: string[]) { try {} catch (caught) {} } type Alias = number; interface Shape {} class Both {}";
        let binder = bind(source);
        assert_eq!(binder.declarations.len(), 8);

        let declarations: Vec<_> = binder.declarations.iter().collect();
        for (index, declaration) in declarations.iter().enumerate() {
            assert_eq!(declaration.id.index(), index);
            assert_eq!(declaration.site.module, binder.module);
            assert!(
                declaration.site.declaration_span.start < declaration.site.declaration_span.end
            );
            assert!(declaration.site.binding_span.start < declaration.site.binding_span.end);
            lexical(declaration.id);
        }
        assert!(declarations.iter().all(|declaration| {
            (declaration.value_storage.is_none() && declaration.type_group.is_none())
                || declaration.site.scope.is_some()
        }));

        let value = declarations
            .iter()
            .find(|declaration| declaration.kind == DeclarationKind::Variable)
            .expect("variable declaration");
        assert_eq!(&source[value.site.declaration_span.range()], "value = 0");
        assert_eq!(&source[value.site.binding_span.range()], "value");
        value_storage(value.value_storage.expect("variable value storage"));

        let parameters: Vec<_> = declarations
            .iter()
            .filter(|declaration| declaration.kind == DeclarationKind::Parameter)
            .collect();
        assert_eq!(parameters.len(), 2);
        assert_eq!(
            parameters
                .iter()
                .map(|declaration| &source[declaration.site.binding_span.range()])
                .collect::<Vec<_>>(),
            vec!["param", "rest"]
        );

        let caught = declarations
            .iter()
            .find(|declaration| declaration.kind == DeclarationKind::CatchParameter)
            .expect("catch declaration");
        assert_eq!(&source[caught.site.declaration_span.range()], "caught");
        assert_eq!(&source[caught.site.binding_span.range()], "caught");

        for kind in [
            DeclarationKind::TypeAlias,
            DeclarationKind::Interface,
            DeclarationKind::Class,
        ] {
            let declaration = declarations
                .iter()
                .find(|declaration| declaration.kind == kind)
                .expect("type declaration");
            type_group(declaration.type_group.expect("type group identity"));
        }

        let class = declarations
            .iter()
            .find(|declaration| declaration.kind == DeclarationKind::Class)
            .expect("class declaration");
        value_storage(class.value_storage.expect("class value storage"));
        type_group(class.type_group.expect("class type group identity"));
        let class_symbol = binder
            .graph
            .get(binder.module)
            .and_then(|scope| scope.lookup_local("Both"))
            .and_then(|symbol| binder.symbols.get(symbol))
            .expect("class symbol");
        assert_eq!(class_symbol.declarations, vec![class.id]);
    }

    #[test]
    fn type_groups_retain_every_fragment_in_source_order_behind_legacy_boundary() {
        let source = "export interface M { first: number } export class M {} export interface M { last: string }";
        let binder = bind(source);
        let symbol_id = binder
            .graph
            .get(binder.module)
            .and_then(|scope| scope.lookup_local("M"))
            .expect("merged symbol");
        let symbol = binder.symbols.get(symbol_id).expect("merged symbol row");
        let group_id = symbol.ty.expect("type group");
        let group = binder.type_groups.get(group_id).expect("type group row");

        assert_eq!(group.name, "M");
        assert_eq!(
            group
                .fragments
                .iter()
                .map(|fragment| fragment.kind)
                .collect::<Vec<_>>(),
            vec![
                TypeFragmentKind::Interface,
                TypeFragmentKind::Class,
                TypeFragmentKind::Interface,
            ]
        );
        assert!(group
            .fragments
            .windows(2)
            .all(|pair| pair[0].site.declaration_span.start < pair[1].site.declaration_span.start));
        assert!(group.fragments.iter().all(|fragment| {
            binder
                .declarations
                .get(fragment.declaration)
                .is_some_and(|declaration| {
                    declaration.site == fragment.site
                        && declaration.type_group == Some(group_id)
                        && fragment.scope == declaration.site.scope.expect("bound type scope")
                })
        }));

        assert!(group.fragments.iter().all(
            |fragment| fragment.site.declaration_span.start < fragment.site.binding_span.start
        ));
        assert_eq!(
            group
                .fragments
                .iter()
                .map(|fragment| &source[fragment.site.binding_span.range()])
                .collect::<Vec<_>>(),
            vec!["M", "M", "M"]
        );

        assert_eq!(symbol.ty, Some(group_id));

        let class = group
            .fragments
            .iter()
            .find(|fragment| fragment.kind == TypeFragmentKind::Class)
            .and_then(|fragment| binder.declarations.get(fragment.declaration))
            .expect("class fragment declaration");
        assert!(class.value_storage.is_some());
    }

    #[test]
    fn source_prewalk_records_imports_and_every_nested_binding_leaf() {
        let source = "import Default, * as NS from 'pkg'; import type { Remote as Local } from './dep'; const { a, nested: { b = 1 }, ...objectRest } = value; const [c, , [d], ...arrayRest] = value; function f({ p: [q] }, [r, ...s], t = 1) {} try {} catch ({ e: [caught], ...catchRest }) {}";
        let binder = bind(source);
        let declarations: Vec<_> = binder
            .declarations
            .iter()
            .filter(|declaration| declaration.site.module == binder.module)
            .collect();

        let binding_names: Vec<_> = declarations
            .iter()
            .map(|declaration| &source[declaration.site.binding_span.range()])
            .collect();
        assert_eq!(
            binding_names,
            vec![
                "Default",
                "NS",
                "Local",
                "a",
                "b",
                "objectRest",
                "c",
                "d",
                "arrayRest",
                "f",
                "q",
                "r",
                "s",
                "t",
                "caught",
                "catchRest",
            ]
        );

        let imports: Vec<_> = declarations
            .iter()
            .filter(|declaration| declaration.kind == DeclarationKind::Import)
            .collect();
        assert_eq!(imports.len(), 3);
        assert_eq!(
            imports
                .iter()
                .map(|declaration| &source[declaration.site.declaration_span.range()])
                .collect::<Vec<_>>(),
            vec![
                "import Default, * as NS from 'pkg';",
                "import Default, * as NS from 'pkg';",
                "import type { Remote as Local } from './dep';",
            ]
        );
        assert!(imports.iter().all(|declaration| {
            declaration.value_storage.is_none() && declaration.type_group.is_none()
        }));

        let a = declarations
            .iter()
            .find(|declaration| source[declaration.site.binding_span.range()] == *"a")
            .expect("nested object leaf");
        let b = declarations
            .iter()
            .find(|declaration| source[declaration.site.binding_span.range()] == *"b")
            .expect("nested assignment leaf");
        assert_eq!(a.site.declaration_span, b.site.declaration_span);
        assert_eq!(
            &source[a.site.declaration_span.range()],
            "{ a, nested: { b = 1 }, ...objectRest } = value"
        );
        assert!(a.value_storage.is_none());
        assert!(b.value_storage.is_none());

        let supported_parameter = declarations
            .iter()
            .find(|declaration| source[declaration.site.binding_span.range()] == *"t")
            .expect("simple parameter");
        assert!(supported_parameter.value_storage.is_some());
        for name in ["q", "r", "s", "caught", "catchRest"] {
            let declaration = declarations
                .iter()
                .find(|declaration| source[declaration.site.binding_span.range()] == *name)
                .expect("destructured binding leaf");
            assert!(declaration.value_storage.is_none());
        }
    }

    #[test]
    fn semantic_walk_attaches_truthful_scopes_without_fabricating_unsupported_ones() {
        let source = "const { top, nested: [topNested] } = value; { let [blockLeaf] = value; type BlockType = number; function nested({ paramLeaf }, ...restParam) { try {} catch ({ caughtLeaf }) {} } } namespace Unsupported { export const hidden = 1; } export {}; declare global { interface GlobalShape {} }";
        let binder = bind(source);
        let outer_block_start = u32::try_from(source.find("{ let").unwrap()).unwrap();
        let function_start = u32::try_from(source.find("function nested").unwrap()).unwrap();
        let catch_start = u32::try_from(source.find("catch").unwrap()).unwrap();
        let block_scope = binder
            .block_scopes
            .get(&(binder.module, outer_block_start))
            .copied()
            .expect("outer block scope");
        let function_scope = binder
            .fn_scopes
            .get(&(binder.module, function_start))
            .copied()
            .expect("nested function scope");
        let catch_scope = binder
            .block_scopes
            .get(&(binder.module, catch_start))
            .copied()
            .expect("catch scope");

        let declaration = |name: &str| {
            binder
                .declarations
                .iter()
                .find(|declaration| &source[declaration.site.binding_span.range()] == name)
                .expect("source declaration")
        };
        for name in ["top", "topNested"] {
            assert_eq!(declaration(name).site.scope, Some(binder.module));
        }
        for name in ["blockLeaf", "BlockType", "nested"] {
            assert_eq!(declaration(name).site.scope, Some(block_scope));
        }
        for name in ["paramLeaf", "restParam"] {
            assert_eq!(declaration(name).site.scope, Some(function_scope));
        }
        assert_eq!(declaration("caughtLeaf").site.scope, Some(catch_scope));

        let namespace = declaration("Unsupported")
            .namespace
            .and_then(|namespace| binder.namespaces.get(namespace))
            .expect("dormant namespace metadata");
        let fragment = namespace
            .fragments
            .first()
            .and_then(|fragment| binder.namespaces.fragment(*fragment))
            .expect("namespace fragment");
        assert_eq!(declaration("Unsupported").site.scope, Some(binder.module));
        assert_eq!(
            declaration("hidden").site.scope,
            Some(fragment.private_scope)
        );
        assert_eq!(declaration("global").site.scope, Some(binder.module));
        assert_eq!(
            declaration("GlobalShape").site.scope,
            Some(binder.compilation_global)
        );
        assert_eq!(declaration("Unsupported").kind, DeclarationKind::Namespace);
        assert_eq!(declaration("global").kind, DeclarationKind::Global);
    }

    /// WU2: every `case`/`default` clause binds into ONE switch-local lexical
    /// scope, and that scope does not leak into the enclosing function.
    #[test]
    fn switch_clauses_share_one_switch_local_scope() {
        let binder = bind(
            "function f(x: number) { \
               switch (x) { \
                 case 1: let a = 1; break; \
                 case 2: let b = 2; break; \
               } \
             }",
        );

        // The switch introduces exactly one block scope (no explicit `{ }` blocks
        // in this fixture), shared by both clauses.
        assert_eq!(binder.block_scopes.len(), 1, "one switch-local scope");
        let switch_scope = *binder.block_scopes.values().next().unwrap();
        let scope = binder.graph.get(switch_scope).unwrap();
        assert_eq!(scope.kind, ScopeKind::Block);

        // Both clause-local `let`s live directly in that same scope, as distinct
        // symbols — proving the clauses share ONE ScopeId.
        let a = scope.lookup_local("a").expect("a in switch scope");
        let b = scope.lookup_local("b").expect("b in switch scope");
        assert_ne!(a, b);

        // The switch-local names do not leak up to the enclosing function scope.
        let parent = binder.graph.get(scope.parent.unwrap()).unwrap();
        assert_eq!(parent.kind, ScopeKind::Function);
        assert!(parent.lookup_local("a").is_none());
        assert!(parent.lookup_local("b").is_none());
    }

    /// An explicit `{ }` block inside a clause still gets its own nested scope,
    /// child of the switch-local scope — its declarations do not reach the switch.
    #[test]
    fn explicit_block_in_clause_keeps_its_own_scope() {
        let binder = bind(
            "function f(x: number) { \
               switch (x) { \
                 case 1: { let inner = 1; } break; \
               } \
             }",
        );

        // Two block scopes: the switch-local one and the explicit `{ }` inside it.
        assert_eq!(binder.block_scopes.len(), 2);
        let inner_scope = binder
            .block_scopes
            .values()
            .find(|id| {
                binder
                    .graph
                    .get(**id)
                    .unwrap()
                    .lookup_local("inner")
                    .is_some()
            })
            .copied()
            .expect("inner block scope");
        // Its parent is a switch-local block scope, and `inner` is not in the switch.
        let parent = binder.graph.get(inner_scope).unwrap().parent.unwrap();
        assert_eq!(binder.graph.get(parent).unwrap().kind, ScopeKind::Block);
        assert!(binder
            .graph
            .get(parent)
            .unwrap()
            .lookup_local("inner")
            .is_none());
    }

    #[test]
    fn var_bindings_target_the_nearest_function_or_module_scope() {
        let binder = bind(
            "{ var module_var = 1; let module_let = 1; } \
             function outer() { \
               if (true) { var from_if = 1; } \
               for (var from_for = 0; false;) {} \
               for (var from_in in { key: 1 }) {} \
               for (var from_of of [1]) {} \
               while (false) { var from_while = 1; } \
               switch (1) { case 1: var from_switch = 1; break; } \
               { let block_let = 1; const block_const = 2; } \
               function inner() { { var inner_only = 1; } } \
             }",
        );

        let module_scope = binder.graph.get(binder.module).expect("module scope");
        assert!(module_scope.lookup_local("module_var").is_some());
        assert!(module_scope.lookup_local("module_let").is_none());

        let outer_scope = binder
            .fn_scopes
            .values()
            .copied()
            .find(|scope| {
                binder
                    .graph
                    .get(*scope)
                    .is_some_and(|scope| scope.lookup_local("from_if").is_some())
            })
            .expect("outer function scope");
        let outer = binder.graph.get(outer_scope).expect("outer scope");
        for name in [
            "from_if",
            "from_for",
            "from_in",
            "from_of",
            "from_while",
            "from_switch",
        ] {
            assert!(outer.lookup_local(name).is_some(), "{name} in outer scope");
        }
        assert!(outer.lookup_local("block_let").is_none());
        assert!(outer.lookup_local("block_const").is_none());
        assert!(outer.lookup_local("inner_only").is_none());

        let inner_scope = binder
            .fn_scopes
            .values()
            .copied()
            .find(|scope| {
                binder
                    .graph
                    .get(*scope)
                    .is_some_and(|scope| scope.lookup_local("inner_only").is_some())
            })
            .expect("inner function scope");
        assert_ne!(outer_scope, inner_scope);
    }
}
