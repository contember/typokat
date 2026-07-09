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
    Class, Declaration, Expression, Program, Statement, TSInterfaceDeclaration,
    TSInterfaceHeritage, TSTypeAliasDeclaration, TSTypeParameterDeclaration,
};

mod interface;
mod params;
mod resolve;

impl<'a, 'ast> Pass<'a, 'ast> {
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
            let (placeholder, params, param_decl, annotation, name, name_span) =
                match &self.type_decls[index] {
                    TypeDecl::Alias {
                        conditional_template: Some(placeholder),
                        params,
                        param_decl,
                        annotation,
                        name,
                        name_span,
                        ..
                    } => (
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
            let lowered = self.with_type_params(frame, |pass| {
                pass.lower_type_param_constraints(scope, param_decl, &params);
                pass.lower_annotation(scope, annotation)
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
            let (placeholder, params, param_decl, annotation, name, name_span) =
                match &self.type_decls[index] {
                    TypeDecl::Alias {
                        mapped_template: Some(placeholder),
                        params,
                        param_decl,
                        annotation,
                        name,
                        name_span,
                        ..
                    } => (
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
            self.resolving_alias_stack
                .push((decl_id, name_span, name, self.alias_indirection_depth));
            let lowered = self.with_type_params(frame, |pass| {
                pass.lower_type_param_constraints(scope, param_decl, &params);
                pass.lower_annotation(scope, annotation)
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

        // Fill class types after named annotations are available; bodies are checked later.
        for index in start..end {
            if matches!(self.type_decls[index], TypeDecl::Class { .. }) {
                self.ensure_class_filled(scope, index);
            }
        }

        // Value-position annotations may evaluate conditionals after fill.
        self.building_template = false;
    }

    /// Fill one interface's reserved object with its composed type.
    /// Bases are filled first in any declaration order; `Filling` breaks out-of-scope
    /// `extends` cycles without a diagnostic.
    fn ensure_interface_filled(&mut self, scope: ScopeId, index: usize) {
        match self.template_fill.get(index).copied() {
            // Already filled, or filling (an `extends` cycle re-entered) — do not recurse.
            Some(ClassFillState::Done) | Some(ClassFillState::Filling) => return,
            Some(ClassFillState::Pending) => {}
            // Out of range: nothing to fill.
            None => return,
        }
        let TypeDecl::Interface {
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
        let params = params.clone();

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

    /// Fill one seeded object-literal alias's reserved object with lowered members.
    /// Runs on demand in `template_fill`; `resolving_alias` stays set so nested
    /// mapped self-references still report `TK2456`.
    fn ensure_object_alias_filled(&mut self, scope: ScopeId, index: usize) {
        match self.template_fill.get(index).copied() {
            Some(ClassFillState::Done) | Some(ClassFillState::Filling) => return,
            Some(ClassFillState::Pending) => {}
            None => return,
        }
        let (reserved, members, name, name_span) = match &self.type_decls[index] {
            TypeDecl::Alias {
                object_template: Some(reserved),
                annotation: oxc_ast::ast::TSType::TSTypeLiteral(lit),
                name,
                name_span,
                ..
            } => (*reserved, &lit.members, name.clone(), *name_span),
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
    /// Interfaces recurse, classes use class fill, and aliases resolve then fill any
    /// reserved template they land on. Generic alias heritage resolves on demand at
    /// instantiation time.
    fn ensure_heritage_base_filled(&mut self, scope: ScopeId, heritage: &TSInterfaceHeritage<'_>) {
        let Expression::Identifier(ident) = &heritage.expression else {
            return;
        };
        let Some(decl_id) = type_decl_id(self.binder, scope, ident.name.as_str()) else {
            return;
        };
        match self.type_decls.get(decl_id.index()) {
            Some(TypeDecl::Interface { .. }) => self.ensure_interface_filled(scope, decl_id.index()),
            Some(TypeDecl::Class { .. }) => self.ensure_class_filled(scope, decl_id.index()),
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
            TypeDecl::Interface { reserved, .. } | TypeDecl::Class { reserved, .. } => {
                *reserved == ty
            }
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
            Some(TypeDecl::Class { .. }) => self.ensure_class_filled(scope, index),
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
    // Build by walking declarations in source order; the binder assigned type
    // `DeclId`s in that same order (`bind_type_declarations`), so pushing in order
    // keeps the decl table index-aligned with the `DeclId`s.
    for stmt in &program.body {
        match top_type_decl(stmt) {
            Some(TopTypeDecl::Interface(iface)) => {
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
                            reserved,
                            params,
                            param_decl: iface.type_parameters.as_deref(),
                            members: &iface.body.body,
                            extends: &iface.extends,
                        },
                    );
                }
            }
            Some(TopTypeDecl::Alias(alias)) => {
                let decl_id = type_decl_id(binder, scope, alias.id.name.as_str());
                let params =
                    alloc_type_param_ids(alias.type_parameters.as_deref(), next_type_param);
                // M25: a top-level conditional-type body reserves a conditional template
                // id and seeds `type_resolved`, so a self-recursive reference resolves to
                // it (as a lazy instantiation) rather than expanding at lowering. The
                // placeholder is filled in the fill step.
                let conditional_template =
                    if matches!(alias.type_annotation, oxc_ast::ast::TSType::TSConditionalType(_)) {
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
                    && matches!(alias.type_annotation, oxc_ast::ast::TSType::TSTypeLiteral(_))
                {
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
            // Named classes reserve their instance type id before fill, enabling own
            // and sibling references. Binder/source order keeps `decls` aligned with
            // type `DeclId`s; anonymous classes have no type name to reserve.
            Some(TopTypeDecl::Class(class)) if class.id.is_some() => {
                let reserved = interner.reserve_object();
                // M13: a fresh stable `ClassId` for this declaration (source order),
                // stamped onto its members in `fill_class`.
                let class_id = ClassId(*next_class_id);
                *next_class_id += 1;
                if let Some(id) = &class.id {
                    let decl_id = type_decl_id(binder, scope, id.name.as_str());
                    if let Some(decl_id) = decl_id {
                        if let Some(slot) = resolved.get_mut(decl_id.index()) {
                            *slot = Some(reserved);
                        }
                        // M16: allocate one id per declared type parameter (in source order),
                        // paired with their names later when the class body is lowered with the
                        // parameter frame in scope (`fill_class`) — exactly like an interface.
                        let params =
                            alloc_type_param_ids(class.type_parameters.as_deref(), next_type_param);
                        place_type_decl(
                            decls,
                            decl_id.index(),
                            TypeDecl::Class {
                                reserved,
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
    }
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

enum TopTypeDecl<'ast> {
    Interface(&'ast TSInterfaceDeclaration<'ast>),
    Alias(&'ast TSTypeAliasDeclaration<'ast>),
    Class(&'ast Class<'ast>),
}

fn top_type_decl<'ast>(stmt: &'ast Statement<'ast>) -> Option<TopTypeDecl<'ast>> {
    match stmt {
        Statement::TSInterfaceDeclaration(iface) => Some(TopTypeDecl::Interface(iface)),
        Statement::TSTypeAliasDeclaration(alias) => Some(TopTypeDecl::Alias(alias)),
        Statement::ClassDeclaration(class) => Some(TopTypeDecl::Class(class)),
        Statement::ExportNamedDeclaration(export) => match &export.declaration {
            Some(Declaration::TSInterfaceDeclaration(iface)) => Some(TopTypeDecl::Interface(iface)),
            Some(Declaration::TSTypeAliasDeclaration(alias)) => Some(TopTypeDecl::Alias(alias)),
            Some(Declaration::ClassDeclaration(class)) => Some(TopTypeDecl::Class(class)),
            _ => None,
        },
        _ => None,
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
    let symbol_id = binder.graph.resolve(scope, name)?;
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
    let symbol_id = binder.graph.resolve(scope, name)?;
    binder.symbols.get(symbol_id).and_then(|s| s.value)
}
