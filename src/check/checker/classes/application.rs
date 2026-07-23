//! Exact-vector class application construction.

use crate::check::query::PublishedClassLookup;
use crate::class_semantics::{
    ClassApplicationArguments, ClassDefaultDeclaration, DemandOutcome, Exhaustion,
};
use crate::types::repr::{ClassId, TypeParamId};
use crate::types::store::TypeId;
use rustc_hash::FxHashMap;

use super::surface_types::SurfaceTypeFactory;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum ClassTypeParameterDefault<Ticket> {
    Absent,
    Ready(TypeId),
    Unsupported(Ticket),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) struct ClassTypeParameter<Ticket> {
    pub id: TypeParamId,
    pub default: ClassTypeParameterDefault<Ticket>,
}

impl<Ticket> ClassTypeParameter<Ticket> {
    fn has_default(&self) -> bool {
        !matches!(self.default, ClassTypeParameterDefault::Absent)
    }
}

pub(in crate::check::checker) fn required_type_argument_count<T>(
    parameters: &[T],
    has_default: impl Fn(&T) -> bool,
) -> usize {
    parameters
        .iter()
        .rposition(|parameter| !has_default(parameter))
        .map_or(0, |index| index + 1)
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum ExplicitClassArgument {
    Ready(TypeId),
    Unavailable,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum ClassApplicationKind {
    TypeReference,
    NewExpression,
}

/// Source distinction required for `new C()` versus `new C<>()`/`new C<A>()`.
/// The omitted form may use constructor inference; an explicit list owns TK2558
/// before defaults or inference are consulted, even when that list is empty.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(in crate::check::checker) enum SourceClassArguments<'a> {
    Omitted,
    Explicit(&'a [ExplicitClassArgument]),
}

impl<'a> SourceClassArguments<'a> {
    fn explicit(self) -> &'a [ExplicitClassArgument] {
        match self {
            SourceClassArguments::Omitted => &[],
            SourceClassArguments::Explicit(arguments) => arguments,
        }
    }

    fn owns_arity(self, kind: ClassApplicationKind) -> bool {
        kind == ClassApplicationKind::TypeReference
            || matches!(self, SourceClassArguments::Explicit(_))
    }
}

pub(in crate::check::checker) struct ClassApplicationRequest<'a, Ticket> {
    pub class: ClassId,
    pub parameters: &'a [ClassTypeParameter<Ticket>],
    /// All explicit children have already been visited, even when arity is invalid.
    pub source_arguments: SourceClassArguments<'a>,
    pub inferred: &'a [(TypeParamId, TypeId)],
    pub kind: ClassApplicationKind,
}

pub(in crate::check::checker) fn build_class_application<Ticket: Copy>(
    factory: &mut SurfaceTypeFactory<'_>,
    published: &impl PublishedClassLookup,
    request: ClassApplicationRequest<'_, Ticket>,
) -> DemandOutcome<TypeId> {
    let class = request.class;
    let arguments = match complete_class_arguments(factory, request) {
        DemandOutcome::Ready(arguments) => arguments,
        DemandOutcome::Exhausted(reason) => return DemandOutcome::Exhausted(reason),
    };

    match published.require_class(class) {
        DemandOutcome::Ready(()) => {
            DemandOutcome::Ready(factory.intern_class_instance(class, arguments))
        }
        DemandOutcome::Exhausted(Exhaustion::ClassHeritagePoison { .. })
        | DemandOutcome::Exhausted(Exhaustion::ClassInitializerPoison { .. })
        | DemandOutcome::Exhausted(Exhaustion::ClassSurfacePoison { .. }) => {
            DemandOutcome::Exhausted(Exhaustion::ClassApplicationArguments(
                ClassApplicationArguments::TargetPoisoned { class },
            ))
        }
        DemandOutcome::Exhausted(reason) => DemandOutcome::Exhausted(reason),
    }
}

