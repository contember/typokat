//! AST → scope graph + multi-slot symbols (architecture §4).
//! Declares value/type names, keeps separate `DeclId` spaces, and records scopes
//! keyed by `(module scope, span start)` for the checker's reserve-then-fill pass.
//! The checker owns type construction and semantic diagnostics.

use crate::binder::scope::{Scope, ScopeGraph, ScopeId, ScopeKind};
use crate::binder::symbol::{DeclId, Symbol, SymbolId, SymbolTable};
use oxc_ast::ast::{
    ArrowFunctionExpression, BindingPattern, BlockStatement, Class, ClassElement, Declaration,
    Expression, ForStatement, ForStatementInit, ForStatementLeft, FormalParameters, Function,
    FunctionBody, Program, Statement, SwitchStatement, TryStatement, VariableDeclarationKind,
    VariableDeclarator,
};
use rustc_hash::FxHashMap;

/// The binder's output for one file: the scope graph, the symbol table, the
/// module scope id, and the per-function scope map. `decl_count` is the number of
/// value declarations bound, which sizes the checker's `DeclId → TypeId` table.
pub struct Binder {
    pub graph: ScopeGraph,
    pub symbols: SymbolTable,
    /// The **user** module scope. M28: its parent is [`Binder::prelude_module`], so a
    /// user reference falls through to the prelude names and a user declaration
    /// shadows them by ordinary innermost-first resolution (no duplicate-name
    /// diagnostics — the two units are distinct scopes).
    pub module: ScopeId,
    /// The **prelude** root scope (M28) — the compilation unit holding the built-in
    /// utility aliases, bound BEFORE the user program. Its parent is `None`.
    pub prelude_module: ScopeId,
    /// Number of value declarations assigned a `DeclId` (`DeclId`s run
    /// `0..decl_count`). Includes variable bindings, function declaration names,
    /// and function parameters.
    pub decl_count: u32,
    /// Number of **type** declarations assigned a `DeclId` in the separate type
    /// numbering space. Type slots key the checker's type table, value slots key
    /// the value table, so one name can occupy both without collision (§4.1).
    pub type_decl_count: u32,
    /// Maps a function/arrow node to its parameter scope. Keyed by `(module scope,
    /// span start)` because offsets are unique only within one file (backlog 58).
    pub fn_scopes: FxHashMap<(ScopeId, u32), ScopeId>,
    /// Maps function declarations to their value declaration id.
    pub fn_decl_ids: FxHashMap<(ScopeId, u32), DeclId>,
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
    graph.resolve_matching(scope, name, |symbol_id| {
        symbols
            .get(symbol_id)
            .and_then(|symbol| symbol.value)
            .is_some()
    })
}

fn resolve_type_symbol(
    graph: &ScopeGraph,
    symbols: &SymbolTable,
    scope: ScopeId,
    name: &str,
) -> Option<SymbolId> {
    graph.resolve_matching(scope, name, |symbol_id| {
        symbols
            .get(symbol_id)
            .and_then(|symbol| symbol.ty)
            .is_some()
    })
}

/// Mutable binder state threaded through the recursive walk.
pub(crate) struct ImportedSymbol {
    name: String,
    value: Option<ImportedSlot>,
    ty: Option<ImportedSlot>,
}

impl ImportedSymbol {
    pub(crate) fn new(name: String, value: Option<DeclId>, ty: Option<DeclId>) -> Self {
        ImportedSymbol {
            name,
            value: value.map(ImportedSlot::Existing),
            ty: ty.map(ImportedSlot::Existing),
        }
    }

    pub(crate) fn placeholder_type(name: String) -> Self {
        ImportedSymbol {
            name,
            value: None,
            ty: Some(ImportedSlot::Placeholder),
        }
    }

    pub(crate) fn placeholder_value_and_type(name: String) -> Self {
        ImportedSymbol {
            name,
            value: Some(ImportedSlot::Placeholder),
            ty: Some(ImportedSlot::Placeholder),
        }
    }
}

pub(crate) enum ImportedSlot {
    Existing(DeclId),
    Placeholder,
}

pub(crate) struct ImportPlaceholder {
    pub(crate) value: Option<DeclId>,
    pub(crate) ty: Option<DeclId>,
}

