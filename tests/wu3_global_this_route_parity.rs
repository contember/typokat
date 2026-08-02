//! Public-route parity for direct script declarations that conflict with `globalThis`.

use typokat::driver::{check_project, check_source, CheckOutput};
use typokat::frontend::FileInput;

const SOURCE: &str = r#"
class globalThis {
    static invented: number;
}

const invented: number = globalThis.invented;
const absolute: number = globalThis.Math.abs(-1);
"#;

fn diagnostic_codes(output: &CheckOutput) -> Vec<String> {
    let mut codes = output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str().to_owned())
        .collect::<Vec<_>>();
    codes.sort_unstable();
    codes
}

fn assert_complete(label: &str, output: &CheckOutput) {
    assert!(
        output.parse_errors.is_empty(),
        "{label} produced parse errors: {:?}",
        output.parse_errors
    );
    assert!(
        output.incomplete.is_empty(),
        "{label} produced incomplete surfaces: {:?}",
        output.incomplete
    );
}

#[test]
fn direct_global_this_class_has_identical_single_and_project_surfaces() -> Result<(), String> {
    let single = check_source(SOURCE).map_err(|error| error.to_string())?;
    let reports = check_project(vec![FileInput {
        name: "/wu3/direct-global-this-class.ts".to_owned(),
        source: SOURCE.to_owned(),
    }])
    .map_err(|error| error.to_string())?;
    let [project] = reports.as_slice() else {
        return Err(format!(
            "one project input produced {} reports",
            reports.len()
        ));
    };

    assert_complete("check_source", &single);
    assert_complete("check_project", &project.output);

    let single_codes = diagnostic_codes(&single);
    let project_codes = diagnostic_codes(&project.output);
    let expected = ["TK2397".to_owned()];
    assert!(
        single_codes == expected && project_codes == expected,
        "both routes must report exactly TK2397: check_source={single_codes:?}, check_project={project_codes:?}"
    );
    assert!(
        !single_codes.iter().any(|code| code == "TK2339")
            && !project_codes.iter().any(|code| code == "TK2339"),
        "neither route may lose the invented static or built-in Math surface"
    );
    assert_eq!(
        single_codes, project_codes,
        "public single-source and one-file project diagnostic multisets must match"
    );
    Ok(())
}
