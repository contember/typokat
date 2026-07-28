//! Disabled RED contract for backlog 105's fragment-attach work.
//!
//! Reopened type groups are already ordered. Adding one fragment must binary-insert under the
//! canonical key, not re-sort the complete group after every append.

use super::bind::{
    bind_module_with_prelude, TypeGroupFragmentAttachWorkForTest,
    TypeGroupFragmentAttachWorkScopeForTest,
};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

const SMALL: usize = 256;
const SCALED: usize = 1_024;
const ROW_PROBES_PER_FRAGMENT: u64 = 32;

fn source(fragments: usize) -> String {
    (0..fragments)
        .map(|index| format!("interface Merged {{ member{index}: number }}\n"))
        .collect()
}

fn measure(fragments: usize) -> TypeGroupFragmentAttachWorkForTest {
    let source = source(fragments);
    let prelude_allocator = Allocator::default();
    let source_allocator = Allocator::default();
    let prelude = Parser::new(&prelude_allocator, "", SourceType::ts()).parse();
    let parsed = Parser::new(&source_allocator, &source, SourceType::ts()).parse();
    assert!(prelude.diagnostics.is_empty());
    assert!(parsed.diagnostics.is_empty());

    let type_scope = TypeGroupFragmentAttachWorkScopeForTest::start();
    let binder = bind_module_with_prelude(&prelude.program, &parsed.program);
    let type_groups = type_scope.finish();

    let symbol = binder
        .graph
        .get(binder.compilation_global)
        .and_then(|scope| scope.lookup_local("Merged"))
        .expect("merged symbol");
    assert_eq!(
        binder
            .symbols
            .get(symbol)
            .expect("merged symbol row")
            .declarations
            .len(),
        fragments
    );
    type_groups
}

#[test]
fn fragment_attach_work_scales_with_fragments_not_group_size() {
    let step = u64::try_from(SCALED / SMALL).expect("scale step fits u64");
    let small = measure(SMALL).row_probes;
    let scaled = measure(SCALED).row_probes;
    assert!(small > 0, "type-group fragment attach records work");
    assert!(
        scaled <= ROW_PROBES_PER_FRAGMENT * u64::try_from(SCALED).unwrap_or(u64::MAX),
        "type-group attach exceeds the per-fragment budget: {small} -> {scaled}"
    );
    assert!(
        scaled <= 2 * step * small,
        "type-group attach grows with the group: {small} -> {scaled}"
    );
}
