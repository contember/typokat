use super::*;
use crate::binder::scope::ScopeId;
use crate::types::repr::{ObjectType, PropertyType};
use crate::types::store::TypeId;
use oxc_ast::ast::{Expression, TSInterfaceHeritage, TSSignature};
use rustc_hash::FxHashSet;

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Fold `extends` bases into `own`, then return the composed object.
    /// Bases merge left-to-right, own members override inherited ones, and non-object
    /// bases contribute nothing in this deferred slice.
    pub(super) fn compose_interface_heritage(
        &mut self,
        scope: ScopeId,
        own: ObjectType,
        extends: &[TSInterfaceHeritage<'_>],
    ) -> ObjectType {
        if extends.is_empty() {
            return own;
        }
        let mut base = ObjectType::default();
        for heritage in extends {
            let Some(base_ty) = self.resolve_heritage_type(scope, heritage) else {
                continue;
            };
            let Some(base_obj) = self.interner.store().object_type(base_ty).cloned() else {
                continue;
            };
            base = merge_object_members(base, base_obj);
        }
        merge_object_members(base, own)
    }

    /// B28 — resolve a heritage clause's base to its `TypeId`: a bare interface/alias
    /// reference resolves through `type_resolved`; a generic base (`extends Base<T>`)
    /// instantiates its template with the lowered arguments. Non-identifier bases (out of
    /// subset) yield `None`.
    fn resolve_heritage_type(
        &mut self,
        scope: ScopeId,
        heritage: &TSInterfaceHeritage<'_>,
    ) -> Option<TypeId> {
        let Expression::Identifier(ident) = &heritage.expression else {
            return None;
        };
        let decl_id = type_decl_id(self.binder, scope, ident.name.as_str())?;
        match heritage.type_arguments.as_deref() {
            Some(args) => self.instantiate_type_reference(scope, decl_id, args),
            None => Some(self.resolve_type_decl(scope, decl_id)),
        }
    }

    /// Lower interface members to the reserved nominal object's `ObjectType`.
    /// Unsupported or unlowerable members are skipped; the interface keeps the
    /// expressible subset.
    pub(super) fn lower_interface_members(
        &mut self,
        scope: ScopeId,
        members: &[TSSignature<'_>],
    ) -> ObjectType {
        let mut object = ObjectType::default();
        let overloaded_method_names = self.overloaded_method_names(members);
        let mut lowered_overloaded_methods: FxHashSet<String> = FxHashSet::default();
        for member in members {
            match member {
                TSSignature::TSPropertySignature(sig) => {
                    let Some(name) = sig.key.static_name() else {
                        self.record_property_signature_computed_key(&sig.key);
                        continue;
                    };
                    if overloaded_method_names.contains(name.as_ref()) {
                        continue;
                    }
                    let ty = match sig.type_annotation.as_ref() {
                        Some(annotation) => {
                            let Some(ty) =
                                self.lower_annotation(scope, &annotation.type_annotation)
                            else {
                                continue;
                            };
                            ty
                        }
                        // tsc treats annotationless interface properties as `any`.
                        None => self.interner.well_known().any,
                    };
                    // Optional properties are real members with `| undefined` baked in
                    // while interning is available. The read-only relation engine then
                    // uses existing union-target logic, and `keyof`/indexed access see
                    // the key.
                    let ty = if sig.optional {
                        let undefined = self.interner.well_known().undefined;
                        self.interner.union(vec![ty, undefined])
                    } else {
                        ty
                    };
                    let mut prop = PropertyType::public(name.into_owned(), ty);
                    prop.optional = sig.optional;
                    // Preserve `readonly` on interface members. It is hashed into
                    // structural identity, ignored for assignability, and gates only
                    // assignment targets (`TK2540`).
                    prop.readonly = sig.readonly;
                    object.properties.push(prop);
                }
                // M19: an index signature on an interface — lowered into the
                // string/number slot. An unsupported one (non-`string`/`number` key,
                // un-lowerable value) is **skipped** (lenient, like an out-of-subset
                // property), so the interface keeps the members it can express.
                TSSignature::TSIndexSignature(sig) => {
                    let _ = self.lower_index_signature(scope, sig, &mut object);
                }
                TSSignature::TSMethodSignature(sig) => {
                    let Some(name) = sig.key.static_name() else {
                        self.record_method_signature_computed_key(&sig.key);
                        continue;
                    };
                    if overloaded_method_names.contains(name.as_ref()) {
                        let name = name.into_owned();
                        if lowered_overloaded_methods.insert(name.clone()) {
                            if let Some(prop) =
                                self.lower_method_overload_property(scope, members, &name)
                            {
                                object.properties.push(prop);
                            }
                        }
                        continue;
                    }
                    if let Some(prop) = self.lower_method_signature_property(scope, sig) {
                        object.properties.push(prop);
                    }
                }
                TSSignature::TSCallSignatureDeclaration(sig) => {
                    if let Some(signature) = self.lower_call_signature(scope, sig) {
                        object.call_signatures.push(signature);
                    }
                }
                TSSignature::TSConstructSignatureDeclaration(sig) => {
                    if let Some(signature) = self.lower_construct_signature(scope, sig) {
                        object.construct_signatures.push(signature);
                    }
                }
            }
        }
        object
    }
}

/// Merge `overlay` onto `base`, with overlay members/signatures winning conflicts.
/// Used for interface `extends` and own-member composition.
fn merge_object_members(base: ObjectType, overlay: ObjectType) -> ObjectType {
    let mut properties = base.properties;
    for prop in overlay.properties {
        match properties.iter_mut().find(|p| p.name == prop.name) {
            Some(existing) => *existing = prop,
            None => properties.push(prop),
        }
    }
    ObjectType {
        properties,
        string_index: overlay.string_index.or(base.string_index),
        number_index: overlay.number_index.or(base.number_index),
        call_signatures: if overlay.call_signatures.is_empty() {
            base.call_signatures
        } else {
            overlay.call_signatures
        },
        construct_signatures: if overlay.construct_signatures.is_empty() {
            base.construct_signatures
        } else {
            overlay.construct_signatures
        },
    }
}
