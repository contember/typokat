//! decls module (extracted from checker/mod.rs).

use super::context::*;
use crate::binder::scope::ScopeId;
use crate::binder::symbol::DeclId;
use crate::binder::Binder;
use crate::span::Span;
use crate::types::repr::{ClassId, TypeParamId, TypeTag};
use crate::types::store::TypeId;
use crate::types::Interner;
use oxc_ast::ast::{
    Class, ClassElement, Declaration, Expression, ForStatementInit, ForStatementLeft, Function,
    ObjectPropertyKind, Program, Statement, TSInterfaceDeclaration, TSInterfaceHeritage,
    TSTypeAliasDeclaration, TSTypeParameterDeclaration,
};

mod interface;
mod params;
mod resolve;

impl<'a, 'ast> Pass<'a, 'ast> {
    fn with_type_decl_effects<R>(
        &mut self,
        decl_id: DeclId,
        produce: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let owner = self
            .lexical_events
            .type_decl_owner(decl_id)
            .expect("type declaration must have a preallocated lexical owner");
        self.with_ticket_effects(owner.ticket, produce)
    }

    pub(super) fn resolve_type_decl(&mut self, scope: ScopeId, decl_id: DeclId) -> TypeId {
        if self
            .type_resolved
            .get(decl_id.index())
            .copied()
            .flatten()
            .is_some()
        {
            return self.resolve_type_decl_inner(scope, decl_id);
        }
        self.with_type_decl_effects(decl_id, |pass| pass.resolve_type_decl_inner(scope, decl_id))
    }

    pub(super) fn emit_type_decl_diagnostic(
        &mut self,
        decl_id: DeclId,
        diagnostic: crate::diagnostics::Diagnostic,
    ) {
        self.with_type_decl_effects(decl_id, |pass| pass.emit_diagnostic(diagnostic));
    }

    /// Fill named type declarations so later annotation reads are plain id lookups.
    ///
    /// Interfaces fill before aliases because alias instantiation must substitute over
    /// an already-filled generic interface template; reverse dependencies stay lazy.
    pub(in crate::check::checker) fn fill_type_decls(&mut self, scope: ScopeId) {
        self.fill_type_decls_range(scope, 0, self.type_decls.len());
    }

