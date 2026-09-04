use super::declaration_product::{
    build_packaged_declaration_product, build_packaged_declaration_product_with_order_for_test,
    build_packaged_declaration_product_with_qualified_path_oracle_for_test,
};
use super::profile::ExactLibraryProfile;
use crate::binder::declaration::SourceDeclarationManifestBuildScopeForTest;
use crate::binder::namespace::{
    QualifiedTypePathDeferredReason, QualifiedTypePathResolution, QualifiedValuePathResolution,
};
use crate::binder::product::{
    normalize_qualified_type_path_resolution_for_test,
    normalize_qualified_value_path_resolution_for_test, QualifiedPathRawOracleRowForTest,
    QualifiedPathRawResolutionForTest,
};
use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_ast_visit::Visit;
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType};
use std::collections::BTreeMap;
use std::sync::Arc;
use typokat_library_product::{
    ContainerKind, DependencyKind, IntrinsicMarker, IntrinsicMarkerRecipe,
    LibraryDeclarationProduct, ProductFieldClass, ProductFieldVisitor, QualifiedPathRecipe,
    QualifiedPathResolutionOutcome, QualifiedPathRole, QualifiedPathSegmentRecipe,
    QualifiedPathUnresolvedCause, TypeRecipeNode, TypeReferenceResolution,
};

fn assert_owned_product<T: Send + Sync + 'static>(_: &T) {}

