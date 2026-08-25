use super::profile::ExactLibraryProfile;
use crate::binder::bind::ProjectBinderBuilder;
use crate::binder::declaration::SourceDeclarationManifest;
use crate::binder::namespace::{
    source_file_kind, CompilationUnit, ModuleBindingContext, SourceUnitKey,
};
use crate::binder::product::{
    project_library_declaration_product, CertifiedIntrinsicMarkerEvidence, LibraryProductSource,
};
#[cfg(test)]
use crate::binder::product::{
    project_library_declaration_product_with_qualified_path_oracle_for_test,
    QualifiedPathRawOracleRowForTest,
};
use crate::source::{CompilationOrigin, LibraryFileOrdinal};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub enum LibraryDeclarationProductBuildError {
    Profile(super::profile::LibraryProfileError),
    SourceNotUtf8 {
        ordinal: usize,
    },
    Parse {
        ordinal: usize,
        messages: Vec<String>,
    },
    Bind(crate::binder::bind::LibraryBinderError),
    Projection(crate::binder::product::LibraryProductProjectionError),
}

impl std::fmt::Display for LibraryDeclarationProductBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "default-library declaration product failed: {self:?}"
        )
    }
}
impl std::error::Error for LibraryDeclarationProductBuildError {}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn build_packaged_declaration_product() -> Result<
    Arc<typokat_library_product::LibraryDeclarationProduct>,
    LibraryDeclarationProductBuildError,
> {
    let profile = ExactLibraryProfile::load_packaged()
        .map_err(LibraryDeclarationProductBuildError::Profile)?;
    build_declaration_product(&profile).map(Arc::new)
}

fn build_declaration_product(
    profile: &ExactLibraryProfile,
) -> Result<typokat_library_product::LibraryDeclarationProduct, LibraryDeclarationProductBuildError>
{
    let intrinsic_markers = packaged_intrinsic_marker_evidence(profile);
    build_declaration_product_from_sources(
        profile.profile_identity(),
        profile.sources().iter().collect(),
        &intrinsic_markers,
        false,
    )
}

fn packaged_intrinsic_marker_evidence(
    _verified_profile: &ExactLibraryProfile,
) -> [CertifiedIntrinsicMarkerEvidence; 6] {
    use typokat_library_product::IntrinsicMarker;

    // These byte sites are trusted only after the packaged profile verifies its source bytes.
    [
        CertifiedIntrinsicMarkerEvidence {
            source_ordinal: 0,
            binding_start: 75_505,
            marker: IntrinsicMarker::Uppercase,
        },
        CertifiedIntrinsicMarkerEvidence {
            source_ordinal: 0,
            binding_start: 75_604,
            marker: IntrinsicMarker::Lowercase,
        },
        CertifiedIntrinsicMarkerEvidence {
            source_ordinal: 0,
            binding_start: 75_722,
            marker: IntrinsicMarker::Capitalize,
        },
        CertifiedIntrinsicMarkerEvidence {
            source_ordinal: 0,
            binding_start: 75_841,
            marker: IntrinsicMarker::Uncapitalize,
        },
        CertifiedIntrinsicMarkerEvidence {
            source_ordinal: 0,
            binding_start: 75_941,
            marker: IntrinsicMarker::NoInfer,
        },
        CertifiedIntrinsicMarkerEvidence {
            source_ordinal: 20,
            binding_start: 2_502,
            marker: IntrinsicMarker::BuiltinIteratorReturn,
        },
    ]
}

fn build_declaration_product_from_sources(
    profile_identity: &str,
    sources: Vec<&super::profile::ExactLibrarySource>,
    certified_intrinsic_markers: &[CertifiedIntrinsicMarkerEvidence],
    perturb_allocations: bool,
) -> Result<typokat_library_product::LibraryDeclarationProduct, LibraryDeclarationProductBuildError>
{
    build_declaration_product_from_sources_using(
        profile_identity,
        sources,
        certified_intrinsic_markers,
        perturb_allocations,
        project_library_declaration_product,
    )
}