    pub(in crate::check::checker) fn fill_type_decls_range(
        &mut self,
        scope: ScopeId,
        start: usize,
        end: usize,
    ) {
        // Template lowering keeps conditionals lazy until value-position demand.
        self.building_template = true;

        // Fill interfaces first so generic interface templates are ready to instantiate.
        for index in start..end {
            self.ensure_interface_filled(scope, index);
        }

        // Fill conditional-alias placeholders before ordinary aliases can instantiate them.
        for index in start..end {
            let (scope, placeholder, params, param_decl, annotation, name, name_span) =
                match &self.type_decls[index] {
                    TypeDecl::Alias {
                        scope,
                        conditional_template: Some(placeholder),
                        params,
                        param_decl,
                        annotation,
                        name,
                        name_span,
                        ..
                    } => (
                        *scope,
                        *placeholder,
                        params.clone(),
                        *param_decl,
                        *annotation,
                        name.clone(),
                        *name_span,
                    ),
                    _ => continue,
                };
            let decl_id = DeclId(index as u32);
            let frame = self.build_type_param_frame(param_decl, &params);
            self.resolving_conditional_alias = Some((decl_id, name_span, name));
            let lowered = self.with_type_decl_effects(decl_id, |pass| {
                pass.with_type_params(frame, |pass| {
                    pass.lower_type_param_constraints(scope, param_decl, &params);
                    pass.lower_annotation(scope, annotation)
                })
            });
            self.resolving_conditional_alias = None;

            let error_ty = self.interner.well_known().error;
            match lowered {
                Some(id) if self.interner.store().tag(id) == TypeTag::Conditional => {
                    // Copy the freshly-lowered conditional's body into the reserved
                    // template id, so self-recursive instantiations point at a filled node.
                    if let Some(cond) = self.interner.store().conditional_type(id).copied() {
                        self.interner.fill_conditional(placeholder, cond);
                    }
                }
                // Circular check (`TK2456`) or out-of-subset body → the alias is the error
                // type (silent downstream, m22 discipline).
                _ => {
                    if let Some(slot) = self.type_resolved.get_mut(index) {
                        *slot = Some(error_ty);
                    }
                }
            }
        }

        // Fill mapped-alias placeholders before ordinary aliases can instantiate them.
        for index in start..end {
            let (scope, placeholder, params, param_decl, annotation, name, name_span) =
                match &self.type_decls[index] {
                    TypeDecl::Alias {
                        scope,
                        mapped_template: Some(placeholder),
                        params,
                        param_decl,
                        annotation,
                        name,
                        name_span,
                        ..
                    } => (
                        *scope,
                        *placeholder,
                        params.clone(),
                        *param_decl,
                        *annotation,
                        name.clone(),
                        *name_span,
                    ),
                    _ => continue,
                };
            let decl_id = DeclId(index as u32);
            let frame = self.build_type_param_frame(param_decl, &params);
            let prev_resolving_alias = self.resolving_alias.take();
            self.resolving_alias = Some((decl_id, name_span, name.clone()));
            self.resolving_alias_stack.push((
                decl_id,
                name_span,
                name,
                self.alias_indirection_depth,
            ));
            let lowered = self.with_type_decl_effects(decl_id, |pass| {
                pass.with_type_params(frame, |pass| {
                    pass.lower_type_param_constraints(scope, param_decl, &params);
                    pass.lower_annotation(scope, annotation)
                })
            });
            self.resolving_alias_stack.pop();
            self.resolving_alias = prev_resolving_alias;

            let error_ty = self.interner.well_known().error;
            match lowered {
                Some(id) if self.interner.store().tag(id) == TypeTag::Mapped => {
                    // Copy the freshly-lowered mapped body into the reserved template
                    // id, so self-recursive instantiations point at a filled node.
                    if let Some(mapped) = self.interner.store().mapped_type(id).copied() {
                        self.interner.fill_mapped(placeholder, mapped);
                    }
                }
                // Circular key source (`TK2456`) or out-of-subset body → the alias is
                // the error type (silent downstream, M22 discipline; overwrites the
                // seeded reserved id).
                _ => {
                    if let Some(slot) = self.type_resolved.get_mut(index) {
                        *slot = Some(error_ty);
                    }
                }
            }
        }

        // Fill seeded object aliases so legal member recursion resolves to the reserved id.
        for index in start..end {
            self.ensure_object_alias_filled(scope, index);
        }

        // Touch remaining aliases to resolve the whole memoized DAG.
        for index in start..end {
            if matches!(self.type_decls[index], TypeDecl::Alias { .. }) {
                self.resolve_type_decl(scope, DeclId(index as u32));
            }
        }

        // Value-position annotations may evaluate conditionals after fill.
        self.building_template = false;
    }

    pub(in crate::check::checker) fn fill_pending_interfaces_range(
        &mut self,
        scope: ScopeId,
        start: usize,
        end: usize,
    ) {
        let building_template = std::mem::replace(&mut self.building_template, true);
        for index in start..end {
            self.ensure_interface_filled(scope, index);
        }
        self.building_template = building_template;
    }

    /// Fill one interface's reserved object with its composed type.
    /// Bases are filled first in any declaration order; `Filling` breaks out-of-scope
    /// `extends` cycles without a diagnostic.
    fn ensure_interface_filled(&mut self, scope: ScopeId, index: usize) {
        if !matches!(self.template_fill.get(index), Some(ClassFillState::Pending)) {
            return;
        }
        let decl_id = DeclId(u32::try_from(index).expect("type declaration index fits u32"));
        self.with_type_decl_effects(decl_id, |pass| {
            pass.ensure_interface_filled_inner(scope, index)
        });
    }

