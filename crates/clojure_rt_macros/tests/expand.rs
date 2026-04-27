#[test]
fn macro_cases_compile() {
    let t = trybuild::TestCases::new();
    t.pass("tests/cases/register_type_basic.rs");
    t.pass("tests/cases/protocol_basic.rs");
    t.pass("tests/cases/implements_basic.rs");
}
