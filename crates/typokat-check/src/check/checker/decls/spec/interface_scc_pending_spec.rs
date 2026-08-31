//! RED contract: a frozen interface range must not rebuild heritage topology or SCCs.

use super::{
    interface_scc_construction_work_for_test, reset_interface_scc_construction_work_for_test,
    InterfaceSccConstructionWork,
};
use crate::check::checker::library_compiler::{
    check_caller_certified_collision_free_source_with_owned_library,
    compile_owned_injected_profile, InjectedLibrarySource,
};
use crate::diagnostics::DiagnosticCode;
use crate::source::LibraryFileOrdinal;

fn assert_pending_then_no_pending(work: &[InterfaceSccConstructionWork]) {
    assert_eq!(work.len(), 2, "one fill plus one publication recheck");

    let first = work[0];
    assert_eq!(first.topology_builds, 1);
    assert_eq!(first.scc_builds, 1);
    assert!(first.topology_declaration_scans > 0);
    assert!(first.scc_candidate_scans > 0);
    assert!(first.constructed_components > 0);

    let second = work[1];
    assert_eq!((second.start, second.end), (first.start, first.end));
    assert_eq!(
        second,
        InterfaceSccConstructionWork {
            start: first.start,
            end: first.end,
            ..InterfaceSccConstructionWork::default()
        },
        "a range with no pending interface must return before topology and SCC work"
    );
}

fn wide_interface_source(count: usize) -> String {
    let mut source = String::from("interface Root { root: number; }\n");
    for index in 0..count {
        source.push_str(&format!(
            "interface Layer{index} extends Root {{ own{index}: string; }}\n"
        ));
    }
    source
}

#[test]
fn source_library_publication_recheck_skips_frozen_interface_topology_and_sccs() {
    let source = wide_interface_source(256);
    reset_interface_scc_construction_work_for_test();
    let (run, _state) = compile_owned_injected_profile(&[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "wide.d.ts",
        source: &source,
    }])
    .expect("wide source-backed library compiles");

    assert!(run.library_records.is_empty(), "{:?}", run.library_records);
    let tail = run
        .global_type_probe("Layer255")
        .expect("last interface was constructed");
    assert_eq!(tail.member_names, ["own255", "root"]);
    assert_pending_then_no_pending(&interface_scc_construction_work_for_test());
}

#[test]
fn appended_user_range_still_constructs_after_the_frozen_prelude() {
    let mut source = String::from("interface UserRoot extends LibraryRoot {}\n");
    for index in 0..128 {
        source.push_str(&format!(
            "interface Layer{index} extends UserRoot {{ own{index}: string; }}\n"
        ));
    }
    source.push_str(
        "declare const value: Layer127;\n\
         const inherited: number = value.root;\n\
         const mismatch: boolean = value.own127;\n",
    );

    reset_interface_scc_construction_work_for_test();
    let (_run, state) = compile_owned_injected_profile(&[InjectedLibrarySource {
        file_ordinal: LibraryFileOrdinal::new(0),
        name: "prelude.d.ts",
        source: "interface LibraryRoot { root: number; }\n",
    }])
    .expect("source-backed prelude compiles");
    let output = check_caller_certified_collision_free_source_with_owned_library(state, &source)
        .expect("appended user range checks");
    assert_eq!(
        output
            .result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>(),
        [DiagnosticCode::TK2322],
        "the optimized construction schedule must preserve interface semantics"
    );

    let work = interface_scc_construction_work_for_test();
    assert_eq!(
        work.len(),
        4,
        "prelude and appended user ranges each fill then recheck"
    );
    assert_pending_then_no_pending(&work[..2]);
    assert_pending_then_no_pending(&work[2..]);
    assert!(
        work[2].start >= work[0].end,
        "the user range must append after the frozen prelude"
    );
}