    fn ensure_interface_filled_inner(&mut self, _scope: ScopeId, index: usize) {
        match self.template_fill.get(index).copied() {
            // Already filled, or filling (an `extends` cycle re-entered) — do not recurse.
            Some(ClassFillState::Done) | Some(ClassFillState::Filling) => return,
            Some(ClassFillState::Pending) => {}
            // Out of range: nothing to fill.
            None => return,
        }
        let TypeDecl::Interface {
            scope: declaration_scope,
            reserved,
            ref params,
            param_decl,
            members,
            extends,
        } = self.type_decls[index]
        else {
            // Not an interface (a Pending object-template alias belongs to
            // [`ensure_object_alias_filled`]) — leave the state untouched.
            return;
        };
        let scope = declaration_scope;
        let params = params.clone();

        if !self.class_publication_complete
            && extends
                .iter()
                .any(|heritage| self.heritage_resolves_to_class(scope, heritage))
        {
            return;
        }

        // Mark in-progress before touching a base, so a cyclic `extends` hits the
        // `Filling` guard above rather than looping.
        if let Some(slot) = self.template_fill.get_mut(index) {
            *slot = ClassFillState::Filling;
        }

        // Fill each base first — interface, class, or (through a transparent alias) a
        // seeded object-literal alias — outside this interface's type-param frame, so a
        // base's parameters never leak into (or capture) this interface's frame.
        for heritage in extends {
            self.ensure_heritage_base_filled(scope, heritage);
        }

        let frame = self.build_type_param_frame(param_decl, &params);
        let object = self.with_type_params(frame, |pass| {
            // M24: lower the parameters' `extends` constraints with the frame active.
            pass.lower_type_param_constraints(scope, param_decl, &params);
            let own = pass.lower_interface_members(scope, members);
            pass.compose_interface_heritage(scope, own, extends)
        });
        self.interner.fill_object(reserved, object);

        if let Some(slot) = self.template_fill.get_mut(index) {
            *slot = ClassFillState::Done;
        }
    }

    fn heritage_resolves_to_class(
        &self,
        scope: ScopeId,
        heritage: &TSInterfaceHeritage<'_>,
    ) -> bool {
        let Expression::Identifier(ident) = &heritage.expression else {
            return false;
        };
        type_decl_id(self.binder, scope, ident.name.as_str()).is_some_and(|decl_id| {
            matches!(
                self.type_decls.get(decl_id.index()),
                Some(TypeDecl::Class { .. })
            )
        })
    }

    /// Fill one seeded object-literal alias's reserved object with lowered members.
    /// Runs on demand in `template_fill`; `resolving_alias` stays set so nested
    /// mapped self-references still report `TK2456`.
    fn ensure_object_alias_filled(&mut self, scope: ScopeId, index: usize) {
        if !matches!(self.template_fill.get(index), Some(ClassFillState::Pending)) {
            return;
        }
        let decl_id = DeclId(u32::try_from(index).expect("type declaration index fits u32"));
        self.with_type_decl_effects(decl_id, |pass| {
            pass.ensure_object_alias_filled_inner(scope, index)
        });
    }

    fn ensure_object_alias_filled_inner(&mut self, _scope: ScopeId, index: usize) {
        match self.template_fill.get(index).copied() {
            Some(ClassFillState::Done) | Some(ClassFillState::Filling) => return,
            Some(ClassFillState::Pending) => {}
            None => return,
        }
        let (scope, reserved, members, name, name_span) = match &self.type_decls[index] {
            TypeDecl::Alias {
                scope,
                object_template: Some(reserved),
                annotation: oxc_ast::ast::TSType::TSTypeLiteral(lit),
                name,
                name_span,
                ..
            } => (*scope, *reserved, &lit.members, name.clone(), *name_span),
            // Not a seeded object alias (a Pending interface belongs to
            // [`ensure_interface_filled`]) — leave the state untouched.
            _ => return,
        };
        if let Some(slot) = self.template_fill.get_mut(index) {
            *slot = ClassFillState::Filling;
        }
        let decl_id = DeclId(index as u32);
        let prev_resolving_alias = self.resolving_alias.take();
        self.resolving_alias = Some((decl_id, name_span, name.clone()));
        self.resolving_alias_stack
            .push((decl_id, name_span, name, self.alias_indirection_depth));
        let object = self.lower_interface_members(scope, members);
        self.resolving_alias_stack.pop();
        self.resolving_alias = prev_resolving_alias;
        self.interner.fill_object(reserved, object);
        if let Some(slot) = self.template_fill.get_mut(index) {
            *slot = ClassFillState::Done;
        }
    }

    /// Force-fill a heritage base before composition reads its members.
    /// Interfaces recurse, while aliases resolve then fill any reserved template
    /// they land on. Generic alias heritage resolves on demand at instantiation time.
    fn ensure_heritage_base_filled(&mut self, scope: ScopeId, heritage: &TSInterfaceHeritage<'_>) {
        let Expression::Identifier(ident) = &heritage.expression else {
            return;
        };
        let Some(decl_id) = type_decl_id(self.binder, scope, ident.name.as_str()) else {
            return;
        };
        match self.type_decls.get(decl_id.index()) {
            Some(TypeDecl::Interface { .. }) => {
                self.ensure_interface_filled(scope, decl_id.index())
            }
            Some(TypeDecl::Class { .. }) => {}
            Some(TypeDecl::Alias { .. }) if heritage.type_arguments.is_none() => {
                let ty = self.resolve_type_decl(scope, decl_id);
                self.ensure_reserved_template_filled(scope, ty);
            }
            _ => {}
        }
    }

