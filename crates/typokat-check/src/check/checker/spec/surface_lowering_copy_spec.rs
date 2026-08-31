//! Contract: lowering namespace value surfaces borrows the declaration tables.
//!
//! The tables are per-row deep structures that grow with the whole compilation unit, so a
//! copy taken per lowered member is quadratic in (declarations x members) and invisible to
//! every output assertion. Only a work counter can see it.

use super::context::CopiedDeclRowScopeForTest;
use super::library_compiler::{run_injected_profile, InjectedLibrarySource};
use crate::source::LibraryFileOrdinal;

/// Both dimensions are the ones a per-member copy multiplies together.
const DECLARATIONS: usize = 128;
const NAMESPACE_MEMBERS: usize = 64;

/// Room for two whole-table copies. `publish_class_surfaces` still takes one per publication
/// pass — that one is independent of member count — while a copy per lowered member costs
/// `DECLARATIONS` rows each and blows the budget by more than an order of magnitude.
fn copied_decl_row_budget() -> u64 {
    u64::try_from(DECLARATIONS)
        .unwrap_or(u64::MAX)
        .saturating_mul(2)
}

/// Enough declarations for a table copy to dominate, and enough standalone namespace members
/// — annotated values and functions, the two lowering entries — to take one copy each.
fn library_source() -> String {
    let mut source = String::new();
    for index in 0..DECLARATIONS {
        source.push_str(&format!(
            "interface Shape{index:03} {{ value: string; tag: number }}\n"
        ));
    }
    source.push_str("declare namespace Surfaces {\n");
    for index in 0..NAMESPACE_MEMBERS {
        let shape = index % DECLARATIONS;
        source.push_str(&format!(
            "    export const value{index:03}: Shape{shape:03};\n"
        ));
        source.push_str(&format!(
            "    export function read{index:03}(input: Shape{shape:03}): number;\n"
        ));
    }
    source.push_str("}\n");
    source
}

#[test]
fn standalone_namespace_surface_lowering_copies_no_declaration_rows() {
    let source = library_source();
    let scope = CopiedDeclRowScopeForTest::start();
    let run = run_injected_profile(&[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "surface-lowering-copy.d.ts",
        source: &source,
    }])
    .expect("surface-lowering copy fixture compiles");
    let copied = scope.finish();

    // A silently degraded compile would lower nothing and copy nothing, so pin the work too.
    let surfaces = run
        .global_value_probe("Surfaces")
        .expect("standalone namespace publishes its value root");
    assert_eq!(surfaces.member_names.len(), NAMESPACE_MEMBERS * 2);
    assert!(run.library_records.is_empty(), "{:?}", run.library_records);

    let budget = copied_decl_row_budget();
    assert!(
        copied <= budget,
        "publishing {NAMESPACE_MEMBERS} namespace members over {DECLARATIONS} declarations \
         copied {copied} declaration rows (budget {budget})"
    );
}