struct BindState {
    graph: ScopeGraph,
    symbols: SymbolTable,
    fn_scopes: FxHashMap<(ScopeId, u32), ScopeId>,
    fn_decl_ids: FxHashMap<(ScopeId, u32), DeclId>,
    /// Per-block lexical scopes (M7), keyed by `(module scope, block span start)`.
    block_scopes: FxHashMap<(ScopeId, u32), ScopeId>,
    /// The module scope currently being bound — the disambiguating half of the
    /// scope-map keys (backlog 58). Set before each module's body is walked.
    current_module: ScopeId,
    /// Running `DeclId` counter for value declarations.
    next_decl: u32,
    /// Running `DeclId` counter for **type** declarations (separate space).
    next_type_decl: u32,
}

impl BindState {
    /// Allocate the next value-space `DeclId`.
    fn fresh_decl(&mut self) -> DeclId {
        let id = DeclId(self.next_decl);
        self.next_decl += 1;
        id
    }

    /// Allocate the next type-space `DeclId` (separate numbering space).
    fn fresh_type_decl(&mut self) -> DeclId {
        let id = DeclId(self.next_type_decl);
        self.next_type_decl += 1;
        id
    }
}

/// Build the scope graph and symbol table for the **prelude + user** pair (M28).
/// The prelude binds first and becomes the user module's parent, giving normal
/// shadowing without duplicate-name diagnostics. Each unit still declares all
/// top-level type names before bodies for the checker's reserve-then-fill pass.
pub fn bind_module_with_prelude(prelude: &Program<'_>, program: &Program<'_>) -> Binder {
    let mut builder = ProjectBinderBuilder::new(prelude);
    let (module, _) = builder.add_module(program, &[]);
    builder.finish(module)
}

/// Incremental binder for one serial project graph (M29 slice 1).
pub(crate) struct ProjectBinderBuilder {
    state: BindState,
    prelude_module: ScopeId,
}

impl ProjectBinderBuilder {
    /// Bind the prelude first so its declarations keep the low `DeclId` ranges.
    pub(crate) fn new(prelude: &Program<'_>) -> Self {
        let mut state = BindState {
            graph: ScopeGraph::new(),
            symbols: SymbolTable::new(),
            fn_scopes: FxHashMap::default(),
            fn_decl_ids: FxHashMap::default(),
            block_scopes: FxHashMap::default(),
            current_module: ScopeId(0),
            next_decl: 0,
            next_type_decl: 0,
        };

        let prelude_module = state.graph.push(Scope::new(ScopeKind::Module, None));
        state.current_module = prelude_module;
        bind_type_declarations(&mut state, prelude_module, &prelude.body);
        bind_statements(&mut state, prelude_module, &prelude.body);

        ProjectBinderBuilder {
            state,
            prelude_module,
        }
    }

    /// Add one project module. Imported symbols are declared before local names so
    /// declarations in this file can reference imports during reserve/fill.
    pub(crate) fn add_module(
        &mut self,
        program: &Program<'_>,
        imports: &[ImportedSymbol],
    ) -> (ScopeId, Vec<ImportPlaceholder>) {
        let module = self
            .state
            .graph
            .push(Scope::new(ScopeKind::Module, Some(self.prelude_module)));
        self.state.current_module = module;
        let mut placeholders = Vec::new();
        for import in imports {
            placeholders.push(declare_import(
                &mut self.state,
                module,
                &import.name,
                &import.value,
                &import.ty,
            ));
        }
        bind_type_declarations(&mut self.state, module, &program.body);
        bind_statements(&mut self.state, module, &program.body);
        (module, placeholders)
    }

    pub(crate) fn finish(self, module: ScopeId) -> Binder {
        Binder {
            graph: self.state.graph,
            symbols: self.state.symbols,
            module,
            prelude_module: self.prelude_module,
            decl_count: self.state.next_decl,
            type_decl_count: self.state.next_type_decl,
            fn_scopes: self.state.fn_scopes,
            fn_decl_ids: self.state.fn_decl_ids,
            block_scopes: self.state.block_scopes,
        }
    }