    /// Fill the reserved-template declaration owned by a resolved base `TypeId`, if any.
    /// Transparent alias chains then compose the filled target in any declaration order.
    fn ensure_reserved_template_filled(&mut self, scope: ScopeId, ty: TypeId) {
        let target = self.type_decls.iter().position(|decl| match decl {
            TypeDecl::Interface { reserved, .. } => *reserved == ty,
            TypeDecl::Alias {
                object_template: Some(reserved),
                ..
            } => *reserved == ty,
            _ => false,
        });
        let Some(index) = target else {
            return;
        };
        match self.type_decls.get(index) {
            Some(TypeDecl::Interface { .. }) => self.ensure_interface_filled(scope, index),
            Some(TypeDecl::Alias { .. }) => self.ensure_object_alias_filled(scope, index),
            _ => {}
        }
    }
}

/// Reserve top-level type declarations by type-space `DeclId`.
/// Interfaces get ids before bodies resolve, enabling self/sibling references.
/// Reserve runs per compilation unit; append order matches binder `DeclId` order so
/// prelude and user declarations stay index-aligned.
#[allow(clippy::too_many_arguments)] // Two counters + two appended tables — irreducible reserve state.
pub(in crate::check::checker) fn reserve_type_decls<'ast>(
    interner: &mut Interner,
    binder: &Binder,
    scope: ScopeId,
    program: &'ast Program<'ast>,
    next_type_param: &mut u32,
    next_class_id: &mut u32,
    decls: &mut Vec<TypeDecl<'ast>>,
    resolved: &mut [Option<TypeId>],
) {
    // The binder and this walker use the same scope maps and source order, keeping
    // every nested declaration aligned with its type-space `DeclId`.
    walk_type_decls(binder, scope, program, &mut |scope, _, declaration| {
        match declaration {
            TopTypeDecl::Interface(iface) => {
                let reserved = interner.reserve_object();
                let decl_id = type_decl_id(binder, scope, iface.id.name.as_str());
                if let Some(decl_id) = decl_id {
                    if let Some(slot) = resolved.get_mut(decl_id.index()) {
                        *slot = Some(reserved);
                    }
                    // M9: allocate one id per declared type parameter (in source order).
                    let params =
                        alloc_type_param_ids(iface.type_parameters.as_deref(), next_type_param);
                    place_type_decl(
                        decls,
                        decl_id.index(),
                        TypeDecl::Interface {
                            scope,
                            reserved,
                            params,
                            param_decl: iface.type_parameters.as_deref(),
                            members: &iface.body.body,
                            extends: &iface.extends,
                        },
                    );
                }
            }
            TopTypeDecl::Alias(alias) => {
                let decl_id = type_decl_id(binder, scope, alias.id.name.as_str());
                let params =
                    alloc_type_param_ids(alias.type_parameters.as_deref(), next_type_param);
                // M25: a top-level conditional-type body reserves a conditional template
                // id and seeds `type_resolved`, so a self-recursive reference resolves to
                // it (as a lazy instantiation) rather than expanding at lowering. The
                // placeholder is filled in the fill step.
                let conditional_template = if matches!(
                    alias.type_annotation,
                    oxc_ast::ast::TSType::TSConditionalType(_)
                ) {
                    let reserved = interner.reserve_conditional();
                    // M28 round 3: name the reserved row so a deferred
                    // instantiation renders by alias NAME, not the raw body.
                    interner.set_template_name(reserved, alias.id.name.as_str());
                    if let Some(decl_id) = decl_id {
                        if let Some(slot) = resolved.get_mut(decl_id.index()) {
                            *slot = Some(reserved);
                        }
                    }
                    Some(reserved)
                } else {
                    None
                };
                // Top-level mapped aliases reserve a template id so self-recursive
                // references resolve as lazy instantiations, not error types.
                let mapped_template =
                    if matches!(alias.type_annotation, oxc_ast::ast::TSType::TSMappedType(_)) {
                        let reserved = interner.reserve_mapped();
                        // M28 round 3: named for rendering, like the conditional row.
                        interner.set_template_name(reserved, alias.id.name.as_str());
                        if let Some(decl_id) = decl_id {
                            if let Some(slot) = resolved.get_mut(decl_id.index()) {
                                *slot = Some(reserved);
                            }
                        }
                        Some(reserved)
                    } else {
                        None
                    };
                // Non-generic object-literal aliases reserve an object id so member
                // self-references are legal recursion. Generic object aliases remain
                // structural templates instantiated by substitution, so they are not seeded.
                let object_template = if alias.type_parameters.is_none()
                    && matches!(
                        alias.type_annotation,
                        oxc_ast::ast::TSType::TSTypeLiteral(_)
                    ) {
                    let reserved = interner.reserve_object();
                    if let Some(decl_id) = decl_id {
                        if let Some(slot) = resolved.get_mut(decl_id.index()) {
                            *slot = Some(reserved);
                        }
                    }
                    Some(reserved)
                } else {
                    None
                };
                if let Some(decl_id) = decl_id {
                    place_type_decl(
                        decls,
                        decl_id.index(),
                        TypeDecl::Alias {
                            scope,
                            annotation: &alias.type_annotation,
                            params,
                            param_decl: alias.type_parameters.as_deref(),
                            resolving: false,
                            conditional_template,
                            mapped_template,
                            object_template,
                            name: alias.id.name.to_string(),
                            name_span: Span::from_oxc(alias.id.span),
                        },
                    );
                }
            }
            // Named classes reserve only a stable nominal identity. Their immutable
            // instance/static templates are constructed by class publication.
            TopTypeDecl::Class(class) if class.id.is_some() => {
                // M13: a fresh stable `ClassId` for this declaration (source order),
                // stamped onto its members during class publication.
                let class_id = ClassId(*next_class_id);
                *next_class_id += 1;
                if let Some(id) = &class.id {
                    let decl_id = type_decl_id(binder, scope, id.name.as_str());
                    if let Some(decl_id) = decl_id {
                        // M16: allocate one id per declared type parameter (in source order),
                        // paired with their names later when the class body is lowered with the
                        // parameter frame in scope — exactly like an interface.
                        let params =
                            alloc_type_param_ids(class.type_parameters.as_deref(), next_type_param);
                        place_type_decl(
                            decls,
                            decl_id.index(),
                            TypeDecl::Class {
                                scope,
                                class_id,
                                params,
                                param_decl: class.type_parameters.as_deref(),
                                class,
                            },
                        );
                    }
                }
            }
            _ => {}
        }
    });
}

