use std::path::{Path, PathBuf};

use concurrent::{Format, Graph, Ir, IrNode};

#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    par_supported: bool,
}

const CASES: &[Case] = &[
    Case {
        name: "01_sequence",
        par_supported: true,
    },
    Case {
        name: "02_sequence_basic",
        par_supported: true,
    },
    Case {
        name: "03_parallel",
        par_supported: true,
    },
    Case {
        name: "04_parallel_v2",
        par_supported: false,
    },
    Case {
        name: "05_parallel_v3",
        par_supported: true,
    },
    Case {
        name: "06_nested_seq",
        par_supported: true,
    },
    Case {
        name: "07_dependencies",
        par_supported: false,
    },
    Case {
        name: "08_terminal",
        par_supported: false,
    },
    Case {
        name: "09_complex",
        par_supported: false,
    },
];

fn fixture_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(rel)
}

fn read_fixture(rel: &str) -> String {
    std::fs::read_to_string(fixture_path(rel))
        .unwrap_or_else(|e| panic!("Failed to read fixture {rel}: {e}"))
}

fn normalize_ir(input: &str) -> String {
    Graph::<IrNode, Ir>::parse(input)
        .unwrap_or_else(|e| panic!("Failed to parse IR: {e}"))
        .to_string()
}

fn load_case_ir(case: Case) -> String {
    read_fixture(&format!("ir/valid/{}.graph", case.name))
}

#[test]
fn fk_roundtrip_from_ir_fixtures() {
    for case in CASES {
        let ir_src = load_case_ir(*case);
        let expected = normalize_ir(&ir_src);
        let ir = Graph::<IrNode, Ir>::parse(&ir_src).expect("IR parse failed");
        let fk_text = ir.to_fk().to_string();
        let ir_from_fk =
            concurrent::parse(&fk_text, Format::ForkJoin).expect("Failed to parse fk back into IR");

        assert_eq!(ir_from_fk.to_string(), expected, "case: {}", case.name);
    }
}

#[test]
fn par_roundtrip_from_ir_fixtures() {
    for case in CASES.iter().copied().filter(|c| c.par_supported) {
        let ir_src = load_case_ir(case);
        let expected = normalize_ir(&ir_src);
        let ir = Graph::<IrNode, Ir>::parse(&ir_src).expect("IR parse failed");
        let par_text = ir.to_par().expect("IR to par failed").to_string();
        let ir_from_par =
            concurrent::parse(&par_text, Format::Par).expect("Failed to parse par back into IR");

        assert_eq!(ir_from_par.to_string(), expected, "case: {}", case.name);
    }
}

#[test]
fn par_rejects_dependencies() {
    for case in CASES.iter().copied().filter(|c| !c.par_supported) {
        let ir_src = load_case_ir(case);
        let ir = Graph::<IrNode, Ir>::parse(&ir_src).expect("IR parse failed");
        assert!(
            ir.to_par().is_err(),
            "expected par conversion to fail for case: {}",
            case.name
        );
    }
}