/// Validate and materialize a complete real vector without consulting class
/// publication. Reservation/surface graph construction uses this before any SCC
/// is public; post-publication consumers use [`build_class_application`].
pub(in crate::check::checker) fn complete_class_arguments<Ticket: Copy>(
    factory: &mut SurfaceTypeFactory<'_>,
    request: ClassApplicationRequest<'_, Ticket>,
) -> DemandOutcome<Vec<TypeId>> {
    let expected_max = request.parameters.len();
    let expected_min =
        required_type_argument_count(request.parameters, ClassTypeParameter::has_default);
    let explicit = request.source_arguments.explicit();
    let actual = explicit.len();

    if actual > expected_max
        || (request.source_arguments.owns_arity(request.kind) && actual < expected_min)
    {
        return DemandOutcome::Exhausted(Exhaustion::ClassApplicationArguments(
            ClassApplicationArguments::WrongArity {
                expected_min,
                expected_max,
                actual,
            },
        ));
    }

    let mut arguments = Vec::with_capacity(expected_max);
    let mut substitutions = FxHashMap::default();
    for (index, parameter) in request.parameters.iter().enumerate() {
        let argument = if let Some(argument) = explicit.get(index) {
            match argument {
                ExplicitClassArgument::Ready(ty) => *ty,
                ExplicitClassArgument::Unavailable => {
                    return DemandOutcome::Exhausted(Exhaustion::ClassApplicationArguments(
                        ClassApplicationArguments::UnavailableExplicitArgument { index },
                    ));
                }
            }
        } else if let Some((_, inferred)) =
            request.inferred.iter().find(|(id, _)| *id == parameter.id)
        {
            *inferred
        } else {
            match parameter.default {
                ClassTypeParameterDefault::Ready(default) => {
                    factory.substitute(default, &substitutions)
                }
                ClassTypeParameterDefault::Unsupported(_) => {
                    return DemandOutcome::Exhausted(Exhaustion::ClassApplicationArguments(
                        ClassApplicationArguments::UnsupportedDefault {
                            declaration: ClassDefaultDeclaration {
                                class: request.class,
                                parameter: parameter.id,
                                index,
                            },
                        },
                    ));
                }
                ClassTypeParameterDefault::Absent => {
                    return DemandOutcome::Exhausted(Exhaustion::ClassApplicationArguments(
                        ClassApplicationArguments::InferenceIncomplete { index },
                    ));
                }
            }
        };
        arguments.push(argument);
        substitutions.insert(parameter.id, argument);
    }

    DemandOutcome::Ready(arguments)
}