fn place_type_decl<'ast>(decls: &mut Vec<TypeDecl<'ast>>, index: usize, decl: TypeDecl<'ast>) {
    while decls.len() < index {
        decls.push(TypeDecl::Resolved { params: Vec::new() });
    }
    if decls.len() == index {
        decls.push(decl);
    } else if let Some(slot) = decls.get_mut(index) {
        *slot = decl;
    }
}

#[derive(Copy, Clone)]
pub(in crate::check::checker) enum TopTypeDecl<'ast> {
    Interface(&'ast TSInterfaceDeclaration<'ast>),
    Alias(&'ast TSTypeAliasDeclaration<'ast>),
    Class(&'ast Class<'ast>),
}

/// Visit every named type declaration with the exact lexical scope allocated by
/// the binder. The walk mirrors binder scope entry and never creates a scope.
pub(in crate::check::checker) fn walk_type_decls<'ast>(
    binder: &Binder,
    module: ScopeId,
    program: &'ast Program<'ast>,
    visit: &mut impl FnMut(ScopeId, u32, TopTypeDecl<'ast>),
) {
    walk_type_decl_statements(binder, module, module, &program.body, visit);
}

fn walk_type_decl_statements<'ast>(
    binder: &Binder,
    module: ScopeId,
    scope: ScopeId,
    statements: &'ast [Statement<'ast>],
    visit: &mut impl FnMut(ScopeId, u32, TopTypeDecl<'ast>),
) {
    for statement in statements {
        walk_type_decl_statement(binder, module, scope, statement, visit);
    }
}

