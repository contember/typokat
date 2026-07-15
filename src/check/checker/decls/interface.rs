use super::resolve::QualifiedTypeSegment;
use super::*;
use crate::binder::scope::ScopeId;
use crate::span::Span;
use crate::types::repr::{ObjectType, PropertyType};
use crate::types::store::TypeId;
use oxc_ast::ast::{Expression, TSInterfaceHeritage, TSSignature};
use rustc_hash::FxHashMap;

#[derive(Default)]
struct InterfaceMethodOverloadAccumulator {
    call_signatures: Vec<TypeId>,
    unsupported: bool,
    unavailable: bool,
}

fn flatten_qualified_heritage_expression<'a>(
    expression: &'a Expression<'_>,
    segments: &mut Vec<QualifiedTypeSegment<'a>>,
) -> bool {
    match expression {
        Expression::Identifier(identifier) => {
            segments.push(QualifiedTypeSegment {
                name: identifier.name.as_str(),
                span: Span::from_oxc(identifier.span),
            });
            true
        }
        Expression::StaticMemberExpression(member) => {
            if !flatten_qualified_heritage_expression(&member.object, segments) {
                return false;
            }
            segments.push(QualifiedTypeSegment {
                name: member.property.name.as_str(),
                span: Span::from_oxc(member.property.span),
            });
            true
        }
        _ => false,
    }
}

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
            let demanded = self.evaluate_type(base_ty);
            let Some(base_ty) =
                self.own_type_demand(demanded, crate::span::Span::from_oxc(heritage.span))
            else {
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
        let ident = match &heritage.expression {
            Expression::Identifier(ident) => ident,
            Expression::StaticMemberExpression(member) => {
                let mut segments = Vec::new();
                if flatten_qualified_heritage_expression(&heritage.expression, &mut segments) {
                    self.classify_qualified_type_path(
                        scope,
                        &segments,
                        Span::from_oxc(member.span),
                        heritage.type_arguments.as_deref(),
                    );
                }
                return None;
            }
            _ => return None,
        };
        let decl_id = type_decl_id(self.binder, scope, ident.name.as_str())?;
        if matches!(
            self.type_decls.get(decl_id.index()),
            Some(TypeDecl::Class { .. })
        ) {
            return self.resolve_class_type_reference(
                scope,
                decl_id,
                ident.name.as_str(),
                crate::span::Span::from_oxc(ident.span),
                heritage.type_arguments.as_deref(),
            );
        }
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
        let mut overloads: FxHashMap<String, InterfaceMethodOverloadAccumulator> =
            FxHashMap::default();
        let mut overload_order = Vec::new();
        for member in members {
            match member {
                TSSignature::TSPropertySignature(sig) => {
                    if sig.computed {
                        self.record_property_signature_computed_key(&sig.key);
                        if let Some(annotation) = sig.type_annotation.as_ref() {
                            self.lower_annotation(scope, &annotation.type_annotation);
                        }
                        continue;
                    }
                    let Some(name) = sig.key.static_name() else {
                        self.record_property_signature_computed_key(&sig.key);
                        if let Some(annotation) = sig.type_annotation.as_ref() {
                            self.lower_annotation(scope, &annotation.type_annotation);
                        }
                        continue;
                    };
                    if overloaded_method_names.contains(name.as_ref()) {
                        let name = name.into_owned();
                        if !overloads.contains_key(&name) {
                            overload_order.push(name.clone());
                        }
                        let overload = overloads.entry(name).or_default();
                        overload.unsupported = true;
                        let lowered = sig.type_annotation.as_ref().and_then(|annotation| {
                            self.lower_annotation(scope, &annotation.type_annotation)
                        });
                        if lowered.is_none() {
                            overload.unavailable = true;
                        }
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
                    if sig.computed {
                        self.record_method_signature_computed_key(&sig.key);
                        self.lower_generic_strict_signature_function_type(
                            scope,
                            sig.type_parameters.as_deref(),
                            sig.this_param.as_deref(),
                            &sig.params,
                            sig.return_type.as_deref(),
                        );
                        continue;
                    }
                    let Some(name) = sig.key.static_name() else {
                        self.record_method_signature_computed_key(&sig.key);
                        self.lower_generic_strict_signature_function_type(
                            scope,
                            sig.type_parameters.as_deref(),
                            sig.this_param.as_deref(),
                            &sig.params,
                            sig.return_type.as_deref(),
                        );
                        continue;
                    };
                    if overloaded_method_names.contains(name.as_ref()) {
                        let name = name.into_owned();
                        if !overloads.contains_key(&name) {
                            overload_order.push(name.clone());
                        }
                        let overload = overloads.entry(name).or_default();
                        let signature = self.lower_generic_strict_signature_function_type(
                            scope,
                            sig.type_parameters.as_deref(),
                            sig.this_param.as_deref(),
                            &sig.params,
                            sig.return_type.as_deref(),
                        );
                        if sig.kind != oxc_ast::ast::TSMethodSignatureKind::Method || sig.optional {
                            overload.unsupported = true;
                        }
                        match signature {
                            Some(signature)
                                if sig.kind == oxc_ast::ast::TSMethodSignatureKind::Method
                                    && !sig.optional =>
                            {
                                overload.call_signatures.push(signature);
                            }
                            Some(_) => {}
                            None => overload.unavailable = true,
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
        for name in overload_order {
            let overload = overloads
                .remove(&name)
                .expect("every interface overload retains its accumulator");
            if overload.unavailable || overload.call_signatures.is_empty() {
                continue;
            }
            let ty = if overload.unsupported {
                self.interner.well_known().never
            } else {
                self.interner.intern_object(ObjectType {
                    call_signatures: overload.call_signatures,
                    ..Default::default()
                })
            };
            object.properties.push(PropertyType::public(name, ty));
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

#[cfg(test)]
mod qualified_heritage_tests {
    use crate::diagnostics::DiagnosticCode;
    use crate::driver::{check_source, CheckOutput};
    use crate::span::Span;

    fn checked(source: &str) -> CheckOutput {
        let output = check_source(source);
        assert!(
            output.parse_errors.is_empty(),
            "unexpected parse errors: {:?}",
            output.parse_errors
        );
        output
    }

    fn span_text(source: &str, span: Span) -> &str {
        let start = usize::try_from(span.start).expect("source span start fits usize");
        let end = usize::try_from(span.end).expect("source span end fits usize");
        &source[start..end]
    }

    #[test]
    fn failed_qualified_heritage_replays_path_then_type_argument_at_interface_owner() {
        let source = "interface D extends Missing.Root<Unknown> {}";
        let output = checked(source);
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| (
                    diagnostic.code,
                    diagnostic.message.as_str(),
                    span_text(source, diagnostic.span),
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    DiagnosticCode::TK2503,
                    "Cannot find namespace 'Missing'.",
                    "Missing",
                ),
                (
                    DiagnosticCode::TK2304,
                    "Cannot find name 'Unknown'",
                    "Unknown",
                ),
            ]
        );
        assert!(output
            .incomplete
            .iter()
            .all(|record| record.id != "annotation-lower/type-name/qualified-name"));
    }

    #[test]
    fn successful_qualified_heritage_has_one_precise_wu3_incomplete() {
        let source = "\
namespace HeritageNs { export interface Base {} }
interface D extends HeritageNs.Base {}
";
        let output = checked(source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let records = output
            .incomplete
            .iter()
            .filter(|record| record.id == "annotation-lower/type-name/qualified-name")
            .collect::<Vec<_>>();
        assert_eq!(records.len(), 1, "{records:?}");
        assert_eq!(
            records[0].context,
            "qualified type path classified; leaf lowering deferred to WU3"
        );
        assert_eq!(span_text(source, records[0].span), "HeritageNs.Base");
    }

    #[test]
    fn qualified_heritage_member_lookup_never_falls_back_to_parent_namespace() {
        let source = "\
namespace Root {
  export interface ParentLeaf {}
  export namespace Child {}
}
interface D extends Root.Child.ParentLeaf {}
";
        let output = checked(source);
        assert_eq!(output.diagnostics.len(), 1);
        let diagnostic = &output.diagnostics[0];
        assert_eq!(diagnostic.code, DiagnosticCode::TK2694);
        assert_eq!(
            diagnostic.message,
            "Namespace 'Root.Child' has no exported member 'ParentLeaf'."
        );
        assert_eq!(span_text(source, diagnostic.span), "ParentLeaf");
        assert!(output
            .incomplete
            .iter()
            .all(|record| record.id != "annotation-lower/type-name/qualified-name"));
    }

    #[test]
    fn computed_generic_method_children_keep_constraint_parameter_return_order() {
        let source = r#"
declare const computed: "computed";
interface I {
  [computed]<T extends MissingConstraint.Member>(value: MissingParameter.Member): MissingReturn.Member;
}
"#;
        let output = checked(source);
        assert_eq!(
            output
                .diagnostics
                .iter()
                .map(|diagnostic| (
                    diagnostic.code,
                    span_text(source, diagnostic.span).to_string(),
                ))
                .collect::<Vec<_>>(),
            vec![
                (DiagnosticCode::TK2503, "MissingConstraint".to_string()),
                (DiagnosticCode::TK2503, "MissingParameter".to_string()),
                (DiagnosticCode::TK2503, "MissingReturn".to_string()),
            ]
        );
        assert!(output.incomplete.iter().any(|record| {
            record.id == "signature/method-signature/computed-key"
                && span_text(source, record.span) == "computed"
        }));
    }
}
