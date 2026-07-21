//! Source-independent user route for a fully decoded WU0B base.

use super::wu0b_library::check_caller_certified_collision_free_source_with_owned_library;
use super::wu0b_snapshot::{
    record_decoded_route, DecodedLibraryBaseForTest, DecodedUserCheckResultForTest,
    IdentityRangeSetForTest, ReusedBaseShapeForTest,
};

pub(in crate::check::checker) fn check_source_with_decoded_base_for_test(
    base: DecodedLibraryBaseForTest,
    source: &str,
) -> DecodedUserCheckResultForTest {
    let projection = base.projection.clone();
    let prefixes = base.prefix_lengths.clone();
    let observed_classes = base.identity.classes.clone();
    record_decoded_route(&projection);
    match check_caller_certified_collision_free_source_with_owned_library(base.state, source) {
        Ok(run) => {
            let ends = &run.final_identity.ends;
            let ranges = IdentityRangeSetForTest {
                store: prefixes.types..ends.store,
                type_params: prefixes.type_params..ends.type_params,
                classes: prefixes.classes..ends.classes,
                scopes: prefixes.scopes..ends.scopes,
                symbols: prefixes.symbols..ends.symbols,
                declarations: prefixes.declarations..ends.declarations,
                type_groups: prefixes.type_groups..ends.type_groups,
                namespaces: prefixes.namespaces..ends.namespaces,
                value_storages: prefixes.value_storages..ends.value_storages,
            };
            DecodedUserCheckResultForTest {
                parse_errors: Vec::new(),
                diagnostics: run.result.diagnostics,
                incomplete: run.result.incomplete,
                base_projection_after_user_check: projection,
                user_identity_ranges: ranges,
                reused_base_shape: run
                    .final_identity
                    .reused_base_shape
                    .map(|witness| ReusedBaseShapeForTest::new(witness.type_id, witness.tag)),
                user_types: run.final_identity.named_alias_types,
                observed_classes,
            }
        }
        Err(message) => DecodedUserCheckResultForTest {
            parse_errors: vec![message],
            diagnostics: Vec::new(),
            incomplete: Vec::new(),
            base_projection_after_user_check: projection,
            user_identity_ranges: IdentityRangeSetForTest {
                store: prefixes.types..prefixes.types,
                type_params: prefixes.type_params..prefixes.type_params,
                classes: prefixes.classes..prefixes.classes,
                scopes: prefixes.scopes..prefixes.scopes,
                symbols: prefixes.symbols..prefixes.symbols,
                declarations: prefixes.declarations..prefixes.declarations,
                type_groups: prefixes.type_groups..prefixes.type_groups,
                namespaces: prefixes.namespaces..prefixes.namespaces,
                value_storages: prefixes.value_storages..prefixes.value_storages,
            },
            reused_base_shape: None,
            user_types: Default::default(),
            observed_classes,
        },
    }
}