    pub(crate) fn symbol_slots(
        &self,
        scope: ScopeId,
        name: &str,
    ) -> (Option<DeclId>, Option<DeclId>) {
        let value = resolve_value_symbol(&self.state.graph, &self.state.symbols, scope, name)
            .and_then(|symbol_id| self.state.symbols.get(symbol_id))
            .and_then(|symbol| symbol.value);
        let ty = resolve_type_symbol(&self.state.graph, &self.state.symbols, scope, name)
            .and_then(|symbol_id| self.state.symbols.get(symbol_id))
            .and_then(|symbol| symbol.ty);
        (value, ty)
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
            let decl_id = state.fresh_type_decl();
            declare_type(state, scope, alias.id.name.as_str(), decl_id);
        }
        Statement::TSInterfaceDeclaration(iface) => {
            let decl_id = state.fresh_type_decl();
            declare_type(state, scope, iface.id.name.as_str(), decl_id);
        }
        // Class type-side names are reserved up front so self/sibling type references resolve.
        Statement::ClassDeclaration(class) => {
            if let Some(id) = &class.id {
                let decl_id = state.fresh_type_decl();
                declare_type(state, scope, id.name.as_str(), decl_id);
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
            let decl_id = state.fresh_type_decl();
            declare_type(state, scope, alias.id.name.as_str(), decl_id);
        }
        Declaration::TSInterfaceDeclaration(iface) => {
            let decl_id = state.fresh_type_decl();
            declare_type(state, scope, iface.id.name.as_str(), decl_id);
        }
        Declaration::ClassDeclaration(class) => {
            if let Some(id) = &class.id {
                let decl_id = state.fresh_type_decl();
                declare_type(state, scope, id.name.as_str(), decl_id);
            }
        }
        _ => {}
    }
}