fn walk_type_decl_statement<'ast>(
    binder: &Binder,
    module: ScopeId,
    scope: ScopeId,
    statement: &'ast Statement<'ast>,
    visit: &mut impl FnMut(ScopeId, u32, TopTypeDecl<'ast>),
) {
    match statement {
        Statement::TSInterfaceDeclaration(interface) => {
            visit(
                scope,
                interface.span.start,
                TopTypeDecl::Interface(interface),
            );
        }
        Statement::TSTypeAliasDeclaration(alias) => {
            visit(scope, alias.span.start, TopTypeDecl::Alias(alias));
        }
        Statement::ClassDeclaration(class) => {
            visit(scope, class.span.start, TopTypeDecl::Class(class));
            walk_type_decl_class(binder, module, scope, class, visit);
        }
        Statement::FunctionDeclaration(function) => {
            walk_type_decl_function(binder, module, function, visit);
        }
        Statement::ExportNamedDeclaration(export) => {
            let Some(declaration) = &export.declaration else {
                return;
            };
            match declaration {
                Declaration::TSInterfaceDeclaration(interface) => {
                    visit(scope, export.span.start, TopTypeDecl::Interface(interface))
                }
                Declaration::TSTypeAliasDeclaration(alias) => {
                    visit(scope, export.span.start, TopTypeDecl::Alias(alias));
                }
                Declaration::ClassDeclaration(class) => {
                    visit(scope, export.span.start, TopTypeDecl::Class(class));
                    walk_type_decl_class(binder, module, scope, class, visit);
                }
                Declaration::FunctionDeclaration(function) => {
                    walk_type_decl_function(binder, module, function, visit);
                }
                Declaration::VariableDeclaration(declaration) => {
                    walk_type_decl_variable(binder, module, scope, declaration, visit);
                }
                _ => {}
            }
        }
        Statement::VariableDeclaration(declaration) => {
            walk_type_decl_variable(binder, module, scope, declaration, visit);
        }
        Statement::ExpressionStatement(statement) => {
            walk_type_decl_expression(binder, module, scope, &statement.expression, visit);
        }
        Statement::ReturnStatement(statement) => {
            if let Some(argument) = &statement.argument {
                walk_type_decl_expression(binder, module, scope, argument, visit);
            }
        }
        Statement::ThrowStatement(statement) => {
            walk_type_decl_expression(binder, module, scope, &statement.argument, visit);
        }
        Statement::IfStatement(statement) => {
            walk_type_decl_expression(binder, module, scope, &statement.test, visit);
            walk_type_decl_statement(binder, module, scope, &statement.consequent, visit);
            if let Some(alternate) = &statement.alternate {
                walk_type_decl_statement(binder, module, scope, alternate, visit);
            }
        }
        Statement::BlockStatement(block) => {
            walk_type_decl_block(binder, module, scope, block, visit);
        }
        Statement::SwitchStatement(statement) => {
            walk_type_decl_expression(binder, module, scope, &statement.discriminant, visit);
            let Some(&switch_scope) = binder.block_scopes.get(&(module, statement.span.start))
            else {
                return;
            };
            for case in &statement.cases {
                if let Some(test) = &case.test {
                    walk_type_decl_expression(binder, module, switch_scope, test, visit);
                }
                walk_type_decl_statements(binder, module, switch_scope, &case.consequent, visit);
            }
        }
        Statement::WhileStatement(statement) => {
            walk_type_decl_expression(binder, module, scope, &statement.test, visit);
            walk_type_decl_statement(binder, module, scope, &statement.body, visit);
        }
        Statement::DoWhileStatement(statement) => {
            walk_type_decl_statement(binder, module, scope, &statement.body, visit);
            walk_type_decl_expression(binder, module, scope, &statement.test, visit);
        }
        Statement::ForStatement(statement) => {
            let Some(&loop_scope) = binder.block_scopes.get(&(module, statement.span.start)) else {
                return;
            };
            if let Some(init) = &statement.init {
                match init {
                    ForStatementInit::VariableDeclaration(declaration) => {
                        walk_type_decl_variable(binder, module, loop_scope, declaration, visit);
                    }
                    other => {
                        if let Some(expression) = other.as_expression() {
                            walk_type_decl_expression(
                                binder, module, loop_scope, expression, visit,
                            );
                        }
                    }
                }
            }
            if let Some(test) = &statement.test {
                walk_type_decl_expression(binder, module, loop_scope, test, visit);
            }
            if let Some(update) = &statement.update {
                walk_type_decl_expression(binder, module, loop_scope, update, visit);
            }
            walk_type_decl_statement(binder, module, loop_scope, &statement.body, visit);
        }
        Statement::ForInStatement(statement) => {
            walk_type_decl_for_in_of(
                binder,
                module,
                scope,
                &statement.left,
                &statement.right,
                &statement.body,
                statement.span.start,
                visit,
            );
        }
        Statement::ForOfStatement(statement) => {
            walk_type_decl_for_in_of(
                binder,
                module,
                scope,
                &statement.left,
                &statement.right,
                &statement.body,
                statement.span.start,
                visit,
            );
        }
        Statement::LabeledStatement(statement) => {
            walk_type_decl_statement(binder, module, scope, &statement.body, visit);
        }
        Statement::TryStatement(statement) => {
            walk_type_decl_block(binder, module, scope, &statement.block, visit);
            if let Some(handler) = &statement.handler {
                let Some(&catch_scope) = binder.block_scopes.get(&(module, handler.span.start))
                else {
                    return;
                };
                walk_type_decl_block(binder, module, catch_scope, &handler.body, visit);
            }
            if let Some(finalizer) = &statement.finalizer {
                walk_type_decl_block(binder, module, scope, finalizer, visit);
            }
        }
        _ => {}
    }
}