#[derive(Default)]
struct ProductFieldCensus {
    owned_text: usize,
    product_key: usize,
    product_key_list: usize,
    scalar: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct StableSyntaxDependencyRow {
    source_ordinal: usize,
    start: u32,
    end: u32,
    kind: &'static str,
}

struct IndependentDependencyVisitor {
    source_ordinal: usize,
    rows: Vec<StableSyntaxDependencyRow>,
}

impl IndependentDependencyVisitor {
    fn push(&mut self, span: oxc_span::Span, kind: &'static str) {
        self.rows.push(StableSyntaxDependencyRow {
            source_ordinal: self.source_ordinal,
            start: span.start,
            end: span.end,
            kind,
        });
    }
}

impl<'ast> Visit<'ast> for IndependentDependencyVisitor {
    fn enter_node(&mut self, kind: AstKind<'ast>) {
        match kind {
            AstKind::TSTypeReference(value) => self.push(value.span, "reference"),
            AstKind::TSInterfaceHeritage(value) => self.push(value.span, "heritage"),
            AstKind::TSTypeQuery(value) => self.push(value.span, "query"),
            AstKind::TSMethodSignature(value) if value.computed => {
                self.push(value.key.span(), "query")
            }
            AstKind::TSPropertySignature(value) if value.computed => {
                self.push(value.key.span(), "query")
            }
            AstKind::MethodDefinition(value) if value.computed => {
                self.push(value.key.span(), "query")
            }
            AstKind::PropertyDefinition(value) if value.computed => {
                self.push(value.key.span(), "query")
            }
            _ => {}
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AstPathSegment {
    name: String,
    start: u32,
    end: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AstQualifiedPath {
    source_ordinal: usize,
    recipe_start: u32,
    recipe_end: u32,
    role: QualifiedPathRole,
    segments: Vec<AstPathSegment>,
}

struct IndependentQualifiedPathVisitor {
    source_ordinal: usize,
    paths: Vec<AstQualifiedPath>,
}

impl IndependentQualifiedPathVisitor {
    fn push(
        &mut self,
        span: oxc_span::Span,
        role: QualifiedPathRole,
        segments: Vec<AstPathSegment>,
    ) {
        if !segments.is_empty() {
            self.paths.push(AstQualifiedPath {
                source_ordinal: self.source_ordinal,
                recipe_start: span.start,
                recipe_end: span.end,
                role,
                segments,
            });
        }
    }
}

impl<'ast> Visit<'ast> for IndependentQualifiedPathVisitor {
    fn enter_node(&mut self, kind: AstKind<'ast>) {
        match kind {
            AstKind::TSTypeReference(value) => self.push(
                value.span,
                QualifiedPathRole::Reference,
                type_name_segment_sites(&value.type_name),
            ),
            AstKind::TSInterfaceHeritage(value) => {
                if let Some(segments) = expression_segment_sites(&value.expression) {
                    self.push(value.span, QualifiedPathRole::Reference, segments);
                }
            }
            AstKind::TSClassImplements(value) => self.push(
                value.span,
                QualifiedPathRole::Reference,
                type_name_segment_sites(&value.expression),
            ),
            AstKind::Class(value) => {
                if let Some(super_class) = &value.super_class {
                    if let Some(segments) = expression_segment_sites(super_class) {
                        self.push(super_class.span(), QualifiedPathRole::Reference, segments);
                    }
                }
            }
            AstKind::TSTypeQuery(value) => {
                if let Some(segments) = query_name_segment_sites(&value.expr_name) {
                    self.push(value.span, QualifiedPathRole::Query, segments);
                }
            }
            AstKind::TSImportType(value) => {
                if let Some(qualifier) = &value.qualifier {
                    self.push(
                        value.span,
                        QualifiedPathRole::ImportQualifier,
                        import_qualifier_segment_sites(qualifier),
                    );
                }
            }
            _ => {}
        }
    }
}

fn ast_segment(name: &str, span: oxc_span::Span) -> AstPathSegment {
    AstPathSegment {
        name: name.to_owned(),
        start: span.start,
        end: span.end,
    }
}

fn type_name_segment_sites(name: &oxc_ast::ast::TSTypeName<'_>) -> Vec<AstPathSegment> {
    match name {
        oxc_ast::ast::TSTypeName::IdentifierReference(value) => {
            vec![ast_segment(value.name.as_str(), value.span)]
        }
        oxc_ast::ast::TSTypeName::QualifiedName(value) => {
            let mut segments = type_name_segment_sites(&value.left);
            segments.push(ast_segment(value.right.name.as_str(), value.right.span));
            segments
        }
        oxc_ast::ast::TSTypeName::ThisExpression(value) => {
            vec![ast_segment("this", value.span)]
        }
    }
}

fn query_name_segment_sites(
    name: &oxc_ast::ast::TSTypeQueryExprName<'_>,
) -> Option<Vec<AstPathSegment>> {
    match name {
        oxc_ast::ast::TSTypeQueryExprName::IdentifierReference(value) => {
            Some(vec![ast_segment(value.name.as_str(), value.span)])
        }
        oxc_ast::ast::TSTypeQueryExprName::QualifiedName(value) => {
            let mut segments = type_name_segment_sites(&value.left);
            segments.push(ast_segment(value.right.name.as_str(), value.right.span));
            Some(segments)
        }
        oxc_ast::ast::TSTypeQueryExprName::ThisExpression(value) => {
            Some(vec![ast_segment("this", value.span)])
        }
        oxc_ast::ast::TSTypeQueryExprName::TSImportType(_) => None,
    }
}

fn import_qualifier_segment_sites(
    qualifier: &oxc_ast::ast::TSImportTypeQualifier<'_>,
) -> Vec<AstPathSegment> {
    match qualifier {
        oxc_ast::ast::TSImportTypeQualifier::Identifier(value) => {
            vec![ast_segment(value.name.as_str(), value.span)]
        }
        oxc_ast::ast::TSImportTypeQualifier::QualifiedName(value) => {
            let mut segments = import_qualifier_segment_sites(&value.left);
            segments.push(ast_segment(value.right.name.as_str(), value.right.span));
            segments
        }
    }
}

fn expression_segment_sites(
    expression: &oxc_ast::ast::Expression<'_>,
) -> Option<Vec<AstPathSegment>> {
    match expression {
        oxc_ast::ast::Expression::Identifier(value) => {
            Some(vec![ast_segment(value.name.as_str(), value.span)])
        }
        oxc_ast::ast::Expression::StaticMemberExpression(value) => {
            let mut segments = expression_segment_sites(&value.object)?;
            segments.push(ast_segment(
                value.property.name.as_str(),
                value.property.span,
            ));
            Some(segments)
        }
        _ => None,
    }
}

fn independent_qualified_paths(profile: &ExactLibraryProfile) -> Vec<AstQualifiedPath> {
    let mut paths = Vec::new();
    for source in profile.sources().iter() {
        let allocator = Allocator::default();
        let source_text = std::str::from_utf8(source.bytes()).expect("packaged source is UTF-8");
        let parsed = Parser::new(&allocator, source_text, SourceType::d_ts()).parse();
        assert!(!parsed.panicked, "{} parse panicked", source.name());
        assert!(
            parsed.diagnostics.is_empty(),
            "{} parse diagnostics: {:?}",
            source.name(),
            parsed.diagnostics
        );
        let mut visitor = IndependentQualifiedPathVisitor {
            source_ordinal: source.ordinal().index(),
            paths: Vec::new(),
        };
        visitor.visit_program(&parsed.program);
        paths.extend(visitor.paths);
    }
    paths
}

fn recipe_path<'a>(
    product: &'a LibraryDeclarationProduct,
    recipe: &'a typokat_library_product::TypeRecipe,
) -> Option<(QualifiedPathRole, Vec<&'a str>)> {
    let (role, path) = match &recipe.node {
        TypeRecipeNode::Reference { path, .. } => (QualifiedPathRole::Reference, path.as_ref()),
        TypeRecipeNode::Query { path, .. } => (QualifiedPathRole::Query, path.as_ref()),
        TypeRecipeNode::Import { qualifier, .. } if !qualifier.is_empty() => {
            (QualifiedPathRole::ImportQualifier, qualifier.as_ref())
        }
        _ => return None,
    };
    Some((
        role,
        path.iter()
            .map(|name| product.strings()[name.index()].as_str())
            .collect(),
    ))
}

fn recipe_path_keys(
    recipe: &typokat_library_product::TypeRecipe,
) -> Option<(QualifiedPathRole, &[typokat_library_product::StringKey])> {
    match &recipe.node {
        TypeRecipeNode::Reference { path, .. } => {
            Some((QualifiedPathRole::Reference, path.as_ref()))
        }
        TypeRecipeNode::Query { path, .. } => Some((QualifiedPathRole::Query, path.as_ref())),
        TypeRecipeNode::Import { qualifier, .. } if !qualifier.is_empty() => {
            Some((QualifiedPathRole::ImportQualifier, qualifier.as_ref()))
        }
        _ => None,
    }
}

fn recipe_index_for_ast_path(
    product: &LibraryDeclarationProduct,
    expected: &AstQualifiedPath,
) -> usize {
    let source = product
        .sources()
        .iter()
        .position(|source| usize::try_from(source.ordinal).ok() == Some(expected.source_ordinal))
        .expect("AST source exists in product");
    let expected_names = expected
        .segments
        .iter()
        .map(|segment| segment.name.as_str())
        .collect::<Vec<_>>();
    let candidates = product
        .types()
        .iter()
        .enumerate()
        .filter(|(_, recipe)| {
            recipe.site.source.index() == source
                && recipe.site.start == expected.recipe_start
                && recipe.site.end == expected.recipe_end
                && recipe_path(product, recipe)
                    .is_some_and(|(role, names)| role == expected.role && names == expected_names)
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    assert_eq!(candidates.len(), 1, "AST path must map to one recipe");
    candidates[0]
}

fn binder_outcome_for_ast_path(
    expected: &AstQualifiedPath,
    oracle: &[QualifiedPathRawOracleRowForTest],
) -> QualifiedPathResolutionOutcome {
    let expected_path = expected
        .segments
        .iter()
        .map(|segment| segment.name.as_str())
        .collect::<Vec<_>>();
    let matches = oracle
        .iter()
        .filter(|row| {
            usize::try_from(row.source_ordinal).ok() == Some(expected.source_ordinal)
                && row.start == expected.recipe_start
                && row.end == expected.recipe_end
                && row.role == expected.role
                && row
                    .path
                    .iter()
                    .map(String::as_str)
                    .eq(expected_path.iter().copied())
        })
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1, "one pre-normalization binder outcome");
    independently_normalize_raw_resolution(matches[0].resolution)
}

fn independently_normalize_raw_resolution(
    resolution: QualifiedPathRawResolutionForTest,
) -> QualifiedPathResolutionOutcome {
    let unresolved = |cause, segment| QualifiedPathResolutionOutcome::Unresolved {
        cause,
        failing_segment_ordinal: u32::try_from(segment).expect("qualified path ordinal"),
    };
    match resolution {
        QualifiedPathRawResolutionForTest::QualifiedType(raw) => match raw {
            QualifiedTypePathResolution::TypeGroup(_) => QualifiedPathResolutionOutcome::Resolved,
            QualifiedTypePathResolution::MissingRoot { segment } => {
                unresolved(QualifiedPathUnresolvedCause::MissingRoot, segment)
            }
            QualifiedTypePathResolution::TypeOnlyRoot { segment } => {
                unresolved(QualifiedPathUnresolvedCause::TypeOnlyRoot, segment)
            }
            QualifiedTypePathResolution::MissingMember { segment } => {
                unresolved(QualifiedPathUnresolvedCause::MissingMember, segment)
            }
            QualifiedTypePathResolution::TypeOnlyIntermediate { segment } => {
                unresolved(QualifiedPathUnresolvedCause::TypeOnlyIntermediate, segment)
            }
            QualifiedTypePathResolution::ValueOnlyLeaf { segment } => {
                unresolved(QualifiedPathUnresolvedCause::ValueOnlyLeaf, segment)
            }
            QualifiedTypePathResolution::Unavailable { segment } => {
                unresolved(QualifiedPathUnresolvedCause::Unavailable, segment)
            }
            QualifiedTypePathResolution::Deferred { segment, reason } => unresolved(
                QualifiedPathUnresolvedCause::Deferred(match reason {
                    QualifiedTypePathDeferredReason::Import => {
                        typokat_library_product::QualifiedPathDeferredReason::Import
                    }
                    QualifiedTypePathDeferredReason::Enum => {
                        typokat_library_product::QualifiedPathDeferredReason::Enum
                    }
                }),
                segment,
            ),
        },
        QualifiedPathRawResolutionForTest::QualifiedValue(raw) => match raw {
            QualifiedValuePathResolution::Value(_) => QualifiedPathResolutionOutcome::Resolved,
            QualifiedValuePathResolution::MissingRoot { segment } => {
                unresolved(QualifiedPathUnresolvedCause::MissingRoot, segment)
            }
            QualifiedValuePathResolution::TypeOnlyRoot { segment } => {
                unresolved(QualifiedPathUnresolvedCause::TypeOnlyRoot, segment)
            }
            QualifiedValuePathResolution::MissingMember { segment } => {
                unresolved(QualifiedPathUnresolvedCause::MissingMember, segment)
            }
            QualifiedValuePathResolution::NamespaceOnlyLeaf { segment } => {
                unresolved(QualifiedPathUnresolvedCause::NamespaceOnlyLeaf, segment)
            }
            QualifiedValuePathResolution::Unavailable { segment } => {
                unresolved(QualifiedPathUnresolvedCause::Unavailable, segment)
            }
            QualifiedValuePathResolution::Deferred { segment, reason } => unresolved(
                QualifiedPathUnresolvedCause::Deferred(match reason {
                    QualifiedTypePathDeferredReason::Import => {
                        typokat_library_product::QualifiedPathDeferredReason::Import
                    }
                    QualifiedTypePathDeferredReason::Enum => {
                        typokat_library_product::QualifiedPathDeferredReason::Enum
                    }
                }),
                segment,
            ),
        },
        QualifiedPathRawResolutionForTest::StableTypeGroup(_)
        | QualifiedPathRawResolutionForTest::StableValue(_)
        | QualifiedPathRawResolutionForTest::TypeParameter
        | QualifiedPathRawResolutionForTest::IntrinsicGlobalValue => {
            QualifiedPathResolutionOutcome::Resolved
        }
        QualifiedPathRawResolutionForTest::MissingTypeRoot
        | QualifiedPathRawResolutionForTest::MissingValueRoot => {
            unresolved(QualifiedPathUnresolvedCause::MissingRoot, 0)
        }
        QualifiedPathRawResolutionForTest::TypeOnlyValueRoot => {
            unresolved(QualifiedPathUnresolvedCause::TypeOnlyRoot, 0)
        }
        QualifiedPathRawResolutionForTest::ImportQualifier => {
            QualifiedPathResolutionOutcome::Unresolved {
                cause: QualifiedPathUnresolvedCause::Deferred(
                    typokat_library_product::QualifiedPathDeferredReason::Import,
                ),
                failing_segment_ordinal: 0,
            }
        }
    }
}

fn expected_qualified_path_tables(
    product: &LibraryDeclarationProduct,
    paths: &[AstQualifiedPath],
    oracle: &[QualifiedPathRawOracleRowForTest],
) -> (Vec<QualifiedPathRecipe>, Vec<QualifiedPathSegmentRecipe>) {
    let mut headers = Vec::with_capacity(paths.len());
    let mut segments = Vec::new();
    for path in paths {
        let recipe_index = recipe_index_for_ast_path(product, path);
        let recipe = product
            .type_recipe_key_at(recipe_index)
            .expect("qualified path recipe key");
        let (role, names) = recipe_path_keys(&product.types()[recipe_index])
            .expect("qualified product recipe shape");
        assert_eq!(role, path.role);
        assert_eq!(names.len(), path.segments.len());
        headers.push(QualifiedPathRecipe {
            recipe,
            role,
            resolution: binder_outcome_for_ast_path(path, oracle),
        });
        segments.extend(path.segments.iter().enumerate().map(|(ordinal, segment)| {
            QualifiedPathSegmentRecipe {
                recipe,
                ordinal: u32::try_from(ordinal).expect("qualified path ordinal fits u32"),
                name: names[ordinal],
                site: typokat_library_product::SourceSite {
                    source: product.types()[recipe_index].site.source,
                    start: segment.start,
                    end: segment.end,
                },
            }
        }));
    }
    headers.sort_by_key(|row| row.recipe.index());
    segments.sort_by_key(|row| (row.recipe.index(), row.ordinal));
    (headers, segments)
}

fn assert_binder_oracle_matches_ast_domain(
    paths: &[AstQualifiedPath],
    oracle: &[QualifiedPathRawOracleRowForTest],
) {
    let import_qualifiers = paths
        .iter()
        .filter(|path| path.role == QualifiedPathRole::ImportQualifier)
        .count();
    assert_eq!(
        import_qualifiers, 0,
        "authenticated profile import qualifiers"
    );
    assert_eq!(oracle.len(), paths.len());
    for path in paths {
        let _ = binder_outcome_for_ast_path(path, oracle);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QualifiedPathContractError {
    MissingPath,
    DuplicatePath,
    WrongRole,
    WrongCause,
    WrongFailingSegmentOrdinal,
    MissingSegment(u32),
    DuplicateSegment(u32),
    ExtraSegment,
    OutOfOrder { position: u32, ordinal: u32 },
    WrongName(u32),
    WrongSource(u32),
    WrongSpan(u32),
    SegmentOutsideRecipe(u32),
    WrongSourceText(u32),
}

fn validate_one_qualified_path(
    product: &LibraryDeclarationProduct,
    profile: &ExactLibraryProfile,
    expected: &AstQualifiedPath,
    expected_outcome: QualifiedPathResolutionOutcome,
    recipe_index: usize,
    paths: &[QualifiedPathRecipe],
    segments: &[QualifiedPathSegmentRecipe],
) -> Result<(), QualifiedPathContractError> {
    let matching_paths = paths
        .iter()
        .filter(|path| path.recipe.index() == recipe_index)
        .collect::<Vec<_>>();
    match matching_paths.len() {
        0 => return Err(QualifiedPathContractError::MissingPath),
        1 => {}
        _ => return Err(QualifiedPathContractError::DuplicatePath),
    }
    if matching_paths[0].role != expected.role {
        return Err(QualifiedPathContractError::WrongRole);
    }
    match (matching_paths[0].resolution, expected_outcome) {
        (QualifiedPathResolutionOutcome::Resolved, QualifiedPathResolutionOutcome::Resolved) => {}
        (
            QualifiedPathResolutionOutcome::Unresolved {
                cause: actual_cause,
                failing_segment_ordinal: actual_ordinal,
            },
            QualifiedPathResolutionOutcome::Unresolved {
                cause: expected_cause,
                failing_segment_ordinal: expected_ordinal,
            },
        ) if actual_cause == expected_cause && actual_ordinal == expected_ordinal => {}
        (
            QualifiedPathResolutionOutcome::Unresolved {
                cause: actual_cause,
                ..
            },
            QualifiedPathResolutionOutcome::Unresolved {
                cause: expected_cause,
                ..
            },
        ) if actual_cause != expected_cause => return Err(QualifiedPathContractError::WrongCause),
        (
            QualifiedPathResolutionOutcome::Unresolved { .. },
            QualifiedPathResolutionOutcome::Unresolved { .. },
        ) => return Err(QualifiedPathContractError::WrongFailingSegmentOrdinal),
        _ => return Err(QualifiedPathContractError::WrongCause),
    }

    let matching_segments = segments
        .iter()
        .filter(|segment| segment.recipe.index() == recipe_index)
        .collect::<Vec<_>>();
    for ordinal in 0..expected.segments.len() {
        let count = matching_segments
            .iter()
            .filter(|segment| usize::try_from(segment.ordinal).ok() == Some(ordinal))
            .count();
        let ordinal = u32::try_from(ordinal).expect("path ordinal fits u32");
        match count {
            0 => return Err(QualifiedPathContractError::MissingSegment(ordinal)),
            1 => {}
            _ => return Err(QualifiedPathContractError::DuplicateSegment(ordinal)),
        }
    }
    if matching_segments.len() > expected.segments.len() {
        return Err(QualifiedPathContractError::ExtraSegment);
    }
    for (position, segment) in matching_segments.iter().enumerate() {
        if usize::try_from(segment.ordinal).ok() != Some(position) {
            return Err(QualifiedPathContractError::OutOfOrder {
                position: u32::try_from(position).expect("path position fits u32"),
                ordinal: segment.ordinal,
            });
        }
        let expected_segment = &expected.segments[position];
        let name = product
            .strings()
            .get(segment.name.index())
            .map(String::as_str);
        if name != Some(expected_segment.name.as_str()) {
            return Err(QualifiedPathContractError::WrongName(segment.ordinal));
        }
        let recipe = &product.types()[recipe_index];
        if segment.site.source != recipe.site.source {
            return Err(QualifiedPathContractError::WrongSource(segment.ordinal));
        }
        if segment.site.start < recipe.site.start || segment.site.end > recipe.site.end {
            return Err(QualifiedPathContractError::SegmentOutsideRecipe(
                segment.ordinal,
            ));
        }
        if segment.site.start != expected_segment.start || segment.site.end != expected_segment.end
        {
            return Err(QualifiedPathContractError::WrongSpan(segment.ordinal));
        }
        let source = product.sources()[segment.site.source.index()].ordinal;
        let source = usize::try_from(source).expect("source ordinal fits usize");
        let source_text = profile
            .sources()
            .iter()
            .find(|candidate| candidate.ordinal().index() == source)
            .and_then(|candidate| std::str::from_utf8(candidate.bytes()).ok())
            .expect("product source exists in authenticated profile");
        let start = usize::try_from(segment.site.start).expect("source start fits usize");
        let end = usize::try_from(segment.site.end).expect("source end fits usize");
        if source_text.get(start..end) != Some(expected_segment.name.as_str()) {
            return Err(QualifiedPathContractError::WrongSourceText(segment.ordinal));
        }
    }
    Ok(())
}

fn rows_digest<T: std::fmt::Debug>(rows: &[T]) -> String {
    let rendered = rows
        .iter()
        .enumerate()
        .map(|(row, value)| format!("{row}|{value:?}\n"))
        .collect::<String>();
    typokat_library_product::sha256_hex(rendered.as_bytes())
}

impl ProductFieldVisitor for ProductFieldCensus {
    fn visit_field(
        &mut self,
        class: ProductFieldClass,
        _table: &'static str,
        _row: usize,
        _field: &'static str,
    ) {
        match class {
            ProductFieldClass::OwnedText => self.owned_text += 1,
            ProductFieldClass::ProductKey => self.product_key += 1,
            ProductFieldClass::ProductKeyList => self.product_key_list += 1,
            ProductFieldClass::Scalar => self.scalar += 1,
        }
    }
}

fn iterator_object_global_path<'a>(
    profile: &ExactLibraryProfile,
    paths: &'a [AstQualifiedPath],
) -> &'a AstQualifiedPath {
    let source_ordinal = profile
        .sources()
        .iter()
        .find(|source| source.name() == "lib.es2025.iterator.d.ts")
        .expect("iterator library source")
        .ordinal()
        .index();
    let matches = paths
        .iter()
        .filter(|path| {
            path.source_ordinal == source_ordinal
                && path.role == QualifiedPathRole::Reference
                && path
                    .segments
                    .iter()
                    .map(|segment| segment.name.as_str())
                    .eq(["globalThis", "IteratorObject"])
        })
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1, "exact IteratorObject heritage path");
    matches[0]
}

fn missing_root_diagnostic(path_segment_count: usize) -> u32 {
    if path_segment_count > 1 {
        2503
    } else {
        2304
    }
}

#[test]
fn qualified_path_schema_authenticates_production_tables() {
    let profile = ExactLibraryProfile::load_packaged().expect("packaged profile");
    let (product, binder_oracle) =
        build_packaged_declaration_product_with_qualified_path_oracle_for_test()
            .expect("packaged declaration product and binder oracle");
    let ast_paths = independent_qualified_paths(&profile);
    assert_binder_oracle_matches_ast_domain(&ast_paths, &binder_oracle);
    let (paths, segments) = expected_qualified_path_tables(&product, &ast_paths, &binder_oracle);
    assert!(!paths.is_empty());
    assert!(!segments.is_empty());

    assert_eq!(product.qualified_path_rows(), paths);
    assert_eq!(product.qualified_path_segment_rows(), segments);
    product
        .validate()
        .expect("complete qualified-path tables validate");
    let receipt = product.audit_receipt();
    assert_eq!(receipt.qualified_path_count, paths.len());
    assert_eq!(receipt.qualified_path_segment_count, segments.len());
    let canonical = product.canonical_audit_render();
    assert_eq!(
        canonical
            .lines()
            .filter(|line| line.starts_with("qualified_path|"))
            .count(),
        paths.len()
    );
    assert_eq!(
        canonical
            .lines()
            .filter(|line| line.starts_with("qualified_path_segment|"))
            .count(),
        segments.len()
    );
    let mut qualified_digest_input = String::new();
    for (row, path) in paths.iter().enumerate() {
        qualified_digest_input.push_str(&format!("path|{row}|{path:?}\n"));
    }
    for (row, segment) in segments.iter().enumerate() {
        qualified_digest_input.push_str(&format!("segment|{row}|{segment:?}\n"));
    }
    assert_eq!(
        receipt.qualified_path_digest,
        typokat_library_product::sha256_hex(qualified_digest_input.as_bytes())
    );

    let iterator_path = iterator_object_global_path(&profile, &ast_paths);
    let iterator_recipe = recipe_index_for_ast_path(&product, iterator_path);
    let iterator_outcome = binder_outcome_for_ast_path(iterator_path, &binder_oracle);
    let iterator_headers = paths
        .iter()
        .copied()
        .filter(|path| path.recipe.index() == iterator_recipe)
        .collect::<Vec<_>>();
    let iterator_segments = segments
        .iter()
        .copied()
        .filter(|segment| segment.recipe.index() == iterator_recipe)
        .collect::<Vec<_>>();
    let mut wrong_cause = iterator_headers.clone();
    wrong_cause[0].resolution = QualifiedPathResolutionOutcome::Unresolved {
        cause: QualifiedPathUnresolvedCause::MissingMember,
        failing_segment_ordinal: 0,
    };
    assert_eq!(
        validate_one_qualified_path(
            &product,
            &profile,
            iterator_path,
            iterator_outcome,
            iterator_recipe,
            &wrong_cause,
            &iterator_segments,
        ),
        Err(QualifiedPathContractError::WrongCause)
    );
    let mut wrong_cause_table = paths.clone();
    wrong_cause_table
        .iter_mut()
        .find(|path| path.recipe.index() == iterator_recipe)
        .expect("IteratorObject path")
        .resolution = wrong_cause[0].resolution;
    let validation = typokat_library_product::validate_with_qualified_path_tables_for_test(
        &product,
        wrong_cause_table,
        segments.clone(),
    );
    assert!(matches!(
        validation,
        Err(
            typokat_library_product::ProductValidationError::QualifiedPathResolutionMismatch { .. }
        )
    ));
    let mut wrong_ordinal = iterator_headers;
    wrong_ordinal[0].resolution = QualifiedPathResolutionOutcome::Unresolved {
        cause: QualifiedPathUnresolvedCause::MissingRoot,
        failing_segment_ordinal: 1,
    };
    assert_eq!(
        validate_one_qualified_path(
            &product,
            &profile,
            iterator_path,
            iterator_outcome,
            iterator_recipe,
            &wrong_ordinal,
            &iterator_segments,
        ),
        Err(QualifiedPathContractError::WrongFailingSegmentOrdinal)
    );
    let mut wrong_ordinal_table = paths.clone();
    wrong_ordinal_table
        .iter_mut()
        .find(|path| path.recipe.index() == iterator_recipe)
        .expect("IteratorObject path")
        .resolution = wrong_ordinal[0].resolution;
    let validation = typokat_library_product::validate_with_qualified_path_tables_for_test(
        &product,
        wrong_ordinal_table,
        segments.clone(),
    );
    assert!(matches!(
        validation,
        Err(
            typokat_library_product::ProductValidationError::QualifiedPathResolutionMismatch { .. }
        )
    ));

    let mut missing_header = paths.clone();
    let missing_recipe = missing_header.remove(0).recipe.index();
    let validation = typokat_library_product::validate_with_qualified_path_tables_for_test(
        &product,
        missing_header,
        segments.clone(),
    );
    assert_eq!(
        validation,
        Err(
            typokat_library_product::ProductValidationError::MissingQualifiedPath {
                recipe: missing_recipe,
            }
        )
    );

    let mut duplicate_header = paths.clone();
    duplicate_header.insert(1, duplicate_header[0]);
    let validation = typokat_library_product::validate_with_qualified_path_tables_for_test(
        &product,
        duplicate_header,
        segments.clone(),
    );
    assert_eq!(
        validation,
        Err(
            typokat_library_product::ProductValidationError::DuplicateQualifiedPath {
                table: "qualified_paths",
                row: 1,
            }
        )
    );

    let mut wrong_role = paths.clone();
    wrong_role[0].role = match wrong_role[0].role {
        QualifiedPathRole::Reference => QualifiedPathRole::Query,
        QualifiedPathRole::Query | QualifiedPathRole::ImportQualifier => {
            QualifiedPathRole::Reference
        }
    };
    let validation = typokat_library_product::validate_with_qualified_path_tables_for_test(
        &product,
        wrong_role,
        segments.clone(),
    );
    assert_eq!(
        validation,
        Err(
            typokat_library_product::ProductValidationError::InvalidQualifiedPath {
                table: "qualified_paths",
                row: 0,
            }
        )
    );

    let mut missing_segment = segments.clone();
    missing_segment.remove(0);
    let validation = typokat_library_product::validate_with_qualified_path_tables_for_test(
        &product,
        paths.clone(),
        missing_segment,
    );
    assert!(matches!(
        validation,
        Err(
            typokat_library_product::ProductValidationError::InvalidQualifiedPath {
                table: "qualified_path_segments",
                ..
            }
        )
    ));

    let mut duplicate_segment = segments.clone();
    duplicate_segment.insert(1, duplicate_segment[0]);
    let validation = typokat_library_product::validate_with_qualified_path_tables_for_test(
        &product,
        paths.clone(),
        duplicate_segment,
    );
    assert!(matches!(
        validation,
        Err(
            typokat_library_product::ProductValidationError::DuplicateQualifiedPath {
                table: "qualified_path_segments",
                ..
            }
        )
    ));

    let mut extra_segment = segments.clone();
    let mut extra = extra_segment[0];
    extra.ordinal = extra.ordinal.saturating_add(1);
    extra_segment.insert(1, extra);
    let validation = typokat_library_product::validate_with_qualified_path_tables_for_test(
        &product,
        paths.clone(),
        extra_segment,
    );
    assert!(validation.is_err());

    let mut invalid_span = segments.clone();
    invalid_span[0].site.end = invalid_span[0].site.start;
    let validation = typokat_library_product::validate_with_qualified_path_tables_for_test(
        &product,
        paths,
        invalid_span,
    );
    assert_eq!(
        validation,
        Err(
            typokat_library_product::ProductValidationError::InvalidQualifiedPath {
                table: "qualified_path_segments",
                row: 0,
            }
        )
    );
}

#[test]
fn qualified_path_outcome_normalization_preserves_unrepresented_binder_causes() {
    assert_eq!(
        normalize_qualified_value_path_resolution_for_test(
            QualifiedValuePathResolution::NamespaceOnlyLeaf { segment: 2 }
        ),
        QualifiedPathResolutionOutcome::Unresolved {
            cause: QualifiedPathUnresolvedCause::NamespaceOnlyLeaf,
            failing_segment_ordinal: 2,
        }
    );
    assert_eq!(
        normalize_qualified_type_path_resolution_for_test(QualifiedTypePathResolution::Deferred {
            segment: 1,
            reason: QualifiedTypePathDeferredReason::Import,
        }),
        QualifiedPathResolutionOutcome::Unresolved {
            cause: QualifiedPathUnresolvedCause::Deferred(
                typokat_library_product::QualifiedPathDeferredReason::Import,
            ),
            failing_segment_ordinal: 1,
        }
    );
    assert_eq!(
        normalize_qualified_type_path_resolution_for_test(QualifiedTypePathResolution::Deferred {
            segment: 3,
            reason: QualifiedTypePathDeferredReason::Enum,
        }),
        QualifiedPathResolutionOutcome::Unresolved {
            cause: QualifiedPathUnresolvedCause::Deferred(
                typokat_library_product::QualifiedPathDeferredReason::Enum,
            ),
            failing_segment_ordinal: 3,
        }
    );
}

#[test]
fn packaged_iterator_reference_has_exact_segment_sites_and_tk2503_outcome() {
    let profile = ExactLibraryProfile::load_packaged().expect("packaged profile");
    let (product, binder_oracle) =
        build_packaged_declaration_product_with_qualified_path_oracle_for_test()
            .expect("packaged declaration product and binder oracle");
    let ast_paths = independent_qualified_paths(&profile);
    assert_binder_oracle_matches_ast_domain(&ast_paths, &binder_oracle);
    let expected = iterator_object_global_path(&profile, &ast_paths);
    let recipe_index = recipe_index_for_ast_path(&product, expected);
    let expected_outcome = binder_outcome_for_ast_path(expected, &binder_oracle);
    assert_eq!(
        expected_outcome,
        QualifiedPathResolutionOutcome::Unresolved {
            cause: QualifiedPathUnresolvedCause::MissingRoot,
            failing_segment_ordinal: 0,
        }
    );
    assert!(
        !product.qualified_path_rows().is_empty(),
        "RED: missing qualified-path table"
    );
    assert!(
        !product.qualified_path_segment_rows().is_empty(),
        "RED: missing qualified-path segment table"
    );

    validate_one_qualified_path(
        &product,
        &profile,
        expected,
        expected_outcome,
        recipe_index,
        product.qualified_path_rows(),
        product.qualified_path_segment_rows(),
    )
    .expect("exact AST-derived IteratorObject provenance");

    let path = product
        .qualified_path_rows()
        .iter()
        .find(|path| path.recipe.index() == recipe_index)
        .expect("IteratorObject path row");
    assert_eq!(
        path.resolution,
        QualifiedPathResolutionOutcome::Unresolved {
            cause: QualifiedPathUnresolvedCause::MissingRoot,
            failing_segment_ordinal: 0,
        }
    );
    let diagnostic = missing_root_diagnostic(expected.segments.len());
    assert_eq!(diagnostic, 2503);
    assert_eq!(missing_root_diagnostic(1), 2304);
    let TypeRecipeNode::Reference { resolution, .. } = &product.types()[recipe_index].node else {
        panic!("IteratorObject recipe is a reference");
    };
    assert!(matches!(
        resolution,
        TypeReferenceResolution::Unresolved { region: _ }
    ));
    assert_eq!(expected.segments[0].name, "globalThis");
    assert_eq!(expected.segments[1].name, "IteratorObject");
}

#[test]
fn every_qualified_product_path_has_one_ast_derived_site_per_segment() {
    let profile = ExactLibraryProfile::load_packaged().expect("packaged profile");
    let (product, binder_oracle) =
        build_packaged_declaration_product_with_qualified_path_oracle_for_test()
            .expect("packaged declaration product and binder oracle");
    let expected = independent_qualified_paths(&profile);
    assert!(!expected.is_empty(), "packaged profile has qualified paths");
    assert_binder_oracle_matches_ast_domain(&expected, &binder_oracle);
    let unresolved = binder_oracle
        .iter()
        .filter(|row| {
            matches!(
                independently_normalize_raw_resolution(row.resolution),
                QualifiedPathResolutionOutcome::Unresolved { .. }
            )
        })
        .collect::<Vec<_>>();
    let mut unresolved_projection = unresolved
        .iter()
        .map(|row| {
            (
                row.source_ordinal,
                row.start,
                row.end,
                row.path.clone(),
                independently_normalize_raw_resolution(row.resolution),
            )
        })
        .collect::<Vec<_>>();
    unresolved_projection.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
    assert_eq!(
        unresolved_projection,
        vec![
            (
                0,
                8_340,
                8_341,
                vec!["U".to_owned()],
                QualifiedPathResolutionOutcome::Unresolved {
                    cause: QualifiedPathUnresolvedCause::MissingRoot,
                    failing_segment_ordinal: 0,
                },
            ),
            (
                76,
                1_658,
                1_702,
                vec!["globalThis".to_owned(), "IteratorObject".to_owned()],
                QualifiedPathResolutionOutcome::Unresolved {
                    cause: QualifiedPathUnresolvedCause::MissingRoot,
                    failing_segment_ordinal: 0,
                },
            ),
            (
                76,
                8_155,
                8_180,
                vec!["IteratorObjectConstructor".to_owned()],
                QualifiedPathResolutionOutcome::Unresolved {
                    cause: QualifiedPathUnresolvedCause::MissingRoot,
                    failing_segment_ordinal: 0,
                },
            ),
        ]
    );
    let mut expected_recipe_indices = expected
        .iter()
        .map(|path| recipe_index_for_ast_path(&product, path))
        .collect::<Vec<_>>();
    expected_recipe_indices.sort_unstable();
    assert!(
        expected_recipe_indices
            .windows(2)
            .all(|pair| pair[0] != pair[1]),
        "one AST qualified path per product recipe"
    );
    assert!(
        !product.qualified_path_rows().is_empty(),
        "RED: missing qualified-path table"
    );
    assert!(
        !product.qualified_path_segment_rows().is_empty(),
        "RED: missing qualified-path segment table"
    );

    let mut actual_recipe_indices = product
        .qualified_path_rows()
        .iter()
        .map(|path| path.recipe.index())
        .collect::<Vec<_>>();
    assert!(
        actual_recipe_indices
            .windows(2)
            .all(|pair| pair[0] < pair[1]),
        "qualified-path rows use canonical recipe order"
    );
    actual_recipe_indices.sort_unstable();
    assert_eq!(actual_recipe_indices, expected_recipe_indices);

    for path in &expected {
        let recipe_index = recipe_index_for_ast_path(&product, path);
        validate_one_qualified_path(
            &product,
            &profile,
            path,
            binder_outcome_for_ast_path(path, &binder_oracle),
            recipe_index,
            product.qualified_path_rows(),
            product.qualified_path_segment_rows(),
        )
        .unwrap_or_else(|error| panic!("qualified path {recipe_index}: {error:?}"));
        let resolution = product
            .qualified_path_rows()
            .iter()
            .find(|row| row.recipe.index() == recipe_index)
            .expect("qualified path resolution");
        if let QualifiedPathResolutionOutcome::Unresolved {
            failing_segment_ordinal,
            ..
        } = resolution.resolution
        {
            assert!(
                usize::try_from(failing_segment_ordinal)
                    .is_ok_and(|ordinal| ordinal < path.segments.len()),
                "unresolved ordinal belongs to its path"
            );
        }
    }

    assert!(
        product
            .qualified_path_segment_rows()
            .windows(2)
            .all(|pair| (pair[0].recipe.index(), pair[0].ordinal)
                < (pair[1].recipe.index(), pair[1].ordinal)),
        "segment rows use canonical recipe and ordinal order"
    );
    assert_eq!(
        product.qualified_path_segment_rows().len(),
        expected
            .iter()
            .map(|path| path.segments.len())
            .sum::<usize>()
    );
}

#[test]
fn qualified_path_contract_rejects_segment_provenance_mutations() {
    let profile = ExactLibraryProfile::load_packaged().expect("packaged profile");
    let (product, binder_oracle) =
        build_packaged_declaration_product_with_qualified_path_oracle_for_test()
            .expect("packaged declaration product and binder oracle");
    let ast_paths = independent_qualified_paths(&profile);
    assert_binder_oracle_matches_ast_domain(&ast_paths, &binder_oracle);
    let expected = iterator_object_global_path(&profile, &ast_paths);
    let recipe_index = recipe_index_for_ast_path(&product, expected);
    let expected_outcome = binder_outcome_for_ast_path(expected, &binder_oracle);
    assert!(
        !product.qualified_path_segment_rows().is_empty(),
        "RED: missing qualified-path segment table"
    );
    let paths = product
        .qualified_path_rows()
        .iter()
        .copied()
        .filter(|path| path.recipe.index() == recipe_index)
        .collect::<Vec<_>>();
    let segments = product
        .qualified_path_segment_rows()
        .iter()
        .copied()
        .filter(|segment| segment.recipe.index() == recipe_index)
        .collect::<Vec<_>>();
    validate_one_qualified_path(
        &product,
        &profile,
        expected,
        expected_outcome,
        recipe_index,
        &paths,
        &segments,
    )
    .expect("unmodified provenance validates");
    assert_eq!(segments.len(), 2);

    assert_eq!(
        validate_one_qualified_path(
            &product,
            &profile,
            expected,
            expected_outcome,
            recipe_index,
            &[],
            &segments
        ),
        Err(QualifiedPathContractError::MissingPath)
    );

    let mut duplicate_path = paths.clone();
    duplicate_path.push(paths[0]);
    assert_eq!(
        validate_one_qualified_path(
            &product,
            &profile,
            expected,
            expected_outcome,
            recipe_index,
            &duplicate_path,
            &segments
        ),
        Err(QualifiedPathContractError::DuplicatePath)
    );

    let mut wrong_role = paths.clone();
    wrong_role[0].role = QualifiedPathRole::Query;
    assert_eq!(
        validate_one_qualified_path(
            &product,
            &profile,
            expected,
            expected_outcome,
            recipe_index,
            &wrong_role,
            &segments
        ),
        Err(QualifiedPathContractError::WrongRole)
    );

    let mut wrong_cause = paths.clone();
    wrong_cause[0].resolution = QualifiedPathResolutionOutcome::Unresolved {
        cause: QualifiedPathUnresolvedCause::MissingMember,
        failing_segment_ordinal: 0,
    };
    assert_eq!(
        validate_one_qualified_path(
            &product,
            &profile,
            expected,
            expected_outcome,
            recipe_index,
            &wrong_cause,
            &segments
        ),
        Err(QualifiedPathContractError::WrongCause)
    );

    let mut wrong_failing_ordinal = paths.clone();
    wrong_failing_ordinal[0].resolution = QualifiedPathResolutionOutcome::Unresolved {
        cause: QualifiedPathUnresolvedCause::MissingRoot,
        failing_segment_ordinal: 1,
    };
    assert_eq!(
        validate_one_qualified_path(
            &product,
            &profile,
            expected,
            expected_outcome,
            recipe_index,
            &wrong_failing_ordinal,
            &segments
        ),
        Err(QualifiedPathContractError::WrongFailingSegmentOrdinal)
    );

    let mut missing = segments.clone();
    missing.remove(0);
    assert_eq!(
        validate_one_qualified_path(
            &product,
            &profile,
            expected,
            expected_outcome,
            recipe_index,
            &paths,
            &missing
        ),
        Err(QualifiedPathContractError::MissingSegment(0))
    );

    let mut duplicate = segments.clone();
    duplicate.push(segments[0]);
    assert_eq!(
        validate_one_qualified_path(
            &product,
            &profile,
            expected,
            expected_outcome,
            recipe_index,
            &paths,
            &duplicate
        ),
        Err(QualifiedPathContractError::DuplicateSegment(0))
    );

    let mut out_of_order = segments.clone();
    out_of_order.swap(0, 1);
    assert_eq!(
        validate_one_qualified_path(
            &product,
            &profile,
            expected,
            expected_outcome,
            recipe_index,
            &paths,
            &out_of_order
        ),
        Err(QualifiedPathContractError::OutOfOrder {
            position: 0,
            ordinal: 1,
        })
    );

    let mut extra = segments.clone();
    let mut extra_segment = segments[1];
    extra_segment.ordinal = 2;
    extra.push(extra_segment);
    assert_eq!(
        validate_one_qualified_path(
            &product,
            &profile,
            expected,
            expected_outcome,
            recipe_index,
            &paths,
            &extra
        ),
        Err(QualifiedPathContractError::ExtraSegment)
    );

    let mut wrong_name = segments.clone();
    wrong_name[0].name = segments[1].name;
    assert_eq!(
        validate_one_qualified_path(
            &product,
            &profile,
            expected,
            expected_outcome,
            recipe_index,
            &paths,
            &wrong_name
        ),
        Err(QualifiedPathContractError::WrongName(0))
    );

    let mut wrong_source = segments.clone();
    wrong_source[0].site.source = product
        .source_key_at(0)
        .filter(|source| *source != wrong_source[0].site.source)
        .unwrap_or_else(|| product.source_key_at(1).expect("second product source"));
    assert_eq!(
        validate_one_qualified_path(
            &product,
            &profile,
            expected,
            expected_outcome,
            recipe_index,
            &paths,
            &wrong_source
        ),
        Err(QualifiedPathContractError::WrongSource(0))
    );

    let mut invalid_span = segments.clone();
    invalid_span[0].site.start = invalid_span[0].site.start.saturating_add(1);
    assert_eq!(
        validate_one_qualified_path(
            &product,
            &profile,
            expected,
            expected_outcome,
            recipe_index,
            &paths,
            &invalid_span
        ),
        Err(QualifiedPathContractError::WrongSpan(0))
    );
}

#[test]
fn product_receipt_matches_authenticated_edge_and_attachment_oracles() {
    let product = build_packaged_declaration_product().expect("packaged declaration product");
    product.validate().expect("closed product validates");
    let receipt = product.audit_receipt();
    let mut walked = ProductFieldCensus::default();
    product.walk_fields(&mut walked);
    assert!(walked.owned_text > 0);
    assert!(walked.product_key > 0);
    assert!(walked.product_key_list > 0);
    assert!(walked.scalar > 0);
    eprintln!("{receipt:#?}");
    assert_eq!(receipt.source_count, 82);
    assert_eq!(receipt.container_count, 6);
    assert_eq!(receipt.declaration_count, 13_958);
    assert_eq!(receipt.group_count, 3_406);
    assert_eq!(receipt.region_count, 97);
    assert_eq!(receipt.callable_scope_count, 153);
    assert_eq!(receipt.type_parameter_count, 1_044);
    assert_eq!(receipt.type_recipe_count, 41_853);
    assert_eq!(receipt.signature_count, 6_683);
    assert_eq!(receipt.member_count, 14_234);
    assert_eq!(receipt.lexical_step_count, 26_364);
    assert_eq!(receipt.dependency_edge_count, 16_451);
    assert_eq!(receipt.namespace_attachment_count, 14);
    assert_eq!(receipt.value_attachment_count, 1_318);
    assert_eq!(receipt.qualified_path_count, 16_310);
    assert_eq!(receipt.qualified_path_segment_count, 16_345);
    assert_eq!(receipt.index_signature_parameter_count, 72);
    assert_eq!(receipt.computed_property_name_count, 141);
    assert_eq!(receipt.constructor_count, 2);
    assert_eq!(receipt.private_constructor_count, 2);
    assert_eq!(receipt.abstract_class_count, 1);
    assert_eq!(receipt.abstract_member_count, 1);
    assert_eq!(receipt.resolved_group_reference_count, 14_087);
    assert_eq!(receipt.type_parameter_reference_count, 2_207);
    assert_eq!(receipt.unresolved_reference_count, 3);
    assert_eq!(receipt.resolved_group_dependency_count, 14_235);
    assert_eq!(receipt.type_parameter_dependency_count, 2_207);
    assert_eq!(receipt.unresolved_dependency_count, 3);
    assert_eq!(receipt.canonical_bytes, 35_707_223);
    assert_eq!(
        receipt.canonical_digest,
        "5a3b85f7f056f642c784b2f871b2df813f59e583742b549ad0f1519cefa97335"
    );
    assert_eq!(
        receipt.dependency_edge_digest,
        "9804f3438bf2293a705a71fc4238f18665b0f2a1cf34589a56e04f238f98598a"
    );
    assert_eq!(
        receipt.dependency_scc_digest,
        "a52538ebfe3231d1345ca23343421fac509afde19df35ce1f9635710c7837856"
    );
    assert_eq!(
        receipt.lexical_order_digest,
        "a622952bc63c20ebf7243543051fb0a899fd83d4875cb922af1f14eecf813917"
    );
    assert_eq!(
        receipt.callable_scope_digest,
        "34820efc0bdc8e3ff7e9a004b8b00c01715ca8331f789da6e3582c42c3b77c6c"
    );
    assert_eq!(
        receipt.namespace_attachment_digest,
        "44af5e5f929565fb59dd7eea773256729227f002bb8b82fd402856cbdf4cb606"
    );
    assert_eq!(
        receipt.value_attachment_digest,
        "0a60c72c1311a124160dace3837d43358ce7e0736c00a7ec8a430626b8caa848"
    );
    assert_eq!(
        receipt.qualified_path_digest,
        "1507b19431928d673075dbe54431dc483326738197766ca5bf0f6e44758d06ab"
    );

    assert!(product.groups().iter().any(|group| {
        let mut sources = group
            .fragments
            .iter()
            .map(|fragment| product.declarations()[fragment.index()].site.source.index())
            .collect::<Vec<_>>();
        sources.sort_unstable();
        sources.dedup();
        sources.len() > 1
            && group.fragments.iter().all(|fragment| {
                let declaration = &product.declarations()[fragment.index()];
                product.regions()[declaration.region.index()].container == group.container
            })
    }));
    assert!(product
        .containers()
        .iter()
        .enumerate()
        .filter(|(_, container)| {
            matches!(
                container.kind,
                ContainerKind::Namespace | ContainerKind::AmbientExternalModule
            )
        })
        .any(|(container, _)| {
            let mut sources = product
                .regions()
                .iter()
                .filter(|region| region.container.index() == container)
                .map(|region| region.site.source.index())
                .collect::<Vec<_>>();
            sources.sort_unstable();
            sources.dedup();
            sources.len() > 1
        }));

    let lib_es5 = product
        .sources()
        .iter()
        .position(|source| product.strings()[source.name.index()] == "lib.es5.d.ts")
        .expect("lib.es5 source");
    for helper in [
        "ThisParameterType",
        "OmitThisParameter",
        "Parameters",
        "ConstructorParameters",
    ] {
        let declaration = product
            .declarations()
            .iter()
            .find(|declaration| {
                declaration.site.source.index() == lib_es5
                    && product.strings()[declaration.name.index()] == helper
            })
            .unwrap_or_else(|| panic!("missing {helper}"));
        let mut resolved_parameters = 0usize;
        let mut unresolved_infer_names = 0usize;
        for recipe in product.types().iter().filter(|recipe| {
            recipe.site.source.index() == lib_es5
                && declaration.site.start <= recipe.site.start
                && recipe.site.end <= declaration.site.end
        }) {
            if let TypeRecipeNode::Reference {
                path,
                resolution,
                arguments: _,
                dependency: _,
            } = &recipe.node
            {
                resolved_parameters += usize::from(matches!(
                    resolution,
                    TypeReferenceResolution::TypeParameter(_)
                ));
                unresolved_infer_names += usize::from(
                    matches!(
                        resolution,
                        TypeReferenceResolution::Unresolved { region: _ }
                    ) && path
                        .iter()
                        .any(|name| matches!(product.strings()[name.index()].as_str(), "P" | "R")),
                );
            }
        }
        assert!(
            resolved_parameters > 0,
            "{helper} has no type-parameter refs"
        );
        assert_eq!(unresolved_infer_names, 0, "{helper} leaked infer P/R");
    }
}

#[test]
fn trusted_intrinsic_aliases_have_exact_typed_marker_census() {
    let product = build_packaged_declaration_product().expect("packaged declaration product");
    product.validate().expect("closed product validates");
    let expected = expected_trusted_intrinsic_markers(&product);
    assert_eq!(product.intrinsic_marker_rows(), expected);
}

#[test]
fn trusted_intrinsic_alias_markers_are_canonically_authenticated() {
    let product = build_packaged_declaration_product().expect("packaged declaration product");
    product.validate().expect("closed product validates");
    let expected = expected_trusted_intrinsic_markers(&product);
    let canonical = product.canonical_audit_render();
    let actual_rows = canonical
        .lines()
        .filter(|row| row.starts_with("intrinsic_marker|"))
        .collect::<Vec<_>>();
    let expected_rows = expected
        .iter()
        .enumerate()
        .map(|(index, marker)| format!("intrinsic_marker|{index}|{marker:?}"))
        .collect::<Vec<_>>();
    assert_eq!(actual_rows, expected_rows);
    assert_eq!(
        product.audit_receipt().canonical_digest,
        typokat_library_product::sha256_hex(canonical.as_bytes())
    );
}

fn expected_trusted_intrinsic_markers(
    product: &LibraryDeclarationProduct,
) -> Vec<IntrinsicMarkerRecipe> {
    [
        (
            "lib.es5.d.ts",
            "OmitThisParameter",
            IntrinsicMarker::OmitThisParameter,
        ),
        ("lib.es5.d.ts", "Uppercase", IntrinsicMarker::Uppercase),
        ("lib.es5.d.ts", "Lowercase", IntrinsicMarker::Lowercase),
        ("lib.es5.d.ts", "Capitalize", IntrinsicMarker::Capitalize),
        (
            "lib.es5.d.ts",
            "Uncapitalize",
            IntrinsicMarker::Uncapitalize,
        ),
        ("lib.es5.d.ts", "NoInfer", IntrinsicMarker::NoInfer),
        ("lib.es5.d.ts", "ThisType", IntrinsicMarker::ThisType),
        (
            "lib.es2015.iterable.d.ts",
            "BuiltinIteratorReturn",
            IntrinsicMarker::BuiltinIteratorReturn,
        ),
    ]
    .into_iter()
    .map(|(source_name, alias, marker)| {
        let source = product
            .sources()
            .iter()
            .position(|source| product.strings()[source.name.index()] == source_name)
            .unwrap_or_else(|| panic!("missing {source_name}"));
        let declaration = product
            .declarations()
            .iter()
            .position(|declaration| {
                declaration.site.source.index() == source
                    && product.strings()[declaration.name.index()] == alias
            })
            .and_then(|index| product.declaration_key_at(index))
            .unwrap_or_else(|| panic!("missing trusted alias {source_name}:{alias}"));
        IntrinsicMarkerRecipe {
            declaration,
            marker,
        }
    })
    .collect()
}

#[test]
fn product_dependency_rows_match_the_independent_ast_oracle() {
    let profile = ExactLibraryProfile::load_packaged().expect("packaged profile");
    let product = build_packaged_declaration_product().expect("packaged declaration product");

    let mut syntax_rows = Vec::new();
    for source in profile.sources().iter() {
        let allocator = Allocator::default();
        let source_text = std::str::from_utf8(source.bytes()).expect("packaged source is UTF-8");
        let parsed = Parser::new(&allocator, source_text, SourceType::d_ts()).parse();
        assert!(!parsed.panicked, "{} parse panicked", source.name());
        assert!(
            parsed.diagnostics.is_empty(),
            "{} parse diagnostics: {:?}",
            source.name(),
            parsed.diagnostics
        );
        let mut visitor = IndependentDependencyVisitor {
            source_ordinal: source.ordinal().index(),
            rows: Vec::new(),
        };
        visitor.visit_program(&parsed.program);
        syntax_rows.extend(visitor.rows);
    }
    syntax_rows.sort();
    let syntax_counts =
        syntax_rows
            .iter()
            .fold(BTreeMap::<&'static str, usize>::new(), |mut counts, row| {
                *counts.entry(row.kind).or_default() += 1;
                counts
            });
    assert_eq!(syntax_counts.get("reference"), Some(&15_429));
    assert_eq!(syntax_counts.get("heritage"), Some(&868));
    assert_eq!(syntax_counts.get("query"), Some(&(13 + 141)));
    assert_eq!(syntax_rows.len(), 15_429 + 868 + 13 + 141);

    let mut product_dependency_rows = product
        .dependency_rows()
        .iter()
        .map(|dependency| StableSyntaxDependencyRow {
            source_ordinal: usize::try_from(
                product.sources()[dependency.site.source.index()].ordinal,
            )
            .expect("source ordinal fits usize"),
            start: dependency.site.start,
            end: dependency.site.end,
            kind: match dependency.kind {
                DependencyKind::Reference => "reference",
                DependencyKind::Heritage => "heritage",
                DependencyKind::Query => "query",
                DependencyKind::Import => "import",
                DependencyKind::IndexedAccess => "indexed-access",
                DependencyKind::Constraint => "constraint",
            },
        })
        .collect::<Vec<_>>();
    product_dependency_rows.sort();
    assert_eq!(product_dependency_rows, syntax_rows);
    assert_eq!(
        rows_digest(&product_dependency_rows),
        rows_digest(&syntax_rows)
    );
}

#[test]
fn product_builds_one_manifest_per_source() {
    let scope = SourceDeclarationManifestBuildScopeForTest::start();
    let product = build_packaged_declaration_product().expect("packaged declaration product");
    assert_eq!(scope.finish(), 82);
    assert_eq!(product.source_count(), 82);
}

#[test]
fn product_drops_ast_allocator_and_binder() {
    let provider = super::provider::LibraryBaseProvider::new();
    let product = provider.get().expect("packaged declaration product");
    let second_owner = provider.get().expect("cached declaration product");

    assert_owned_product(&product);
    assert!(Arc::ptr_eq(&product, &second_owner));
    drop(product);
    assert_eq!(second_owner.source_count(), 82);
}

#[test]
fn product_is_repeat_reverse_shuffle_and_process_deterministic() {
    if std::env::var_os("TYPOKAT_PRODUCT_CHILD").is_some() {
        let product = build_packaged_declaration_product().expect("child product");
        println!(
            "child-product-digest={}",
            product.audit_receipt().canonical_digest
        );
        return;
    }
    let first = build_packaged_declaration_product().expect("first product");
    let second = build_packaged_declaration_product().expect("second product");
    let reverse = build_packaged_declaration_product_with_order_for_test(true, false, false)
        .expect("reverse product");
    let shuffle = build_packaged_declaration_product_with_order_for_test(false, true, false)
        .expect("shuffle product");
    let perturbation = (0..10_000)
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    let perturbed = build_packaged_declaration_product_with_order_for_test(true, true, true)
        .expect("perturbed product");
    drop(perturbation);
    assert_eq!(first.audit_receipt(), second.audit_receipt());
    assert_eq!(first.audit_receipt(), reverse.audit_receipt());
    assert_eq!(first.audit_receipt(), shuffle.audit_receipt());
    assert_eq!(first.audit_receipt(), perturbed.audit_receipt());
    let child = std::process::Command::new(std::env::current_exe().expect("test executable"))
        .args(["library_declaration_product_integration_spec::product_is_repeat_reverse_shuffle_and_process_deterministic", "--exact", "--nocapture"])
        .env("TYPOKAT_PRODUCT_CHILD", "1")
        .output()
        .expect("fresh product process");
    assert!(
        child.status.success(),
        "fresh process failed: {}",
        String::from_utf8_lossy(&child.stderr)
    );
    assert!(
        String::from_utf8_lossy(&child.stdout).contains(&first.audit_receipt().canonical_digest)
    );
}

#[test]
fn product_rejects_forbidden_schema_controls() {
    let product = build_packaged_declaration_product().expect("packaged declaration product");
    product.validate().expect("packaged product validates");
    let mut broken = typokat_library_product::ProductAssembly::new();
    let root = broken.next_region_key().expect("reserved root");
    let name = broken.intern("broken.d.ts").expect("name");
    let digest = broken.intern("00").expect("digest");
    broken
        .add_source(typokat_library_product::SourceRecipe {
            ordinal: 0,
            name,
            sha256: digest,
            byte_len: 0,
            classification: typokat_library_product::SourceClassification::Script,
            root_region: root,
        })
        .expect("source");
    assert!(broken.finish("broken").is_err());
}
