use typokat::binder::namespace::SourceFileKind;
use typokat::driver::FileInput;
use typokat::library::compiler::LibraryCompiler;
use typokat::library::profile::ExactLibraryProfile;
use typokat::library::LibraryBaseProvider;

#[test]
fn non_test_library_consumes_the_checkpoint_continuation_seam() {
    let profile = ExactLibraryProfile::load_packaged().expect("pinned library profile");
    let checkpoint = LibraryCompiler::new()
        .compile_binder_checkpoint(&profile)
        .expect("release library compiler produces one opaque binder checkpoint");
    assert_eq!(checkpoint.library_unit_count(), 82);

    let provider = LibraryBaseProvider::new();
    let continuation = provider
        .continue_library_project_binder(
            checkpoint,
            vec![
                FileInput {
                    name: "/release-probe/00_kind_only_external.mts".to_owned(),
                    source: "interface ReleaseProbeModuleLocal { value: number }\n".to_owned(),
                },
                FileInput {
                    name: "/release-probe/10_script.ts".to_owned(),
                    source: "namespace ReleaseProbeScript { export const value = 1; }\n".to_owned(),
                },
            ],
        )
        .expect("release provider consumes the compiled checkpoint");

    assert_eq!(continuation.library_unit_count(), 82);
    assert_eq!(continuation.project_unit_count(), 2);
    assert_eq!(
        continuation.project_source_kind("/release-probe/00_kind_only_external.mts"),
        Some(SourceFileKind::ImplementationMts)
    );
    assert_eq!(
        continuation.project_source_is_external("/release-probe/00_kind_only_external.mts"),
        Some(true)
    );
}
