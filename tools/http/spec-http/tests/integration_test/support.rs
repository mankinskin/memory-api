use spec_api::{
    SpecManifest,
    SpecStore,
};
use spec_http::{
    SpecAppState,
    build_router,
};

pub(super) fn make_app(dir: &std::path::Path) -> axum::Router {
    let mut store = SpecStore::open(dir).expect("open spec store");
    let specs_dir = dir.join("specs");
    std::fs::create_dir_all(&specs_dir).unwrap();
    store
        .entity_store()
        .add_scan_root(memory_api::model::filesystem::ScanRoot {
            path: specs_dir,
            label: "default".into(),
        })
        .expect("add scan root");
    store.scan(false).expect("initial scan");
    let state = SpecAppState::new(store);
    build_router(state)
}

pub(super) fn seed_spec(
    dir: &std::path::Path,
    slug: &str,
    title: &str,
) -> String {
    let mut store = SpecStore::open(dir).expect("open store");
    let specs_dir = dir.join("specs");
    store
        .entity_store()
        .add_scan_root(memory_api::model::filesystem::ScanRoot {
            path: specs_dir.clone(),
            label: "default".into(),
        })
        .expect("add scan root");
    store.scan(false).ok();
    let manifest = SpecManifest::new(slug, title, "test-component");
    let id = store
        .create(&manifest, "# Test body", Some(&specs_dir))
        .expect("create spec");
    id.to_string()
}
