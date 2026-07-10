use super::WorkspaceRegistry;
use std::{
    sync::{
        Arc,
        Barrier,
    },
    thread,
};

#[test]
fn concurrent_get_returns_shared_store_instance() {
    let dir = tempfile::tempdir().expect("create tempdir");
    let registry =
        Arc::new(WorkspaceRegistry::single(dir.path().to_path_buf()));
    let primary_workspace = registry.primary_workspace_name().to_string();

    let workers = 8usize;
    let barrier = Arc::new(Barrier::new(workers));
    let mut handles = Vec::with_capacity(workers);

    for _ in 0..workers {
        let registry = Arc::clone(&registry);
        let primary_workspace = primary_workspace.clone();
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            barrier.wait();
            registry
                .get(&primary_workspace)
                .expect("workspace should open")
        }));
    }

    let first = handles
        .remove(0)
        .join()
        .expect("thread should join without panic");

    for handle in handles {
        let store = handle.join().expect("thread should join without panic");
        assert!(
            Arc::ptr_eq(&first, &store),
            "all concurrent gets should return the same cached store instance"
        );
    }
}
