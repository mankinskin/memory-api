## Why one owning root

The store's append-only history and materialised index belong to exactly one workspace. Allowing a single entity to be visible from two stores would break the resolver, the edge index, and `spec sync-generated`'s output paths.