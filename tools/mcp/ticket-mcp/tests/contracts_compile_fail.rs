//! Compile-fail fixtures asserting AC5 of ticket 3d952036: omitting
//! `description_update` from a struct literal must fail to compile.

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