fn build_declaration_product_from_sources_using<R>(
    profile_identity: &str,
    mut sources: Vec<&super::profile::ExactLibrarySource>,
    certified_intrinsic_markers: &[CertifiedIntrinsicMarkerEvidence],
    perturb_allocations: bool,
    projector: impl FnOnce(
        &str,
        &crate::binder::Binder,
        &[LibraryProductSource<'_, '_>],
        &[CertifiedIntrinsicMarkerEvidence],
    ) -> Result<R, crate::binder::product::LibraryProductProjectionError>,
) -> Result<R, LibraryDeclarationProductBuildError> {
    sources.sort_by_key(|source| source.ordinal());
    let texts = sources
        .iter()
        .map(|source| {
            std::str::from_utf8(source.bytes()).map_err(|_| {
                LibraryDeclarationProductBuildError::SourceNotUtf8 {
                    ordinal: source.ordinal().index(),
                }
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let source_hashes = sources
        .iter()
        .map(|source| typokat_library_product::sha256_hex(source.bytes()))
        .collect::<Vec<_>>();
    let allocators = (0..sources.len())
        .map(|_| Allocator::default())
        .collect::<Vec<_>>();
    let parsed = allocators
        .iter()
        .zip(&sources)
        .zip(&texts)
        .map(|((allocator, source), text)| {
            let parsed = Parser::new(allocator, text, SourceType::d_ts()).parse();
            if parsed.panicked || !parsed.diagnostics.is_empty() {
                return Err(LibraryDeclarationProductBuildError::Parse {
                    ordinal: source.ordinal().index(),
                    messages: parsed
                        .diagnostics
                        .iter()
                        .map(|diagnostic| format!("{diagnostic:?}"))
                        .collect(),
                });
            }
            Ok(parsed)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let manifests = parsed
        .iter()
        .map(|parsed| SourceDeclarationManifest::build(&parsed.program))
        .collect::<Vec<_>>();
    let prelude_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::d_ts()).parse();
    let mut builder = ProjectBinderBuilder::new_empty_library_prelude(&prelude.program);
    #[cfg(test)]
    if perturb_allocations {
        builder.perturb_product_allocations_for_test(257);
    }
    #[cfg(not(test))]
    let _ = perturb_allocations;
    let units = parsed
        .iter()
        .zip(&manifests)
        .zip(&sources)
        .map(|((parsed, manifest), source)| {
            let source_key = SourceUnitKey(
                u32::try_from(source.ordinal().index().saturating_add(1)).unwrap_or(u32::MAX),
            );
            let unit = CompilationUnit {
                source: source_key,
                origin: CompilationOrigin::Library(LibraryFileOrdinal::new(
                    source.ordinal().index(),
                )),
                binding: ModuleBindingContext::for_program(
                    &parsed.program,
                    source_file_kind(source.name()),
                ),
            };
            (&parsed.program, manifest, unit)
        })
        .collect::<Vec<_>>();
    let modules = builder
        .try_add_library_modules_with_declaration_manifests(&units)
        .map_err(LibraryDeclarationProductBuildError::Bind)?;
    let final_module = modules.last().copied().ok_or({
        LibraryDeclarationProductBuildError::Bind(
            crate::binder::bind::LibraryBinderError::EmptyBatch,
        )
    })?;
    let binder = builder.finish(final_module);
    let inputs = sources
        .iter()
        .zip(&texts)
        .zip(&source_hashes)
        .zip(&manifests)
        .zip(&parsed)
        .zip(modules)
        .map(
            |(((((source, text), source_hash), manifest), parsed), module)| LibraryProductSource {
                ordinal: u32::try_from(source.ordinal().index()).unwrap_or(u32::MAX),
                name: source.name(),
                source_sha256: source_hash,
                byte_len: u32::try_from(source.bytes().len()).unwrap_or(u32::MAX),
                external_module: ModuleBindingContext::for_program(
                    &parsed.program,
                    source_file_kind(source.name()),
                )
                .external_module,
                module,
                manifest,
                source_text: text,
            },
        )
        .collect::<Vec<_>>();
    projector(
        profile_identity,
        &binder,
        &inputs,
        certified_intrinsic_markers,
    )
    .map_err(LibraryDeclarationProductBuildError::Projection)
}

#[cfg(test)]
pub(super) fn build_packaged_declaration_product_with_qualified_path_oracle_for_test() -> Result<
    (
        typokat_library_product::LibraryDeclarationProduct,
        Vec<QualifiedPathRawOracleRowForTest>,
    ),
    LibraryDeclarationProductBuildError,
> {
    let profile = ExactLibraryProfile::load_packaged()
        .map_err(LibraryDeclarationProductBuildError::Profile)?;
    let intrinsic_markers = packaged_intrinsic_marker_evidence(&profile);
    build_declaration_product_from_sources_using(
        profile.profile_identity(),
        profile.sources().iter().collect(),
        &intrinsic_markers,
        false,
        project_library_declaration_product_with_qualified_path_oracle_for_test,
    )
}

#[cfg(test)]
pub(super) fn build_packaged_declaration_product_with_order_for_test(
    reverse: bool,
    shuffle: bool,
    perturb_allocations: bool,
) -> Result<typokat_library_product::LibraryDeclarationProduct, LibraryDeclarationProductBuildError>
{
    let profile = ExactLibraryProfile::load_packaged()
        .map_err(LibraryDeclarationProductBuildError::Profile)?;
    let mut sources = profile.sources().iter().collect::<Vec<_>>();
    if reverse {
        sources.reverse();
    }
    if shuffle {
        let len = sources.len();
        for index in 0..len {
            sources.swap(index, (index.saturating_mul(37).saturating_add(11)) % len);
        }
    }
    let intrinsic_markers = packaged_intrinsic_marker_evidence(&profile);
    build_declaration_product_from_sources(
        profile.profile_identity(),
        sources,
        &intrinsic_markers,
        perturb_allocations,
    )
}
