//! Contract: composing an interface's heritage bases tests member names by lookup.
//!
//! Every base surface is merged into one accumulated object, so testing each incoming
//! member against that object by scanning it costs `bases x members` per edge — quadratic
//! in the composed surface and invisible to every output assertion. Only a work counter
//! can see it.

use super::super::library_compiler::{run_injected_profile, InjectedLibrarySource};
use super::interface::HeritageBaseNameProbeScopeForTest;
use crate::source::LibraryFileOrdinal;

/// Both dimensions the scan multiplies together: the number of merged bases and the
/// members each one contributes.
const BASES: usize = 32;
const BASE_MEMBERS: usize = 32;

/// One probe per incoming member is the linear cost. Two of those leave room for the
/// composition to test a name twice; scanning the accumulated surface instead costs about
/// half of its square, two orders of magnitude past this budget.
fn heritage_base_name_probe_budget() -> u64 {
    u64::try_from(BASES * BASE_MEMBERS)
        .unwrap_or(u64::MAX)
        .saturating_mul(2)
}

/// Disjoint bases so the composed surface keeps every member and the accumulated object
/// grows with each merged edge — that growth is what a per-member scan re-reads.
fn library_source() -> String {
    let mut source = String::new();
    for base in 0..BASES {
        source.push_str(&format!("interface Base{base:03} {{"));
        for member in 0..BASE_MEMBERS {
            source.push_str(&format!(" member{base:03}_{member:03}: string;"));
        }
        source.push_str(" }\n");
    }
    source.push_str("interface Composed extends");
    for base in 0..BASES {
        let separator = if base == 0 { " " } else { ", " };
        source.push_str(&format!("{separator}Base{base:03}"));
    }
    source.push_str(" { own: number }\n");
    source
}

#[test]
fn composing_heritage_bases_probes_each_member_name_once() {
    let source = library_source();
    let scope = HeritageBaseNameProbeScopeForTest::start();
    let run = run_injected_profile(&[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "heritage-base-merge-scan.d.ts",
        source: &source,
    }])
    .expect("heritage-base merge fixture compiles");
    let probes = scope.finish();

    // A silently degraded compile would compose nothing and probe nothing, so pin the work too.
    let composed = run
        .global_type_probe("Composed")
        .expect("the derived interface publishes its composed surface");
    assert_eq!(composed.member_names.len(), BASES * BASE_MEMBERS + 1);
    assert!(run.library_records.is_empty(), "{:?}", run.library_records);

    let budget = heritage_base_name_probe_budget();
    assert!(
        probes <= budget,
        "composing {BASES} bases of {BASE_MEMBERS} members probed {probes} member names \
         (budget {budget})"
    );
}
