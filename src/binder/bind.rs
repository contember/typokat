//! AST → scope graph + multi-slot symbols (architecture §4).
//! Declares value/type names, keeps separate `DeclId` spaces, and records scopes
//! keyed by `(module scope, span start)` for the checker's reserve-then-fill pass.
//! The checker owns type construction and semantic diagnostics.

use crate::binder::scope::{Scope, ScopeGraph, ScopeId, ScopeKind};
use crate::binder::symbol::{DeclId, Symbol, SymbolId, SymbolTable};
use oxc_ast::ast::{
    ArrowFunctionExpression, BindingPattern, BlockStatement, Class, ClassElement, Declaration,
    Expression, FormalParameters, Function, FunctionBody, Program, Statement, SwitchStatement,
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
        let Some(symbol_id) = self.state.graph.resolve(scope, name) else {
            return (None, None);
        };
        self.state
            .symbols
            .get(symbol_id)
            .map(|symbol| (symbol.value, symbol.ty))
            .unwrap_or((None, None))
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
                bind_declarator(state, scope, declarator);
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
        // Other statements declare no names in the subset; their sub-expressions (if
        // any) are not in the subset either.
        _ => {}
    }
}

fn bind_declaration(state: &mut BindState, scope: ScopeId, decl: &Declaration<'_>) {
    match decl {
        Declaration::VariableDeclaration(var) => {
            for declarator in &var.declarations {
                bind_declarator(state, scope, declarator);
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
    let block_scope = state
        .graph
        .push(Scope::new(ScopeKind::Block, Some(parent)));
    state
        .block_scopes
        .insert((state.current_module, block.span.start), block_scope);
    bind_statements(state, block_scope, &block.body);
}

/// Bind a `switch`: clauses share `scope` unless an explicit block creates a
/// lexical child. Case tests are literals in the subset, but still walked.
fn bind_switch(state: &mut BindState, scope: ScopeId, switch: &SwitchStatement<'_>) {
    bind_expression(state, scope, &switch.discriminant);
    for case in &switch.cases {
        if let Some(test) = &case.test {
            bind_expression(state, scope, test);
        }
        bind_statements(state, scope, &case.consequent);
    }
}

/// Bind a variable declarator: declare its identifier (if a plain identifier) in
/// `scope`, then recurse into the initializer for nested functions.
fn bind_declarator(state: &mut BindState, scope: ScopeId, declarator: &VariableDeclarator<'_>) {
    if let Some(name) = binding_name(&declarator.id) {
        let decl_id = state.fresh_decl();
        declare_value(state, scope, name, decl_id);
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