fn walk_type_decl_block<'ast>(
    binder: &Binder,
    module: ScopeId,
    parent: ScopeId,
    block: &'ast oxc_ast::ast::BlockStatement<'ast>,
    visit: &mut impl FnMut(ScopeId, u32, TopTypeDecl<'ast>),
) {
    let Some(&scope) = binder.block_scopes.get(&(module, block.span.start)) else {
        return;
    };
    debug_assert_eq!(
        binder.graph.get(scope).and_then(|scope| scope.parent),
        Some(parent)
    );
    walk_type_decl_statements(binder, module, scope, &block.body, visit);
}

#[allow(clippy::too_many_arguments)]
fn walk_type_decl_for_in_of<'ast>(
    binder: &Binder,
    module: ScopeId,
    parent: ScopeId,
    left: &'ast ForStatementLeft<'ast>,
    right: &'ast Expression<'ast>,
    body: &'ast Statement<'ast>,
    span_start: u32,
    visit: &mut impl FnMut(ScopeId, u32, TopTypeDecl<'ast>),
) {
    walk_type_decl_expression(binder, module, parent, right, visit);
    let Some(&scope) = binder.block_scopes.get(&(module, span_start)) else {
        return;
    };
    if let ForStatementLeft::VariableDeclaration(declaration) = left {
        walk_type_decl_variable(binder, module, scope, declaration, visit);
    }
    walk_type_decl_statement(binder, module, scope, body, visit);
}

fn walk_type_decl_variable<'ast>(
    binder: &Binder,
    module: ScopeId,
    scope: ScopeId,
    declaration: &'ast oxc_ast::ast::VariableDeclaration<'ast>,
    visit: &mut impl FnMut(ScopeId, u32, TopTypeDecl<'ast>),
) {
    for declarator in &declaration.declarations {
        if let Some(initializer) = &declarator.init {
            walk_type_decl_expression(binder, module, scope, initializer, visit);
        }
    }
}

fn walk_type_decl_function<'ast>(
    binder: &Binder,
    module: ScopeId,
    function: &'ast Function<'ast>,
    visit: &mut impl FnMut(ScopeId, u32, TopTypeDecl<'ast>),
) {
    let Some(&scope) = binder.fn_scopes.get(&(module, function.span.start)) else {
        return;
    };
    for parameter in &function.params.items {
        if let Some(initializer) = &parameter.initializer {
            walk_type_decl_expression(binder, module, scope, initializer, visit);
        }
    }
    if let Some(body) = &function.body {
        walk_type_decl_statements(binder, module, scope, &body.statements, visit);
    }
}

fn walk_type_decl_class<'ast>(
    binder: &Binder,
    module: ScopeId,
    scope: ScopeId,
    class: &'ast Class<'ast>,
    visit: &mut impl FnMut(ScopeId, u32, TopTypeDecl<'ast>),
) {
    for element in &class.body.body {
        match element {
            ClassElement::MethodDefinition(method) => {
                walk_type_decl_function(binder, module, &method.value, visit);
            }
            ClassElement::PropertyDefinition(property) => {
                if let Some(initializer) = &property.value {
                    walk_type_decl_expression(binder, module, scope, initializer, visit);
                }
            }
            _ => {}
        }
    }
}

