use spec_api::{
    SpecManifest,
    SpecStore,
};
use spec_http::{
    SpecAppState,
    build_router,
};

fn open_or_init_store(dir: &std::path::Path) -> SpecStore {
    let store_root = dir.join(".spec");
    if store_root.exists() {
        SpecStore::open(&store_root).expect("open spec store")
    } else {
        std::fs::create_dir_all(&store_root).expect("create spec store root");
        SpecStore::init(&store_root).expect("init spec store")
    }
}

fn ensure_scan_root(
    store: &SpecStore,
    specs_dir: &std::path::Path,
) {
    let has_root = store
        .entity_store()
        .list_scan_roots()
        .expect("list scan roots")
        .into_iter()
        .any(|root| root.path == specs_dir);

    if !has_root {
        store
            .entity_store()
            .add_scan_root(memory_kernel::model::filesystem::ScanRoot {
                path: specs_dir.to_path_buf(),
                label: "default".into(),
            })
            .expect("add scan root");
    }
}

pub(super) fn make_app(dir: &std::path::Path) -> axum::Router {
    let mut store = open_or_init_store(dir);
    let specs_dir = dir.join("specs");
    std::fs::create_dir_all(&specs_dir).unwrap();
    ensure_scan_root(&store, &specs_dir);
    store.scan(false).expect("initial scan");
    let state = SpecAppState::new(store);
    build_router(state)
}

pub(super) fn seed_spec(
    dir: &std::path::Path,
    slug: &str,
    title: &str,
) -> String {
    let mut store = open_or_init_store(dir);
    let specs_dir = dir.join("specs");
    std::fs::create_dir_all(&specs_dir).unwrap();
    ensure_scan_root(&store, &specs_dir);
    store.scan(false).ok();
    let manifest = SpecManifest::new(slug, title, "test-component");
    let id = store
        .create(&manifest, "# Test body", Some(&specs_dir))
        .expect("create spec");
    id.to_string()
}
