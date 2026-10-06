//! Attribute mistakes and unsupported shapes are compile errors, pinned by
//! `trybuild` snapshots under `tests/ui`. Regenerate a snapshot after an
//! intended message change with `TRYBUILD=overwrite`.

#[test]
fn rejected_derive_inputs_fail_to_compile() {
    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