fn walk_type_decl_expression<'ast>(
    binder: &Binder,
    module: ScopeId,
    scope: ScopeId,
    expression: &'ast Expression<'ast>,
    visit: &mut impl FnMut(ScopeId, u32, TopTypeDecl<'ast>),
) {
    match expression {
        Expression::FunctionExpression(function) => {
            walk_type_decl_function(binder, module, function, visit);
        }
        Expression::ArrowFunctionExpression(arrow) => {
            let Some(&scope) = binder.fn_scopes.get(&(module, arrow.span.start)) else {
                return;
            };
            for parameter in &arrow.params.items {
                if let Some(initializer) = &parameter.initializer {
                    walk_type_decl_expression(binder, module, scope, initializer, visit);
                }
            }
            walk_type_decl_statements(binder, module, scope, &arrow.body.statements, visit);
        }
        Expression::ClassExpression(class) => {
            walk_type_decl_class(binder, module, scope, class, visit);
        }
        Expression::NewExpression(new_expression) => {
            walk_type_decl_expression(binder, module, scope, &new_expression.callee, visit);
            for argument in &new_expression.arguments {
                if let Some(argument) = argument.as_expression() {
                    walk_type_decl_expression(binder, module, scope, argument, visit);
                }
            }
        }
        Expression::CallExpression(call) => {
            walk_type_decl_expression(binder, module, scope, &call.callee, visit);
            for argument in &call.arguments {
                if let Some(argument) = argument.as_expression() {
                    walk_type_decl_expression(binder, module, scope, argument, visit);
                }
            }
        }
        Expression::AssignmentExpression(assignment) => {
            walk_type_decl_expression(binder, module, scope, &assignment.right, visit);
        }
        Expression::StaticMemberExpression(member) => {
            walk_type_decl_expression(binder, module, scope, &member.object, visit);
        }
        Expression::ObjectExpression(object) => {
            for property in &object.properties {
                if let ObjectPropertyKind::ObjectProperty(property) = property {
                    walk_type_decl_expression(binder, module, scope, &property.value, visit);
                }
            }
        }
        Expression::ArrayExpression(array) => {
            for element in &array.elements {
                if let Some(element) = element.as_expression() {
                    walk_type_decl_expression(binder, module, scope, element, visit);
                }
            }
        }
        Expression::ParenthesizedExpression(parenthesized) => {
            walk_type_decl_expression(binder, module, scope, &parenthesized.expression, visit);
        }
        Expression::TSAsExpression(assertion) => {
            walk_type_decl_expression(binder, module, scope, &assertion.expression, visit);
        }
        Expression::TSTypeAssertion(assertion) => {
            walk_type_decl_expression(binder, module, scope, &assertion.expression, visit);
        }
        _ => {}
    }
}

/// Allocate fresh type-parameter ids in source order.
/// Names are paired later when lowering with a parameter frame in scope.
pub(in crate::check::checker) fn alloc_type_param_ids(
    decl: Option<&TSTypeParameterDeclaration<'_>>,
    next_type_param: &mut u32,
) -> Vec<TypeParamId> {
    let Some(decl) = decl else {
        return Vec::new();
    };
    decl.params
        .iter()
        .map(|_| {
            let id = TypeParamId(*next_type_param);
            *next_type_param += 1;
            id
        })
        .collect()
}

/// The type-space `DeclId` a name resolves to from `scope` (binder type slot), if
/// any. Walks the scope graph like value resolution, then reads the `ty` slot.
pub(in crate::check::checker) fn type_decl_id(
    binder: &Binder,
    scope: ScopeId,
    name: &str,
) -> Option<DeclId> {
    let symbol_id = binder.resolve_type(scope, name)?;
    binder.symbols.get(symbol_id).and_then(|s| s.ty)
}

/// The **value**-space `DeclId` a name resolves to from `scope` (binder value slot),
/// if any (M11 — the class constructor side). Mirrors [`type_decl_id`] for the value
/// space.
pub(in crate::check::checker) fn value_decl_id(
    binder: &Binder,
    scope: ScopeId,
    name: &str,
) -> Option<DeclId> {
    let symbol_id = binder.resolve_value(scope, name)?;
    binder.symbols.get(symbol_id).and_then(|s| s.value)
}