/// Bind a list of statements into `scope`.
fn bind_statements(state: &mut BindState, scope: ScopeId, statements: &[Statement<'_>]) {
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
            if let Some(name) = binding_name(&param.pattern) {
                let decl_id = state.fresh_decl();
                declare_value(state, catch_scope, name, decl_id);
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
        // Case tests resolve in the switch-local scope (tsc: a test can name an
        // earlier clause's `let`; it reports only the deferred TS2454, not TS2304).
        if let Some(test) = &case.test {
            bind_expression(state, switch_scope, test);
        }
        bind_statements(state, switch_scope, &case.consequent);
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
fn bind_declarator(
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
    if let Some(name) = binding_name(&declarator.id) {
        let decl_id = state.fresh_decl();
        declare_value(state, declaration_scope, name, decl_id);
    }
    if let Some(init) = &declarator.init {
        bind_expression(state, scope, init);
    }
}

/// Bind a function declaration: declare its name (value space) in `scope`, then
/// bind the function itself (its own scope + parameters + body).
fn bind_function_declaration(state: &mut BindState, scope: ScopeId, func: &Function<'_>) {
    if let Some(id) = &func.id {
        let decl_id = state.fresh_decl();
        state
            .fn_decl_ids
            .insert((state.current_module, func.span.start), decl_id);
        declare_function_value(state, scope, id.name.as_str(), decl_id);
    }
    bind_function(state, scope, func);
}

/// Bind a class declaration: declare the constructor-side value name, then bind
/// the body. Anonymous class bodies are still walked for nested scopes.
fn bind_class_declaration(state: &mut BindState, scope: ScopeId, class: &Class<'_>) {
    if let Some(id) = &class.id {
        let decl_id = state.fresh_decl();
        declare_value(state, scope, id.name.as_str(), decl_id);
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
/// declare parameters with fresh value `DeclId`s for the checker to fill.
fn bind_function(state: &mut BindState, parent: ScopeId, func: &Function<'_>) {
    let fn_scope = state
        .graph
        .push(Scope::new(ScopeKind::Function, Some(parent)));
    state
        .fn_scopes
        .insert((state.current_module, func.span.start), fn_scope);

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
        if let Some(name) = binding_name(&param.pattern) {
            let decl_id = state.fresh_decl();
            declare_value(state, fn_scope, name, decl_id);
        }
    }
    if let Some(rest) = &params.rest {
        if let Some(name) = binding_name(&rest.rest.argument) {
            let decl_id = state.fresh_decl();
            declare_value(state, fn_scope, name, decl_id);
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

/// Declare a value-space binding `name` in `scope`, merging into an existing
/// symbol if the name is already present (so the multi-slot symbol carries the
/// value slot under the same id — architecture §4.1). Redeclaration in the same
/// space (`TK2451`) is deferred (mvp-plan); the later binding wins.
fn declare_value(state: &mut BindState, scope: ScopeId, name: &str, decl_id: DeclId) {
    if let Some(existing) = state.graph.get(scope).and_then(|s| s.lookup_local(name)) {
        if let Some(symbol) = state.symbols.get_mut(existing) {
            symbol.value = Some(decl_id);
        }
        return;
    }
    let mut symbol = Symbol::new(name);
    symbol.value = Some(decl_id);
    let symbol_id: SymbolId = state.symbols.push(symbol);
    state.graph.declare(scope, name, symbol_id);
}

fn declare_function_value(state: &mut BindState, scope: ScopeId, name: &str, decl_id: DeclId) {
    if let Some(existing) = state.graph.get(scope).and_then(|s| s.lookup_local(name)) {
        if let Some(symbol) = state.symbols.get_mut(existing) {
            symbol.value = Some(decl_id);
            symbol.function_values.push(decl_id);
        }
        return;
    }
    let mut symbol = Symbol::new(name);
    symbol.value = Some(decl_id);
    symbol.function_values.push(decl_id);
    let symbol_id: SymbolId = state.symbols.push(symbol);
    state.graph.declare(scope, name, symbol_id);
}

/// Declare a type-space binding, merging with any existing value slot under the
/// same symbol id (§4.1). Duplicate type declarations remain deferred (`TK2451`).
fn declare_type(state: &mut BindState, scope: ScopeId, name: &str, decl_id: DeclId) {
    if let Some(existing) = state.graph.get(scope).and_then(|s| s.lookup_local(name)) {
        if let Some(symbol) = state.symbols.get_mut(existing) {
            symbol.ty = Some(decl_id);
        }
        return;
    }
    let mut symbol = Symbol::new(name);
    symbol.ty = Some(decl_id);
    let symbol_id: SymbolId = state.symbols.push(symbol);
    state.graph.declare(scope, name, symbol_id);
}

fn declare_import(
    state: &mut BindState,
    scope: ScopeId,
    name: &str,
    value: &Option<ImportedSlot>,
    ty: &Option<ImportedSlot>,
) -> ImportPlaceholder {
    let (value_decl, value_placeholder) = match value {
        Some(ImportedSlot::Existing(decl_id)) => (Some(*decl_id), None),
        Some(ImportedSlot::Placeholder) => {
            let decl_id = state.fresh_decl();
            (Some(decl_id), Some(decl_id))
        }
        None => (None, None),
    };
    let (type_decl, type_placeholder) = match ty {
        Some(ImportedSlot::Existing(decl_id)) => (Some(*decl_id), None),
        Some(ImportedSlot::Placeholder) => {
            let decl_id = state.fresh_type_decl();
            (Some(decl_id), Some(decl_id))
        }
        None => (None, None),
    };
    if let Some(existing) = state.graph.get(scope).and_then(|s| s.lookup_local(name)) {
        if let Some(symbol) = state.symbols.get_mut(existing) {
            symbol.value = value_decl;
            symbol.ty = type_decl;
        }
        return ImportPlaceholder {
            value: value_placeholder,
            ty: type_placeholder,
        };
    }
    let mut symbol = Symbol::new(name);
    symbol.value = value_decl;
    symbol.ty = type_decl;
    let symbol_id: SymbolId = state.symbols.push(symbol);
    state.graph.declare(scope, name, symbol_id);
    ImportPlaceholder {
        value: value_placeholder,
        ty: type_placeholder,
    }
}

/// The bound name of a binding pattern, if it is a plain identifier. Returns
/// `None` for destructuring patterns (out of the M3 subset).
fn binding_name<'a>(pattern: &'a BindingPattern<'a>) -> Option<&'a str> {
    match pattern {
        BindingPattern::BindingIdentifier(ident) => Some(ident.name.as_str()),
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
