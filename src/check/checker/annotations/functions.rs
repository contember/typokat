use super::super::calls::{parameter_count, parameter_syntaxes, ParameterSyntax};
use super::super::decls::alloc_type_param_ids;
use super::*;
use oxc_ast::ast::TSTypeParameterDeclaration;

impl<'a, 'ast> Pass<'a, 'ast> {
    /// Lower a constructor-type annotation (`new (x: T) => U`) to an object carrying
    /// a single construct signature, making it equivalent to `{ new (x: T): U }` in
    /// relation while preserving named object members for the object-literal form.
    pub(super) fn lower_constructor_type(
        &mut self,
        scope: ScopeId,
        ctor: &TSConstructorType<'_>,
    ) -> Option<TypeId> {
        let signature = self.lower_generic_strict_construct_function_type(
            scope,
            ctor.type_parameters.as_deref(),
            &ctor.params,
            Some(&ctor.return_type),
        );
        if ctor.r#abstract {
            return None;
        }
        let signature = signature?;
        Some(self.interner.intern_object(ObjectType {
            construct_signatures: vec![signature],
            ..Default::default()
        }))
    }

    pub(super) fn lower_generic_strict_construct_function_type(
        &mut self,
        scope: ScopeId,
        type_parameter_decl: Option<&TSTypeParameterDeclaration<'_>>,
        params: &FormalParameters<'_>,
        return_type: Option<&TSTypeAnnotation<'_>>,
    ) -> Option<TypeId> {
        let ids = alloc_type_param_ids(type_parameter_decl, &mut self.next_type_param);
        let frame = self.build_type_param_frame(type_parameter_decl, &ids);
        self.with_type_params(frame, |pass| {
            let type_params = pass.lower_signature_type_params(scope, type_parameter_decl, &ids);
            let params = pass.lower_strict_signature_parameters(scope, params, true);
            let ret = return_type.and_then(|return_type| {
                pass.with_indirection(|p| p.lower_annotation(scope, &return_type.type_annotation))
            });
            let (Some(params), Some(ret)) = (params, ret) else {
                return None;
            };
            if type_params.unavailable {
                return None;
            }
            Some(pass.interner.intern_function(FunctionType {
                type_params: type_params.params,
                receiver: None,
                params,
                ret,
            }))
        })
    }

    pub(super) fn lower_strict_signature_parameters(
        &mut self,
        scope: ScopeId,
        params: &FormalParameters<'_>,
        with_indirection: bool,
    ) -> Option<Vec<ParameterType>> {
        let mut lowered = Vec::with_capacity(parameter_count(params));
        let mut unavailable = false;
        for syntax in parameter_syntaxes(params) {
            let name = parameter_name(syntax.pattern());
            let annotation = match syntax {
                ParameterSyntax::Fixed { parameter, .. } => parameter.type_annotation.as_ref(),
                ParameterSyntax::Rest { parameter } => parameter.type_annotation.as_ref(),
            };
            let ty = match annotation {
                Some(annotation) if with_indirection => self
                    .with_indirection(|p| p.lower_annotation(scope, &annotation.type_annotation)),
                Some(annotation) => self.lower_annotation(scope, &annotation.type_annotation),
                None => None,
            };
            match (name, ty) {
                (Some(name), Some(ty)) => lowered.push(syntax.with_type(name, ty)),
                _ => unavailable = true,
            }
        }
        if unavailable {
            return None;
        }
        Some(lowered)
    }
}
