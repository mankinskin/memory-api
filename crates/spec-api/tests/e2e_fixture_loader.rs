use memory_fixtures::materialize_fixture;
use spec_api::store::SpecStore;

#[test]
fn spec_store_reads_seeded_specs_from_root_and_submodule_worktrees() {
    let fixture = materialize_fixture().expect("fixture should materialize");

    let root_store_root = fixture
        .store_root("spec-root")
        .expect("spec-root store path should exist");
    let mut root_store =
        SpecStore::open_or_init(root_store_root).expect("open_or_init root");
    root_store.scan(true).expect("scan root specs");

    let root_spec = root_store
        .get("fixture/root")
        .expect("seeded root spec should resolve by slug");
    assert_eq!(root_spec.title(), Some("Root fixture spec"));

    let submodule_store_root = fixture
        .store_root("spec-submodule-b")
        .expect("spec-submodule-b store path should exist");
    let mut submodule_store = SpecStore::open_or_init(submodule_store_root)
        .expect("open_or_init submodule");
    submodule_store.scan(true).expect("scan submodule specs");

    let submodule_spec = submodule_store
        .get("fixture/submodule-b")
        .expect("seeded submodule spec should resolve by slug");
    assert_eq!(submodule_spec.title(), Some("Submodule B fixture spec"));
}