pub(in crate::check::checker) fn build_open_class_application<Ticket>(
    factory: &mut SurfaceTypeFactory<'_>,
    class: ClassId,
    parameters: &[ClassTypeParameter<Ticket>],
    parameter_types: &[TypeId],
) -> DemandOutcome<TypeId> {
    if parameters.len() != parameter_types.len() {
        return DemandOutcome::Exhausted(Exhaustion::ClassApplicationArguments(
            ClassApplicationArguments::WrongArity {
                expected_min: parameters.len(),
                expected_max: parameters.len(),
                actual: parameter_types.len(),
            },
        ));
    }
    DemandOutcome::Ready(factory.intern_class_instance(class, parameter_types.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class_semantics::{ClassConstructionState, PublishedClasses};
    use crate::types::Interner;

    fn parameter(id: u32, default: ClassTypeParameterDefault<u32>) -> ClassTypeParameter<u32> {
        ClassTypeParameter {
            id: TypeParamId(id),
            default,
        }
    }

    #[test]
    fn only_complete_real_vectors_are_interned() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let class = ClassId(1);
        let published = PublishedClasses::forged(class, ClassConstructionState::Published);
        let mut factory = SurfaceTypeFactory::new(&mut interner);
        let parameters = [
            parameter(0, ClassTypeParameterDefault::Absent),
            parameter(1, ClassTypeParameterDefault::Ready(wk.string)),
        ];

        let ready = build_class_application(
            &mut factory,
            &published,
            ClassApplicationRequest {
                class,
                parameters: &parameters,
                source_arguments: SourceClassArguments::Explicit(&[ExplicitClassArgument::Ready(
                    wk.number,
                )]),
                inferred: &[],
                kind: ClassApplicationKind::TypeReference,
            },
        );
        let DemandOutcome::Ready(ready) = ready else {
            panic!("complete explicit/default vector must be ready");
        };
        assert_eq!(
            factory.store().class_instance_type(ready).unwrap().args,
            [wk.number, wk.string]
        );

        let before = factory.store().len();
        let unavailable = build_class_application(
            &mut factory,
            &published,
            ClassApplicationRequest {
                class,
                parameters: &parameters,
                source_arguments: SourceClassArguments::Explicit(&[
                    ExplicitClassArgument::Ready(wk.number),
                    ExplicitClassArgument::Unavailable,
                ]),
                inferred: &[],
                kind: ClassApplicationKind::TypeReference,
            },
        );
        assert!(matches!(
            unavailable,
            DemandOutcome::Exhausted(Exhaustion::ClassApplicationArguments(
                ClassApplicationArguments::UnavailableExplicitArgument { index: 1 }
            ))
        ));
        assert_eq!(factory.store().len(), before);
    }

    #[test]
    fn arity_precedes_unavailable_children_and_defaults() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let class = ClassId(2);
        let published = PublishedClasses::forged(class, ClassConstructionState::Published);
        let mut factory = SurfaceTypeFactory::new(&mut interner);
        let parameters = [parameter(0, ClassTypeParameterDefault::Unsupported(7))];
        let outcome = build_class_application(
            &mut factory,
            &published,
            ClassApplicationRequest {
                class,
                parameters: &parameters,
                source_arguments: SourceClassArguments::Explicit(&[
                    ExplicitClassArgument::Ready(wk.never),
                    ExplicitClassArgument::Unavailable,
                ]),
                inferred: &[],
                kind: ClassApplicationKind::TypeReference,
            },
        );
        assert!(matches!(
            outcome,
            DemandOutcome::Exhausted(Exhaustion::ClassApplicationArguments(
                ClassApplicationArguments::WrongArity { actual: 2, .. }
            ))
        ));
    }

    #[test]
    fn unsupported_default_and_inference_are_distinct() {
        let mut interner = Interner::with_intrinsics();
        let class = ClassId(3);
        let published = PublishedClasses::forged(class, ClassConstructionState::Published);
        let mut factory = SurfaceTypeFactory::new(&mut interner);

        let unsupported = build_class_application(
            &mut factory,
            &published,
            ClassApplicationRequest {
                class,
                parameters: &[parameter(0, ClassTypeParameterDefault::Unsupported(1))],
                source_arguments: SourceClassArguments::Omitted,
                inferred: &[],
                kind: ClassApplicationKind::NewExpression,
            },
        );
        assert_eq!(
            unsupported,
            DemandOutcome::Exhausted(Exhaustion::ClassApplicationArguments(
                ClassApplicationArguments::UnsupportedDefault {
                    declaration: ClassDefaultDeclaration {
                        class,
                        parameter: TypeParamId(0),
                        index: 0,
                    },
                },
            ))
        );

        let missing = build_class_application(
            &mut factory,
            &published,
            ClassApplicationRequest {
                class,
                parameters: &[parameter(0, ClassTypeParameterDefault::Absent)],
                source_arguments: SourceClassArguments::Omitted,
                inferred: &[],
                kind: ClassApplicationKind::NewExpression,
            },
        );
        assert!(matches!(
            missing,
            DemandOutcome::Exhausted(Exhaustion::ClassApplicationArguments(
                ClassApplicationArguments::InferenceIncomplete { index: 0 }
            ))
        ));
    }

    #[test]
    fn explicit_new_list_owns_wrong_arity_but_omitted_list_uses_inference() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let class = ClassId(4);
        let published = PublishedClasses::forged(class, ClassConstructionState::Published);
        let parameters = [
            parameter(0, ClassTypeParameterDefault::Absent),
            parameter(1, ClassTypeParameterDefault::Absent),
        ];
        let mut factory = SurfaceTypeFactory::new(&mut interner);

        let explicit = build_class_application(
            &mut factory,
            &published,
            ClassApplicationRequest {
                class,
                parameters: &parameters,
                source_arguments: SourceClassArguments::Explicit(&[ExplicitClassArgument::Ready(
                    wk.number,
                )]),
                inferred: &[(TypeParamId(1), wk.string)],
                kind: ClassApplicationKind::NewExpression,
            },
        );
        assert!(matches!(
            explicit,
            DemandOutcome::Exhausted(Exhaustion::ClassApplicationArguments(
                ClassApplicationArguments::WrongArity { actual: 1, .. }
            ))
        ));

        let omitted = build_class_application(
            &mut factory,
            &published,
            ClassApplicationRequest {
                class,
                parameters: &parameters,
                source_arguments: SourceClassArguments::Omitted,
                inferred: &[(TypeParamId(0), wk.number), (TypeParamId(1), wk.string)],
                kind: ClassApplicationKind::NewExpression,
            },
        );
        let DemandOutcome::Ready(omitted) = omitted else {
            panic!("omitted new arguments may use constructor inference");
        };
        assert_eq!(
            factory.store().class_instance_type(omitted).unwrap().args,
            [wk.number, wk.string]
        );
    }

    #[test]
    fn recovery_default_holes_keep_later_required_parameters_in_arity() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let class = ClassId(5);
        let published = PublishedClasses::forged(class, ClassConstructionState::Published);
        let parameters = [
            parameter(0, ClassTypeParameterDefault::Ready(wk.string)),
            parameter(1, ClassTypeParameterDefault::Absent),
            parameter(2, ClassTypeParameterDefault::Ready(wk.boolean)),
        ];
        let mut factory = SurfaceTypeFactory::new(&mut interner);

        let too_short = build_class_application(
            &mut factory,
            &published,
            ClassApplicationRequest {
                class,
                parameters: &parameters,
                source_arguments: SourceClassArguments::Explicit(&[ExplicitClassArgument::Ready(
                    wk.number,
                )]),
                inferred: &[],
                kind: ClassApplicationKind::TypeReference,
            },
        );
        assert!(matches!(
            too_short,
            DemandOutcome::Exhausted(Exhaustion::ClassApplicationArguments(
                ClassApplicationArguments::WrongArity {
                    expected_min: 2,
                    expected_max: 3,
                    actual: 1,
                }
            ))
        ));

        let ready = build_class_application(
            &mut factory,
            &published,
            ClassApplicationRequest {
                class,
                parameters: &parameters,
                source_arguments: SourceClassArguments::Explicit(&[
                    ExplicitClassArgument::Ready(wk.number),
                    ExplicitClassArgument::Ready(wk.string),
                ]),
                inferred: &[],
                kind: ClassApplicationKind::TypeReference,
            },
        );
        let DemandOutcome::Ready(ready) = ready else {
            panic!("the trailing recovery default must complete the exact vector");
        };
        assert_eq!(
            factory.store().class_instance_type(ready).unwrap().args,
            [wk.number, wk.string, wk.boolean]
        );
    }

    #[test]
    fn ready_default_is_substituted_through_the_completed_explicit_frame() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let t = interner.intern_type_param(TypeParamId(20), "T");
        let mut factory = SurfaceTypeFactory::new(&mut interner);
        let parameters = [
            parameter(20, ClassTypeParameterDefault::Absent),
            parameter(21, ClassTypeParameterDefault::Ready(t)),
        ];

        let outcome = complete_class_arguments(
            &mut factory,
            ClassApplicationRequest {
                class: ClassId(20),
                parameters: &parameters,
                source_arguments: SourceClassArguments::Explicit(&[ExplicitClassArgument::Ready(
                    wk.string,
                )]),
                inferred: &[],
                kind: ClassApplicationKind::TypeReference,
            },
        );

        assert_eq!(outcome, DemandOutcome::Ready(vec![wk.string, wk.string]));
    }

    #[test]
    fn chained_defaults_share_the_explicit_class_instance_identity() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let t = interner.intern_type_param(TypeParamId(30), "T");
        let u = interner.intern_type_param(TypeParamId(31), "U");
        let class = ClassId(30);
        let published = PublishedClasses::forged(class, ClassConstructionState::Published);
        let parameters = [
            parameter(30, ClassTypeParameterDefault::Absent),
            parameter(31, ClassTypeParameterDefault::Ready(t)),
            parameter(32, ClassTypeParameterDefault::Ready(u)),
        ];
        let mut factory = SurfaceTypeFactory::new(&mut interner);

        let defaulted = build_class_application(
            &mut factory,
            &published,
            ClassApplicationRequest {
                class,
                parameters: &parameters,
                source_arguments: SourceClassArguments::Explicit(&[ExplicitClassArgument::Ready(
                    wk.string,
                )]),
                inferred: &[],
                kind: ClassApplicationKind::TypeReference,
            },
        );
        let explicit = build_class_application(
            &mut factory,
            &published,
            ClassApplicationRequest {
                class,
                parameters: &parameters,
                source_arguments: SourceClassArguments::Explicit(&[
                    ExplicitClassArgument::Ready(wk.string),
                    ExplicitClassArgument::Ready(wk.string),
                    ExplicitClassArgument::Ready(wk.string),
                ]),
                inferred: &[],
                kind: ClassApplicationKind::TypeReference,
            },
        );

        assert_eq!(defaulted, explicit);
        let DemandOutcome::Ready(instance) = defaulted else {
            panic!("completed chained defaults must publish one class instance");
        };
        assert_eq!(
            factory.store().class_instance_type(instance).unwrap().args,
            [wk.string, wk.string, wk.string]
        );
    }

    #[test]
    fn unsupported_default_after_ready_default_returns_exact_protocol_without_an_instance() {
        let mut interner = Interner::with_intrinsics();
        let wk = interner.well_known();
        let t = interner.intern_type_param(TypeParamId(40), "T");
        let class = ClassId(40);
        let parameters = [
            parameter(40, ClassTypeParameterDefault::Absent),
            parameter(41, ClassTypeParameterDefault::Ready(t)),
            parameter(42, ClassTypeParameterDefault::Unsupported(91)),
        ];
        let mut factory = SurfaceTypeFactory::new(&mut interner);
        let before = factory.store().len();

        let outcome = complete_class_arguments(
            &mut factory,
            ClassApplicationRequest {
                class,
                parameters: &parameters,
                source_arguments: SourceClassArguments::Explicit(&[ExplicitClassArgument::Ready(
                    wk.string,
                )]),
                inferred: &[],
                kind: ClassApplicationKind::TypeReference,
            },
        );

        assert_eq!(
            outcome,
            DemandOutcome::Exhausted(Exhaustion::ClassApplicationArguments(
                ClassApplicationArguments::UnsupportedDefault {
                    declaration: ClassDefaultDeclaration {
                        class,
                        parameter: TypeParamId(42),
                        index: 2,
                    },
                },
            ))
        );
        assert_eq!(factory.store().len(), before);
    }
}
