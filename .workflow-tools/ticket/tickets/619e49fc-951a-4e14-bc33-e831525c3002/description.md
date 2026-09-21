---
tags: `#plan` `#context-trace` `#context-search` `#context-insert` `#context-read` `#debugging` `#testing` `#refactoring` `#api`
summary: Refactor `Hypergraph` to use per-vertex `RwLock` instead of a global `RwLock<Hypergraph>`, enabling concurrent reads during writes and fixing the t...
status: 📋
---

# Plan: Fine-Grained Per-Vertex Locking

## Objective
Refactor `Hypergraph` to use per-vertex `RwLock` instead of a global `RwLock<Hypergraph>`, enabling concurrent reads during writes and fixing the token string display issue during insert operations.

## Current State
- `HypergraphRef` = `Arc<RwLock<Hypergraph>>`
- `Hypergraph.graph` = `IndexMap<VertexKey, VertexData>`
- All mutations require `&mut self`
- Token string lookup fails during writes (try_read returns None)

## Target State
- `HypergraphRef` = `Arc<Hypergraph>` (no outer RwLock)
- `Hypergraph.graph` = `DashMap<VertexKey, VertexEntry>`
- `VertexEntry` = `RwLock<VertexData>` (per-vertex lock)
- Mutations use `&self` (interior mutability)
- Token strings readable anytime

## Files to Modify

### Core Changes
1. `Cargo.toml` - Add dashmap dependency
2. `graph/mod.rs` - Restructure Hypergraph and HypergraphRef
3. `graph/vertex/data/core.rs` - Keep VertexData as-is (no lock inside)
4. `graph/getters/vertex.rs` - Update VertexSet trait and impls
5. `graph/insert/*.rs` - Change &mut self to &self

### Secondary Changes
6. `graph/test_graph.rs` - Update GraphStringGetter impl
7. All callers of `graph.write()` / `graph.read()` in other crates

## Execution Steps

### Step 1: Add DashMap dependency
- [ ] Add `dashmap = "6"` to context-trace/Cargo.toml

### Step 2: Create VertexEntry wrapper
- [ ] Create new type `VertexEntry` containing `RwLock<VertexData>`
- [ ] Add helper methods for read/write access

### Step 3: Refactor Hypergraph internals
- [ ] Change `graph: IndexMap<VertexKey, VertexData>` to `graph: DashMap<VertexKey, VertexEntry>`
- [ ] Add `next_id: AtomicUsize` for lock-free ID allocation
- [ ] Update Default impl
- [ ] Update Clone impl (if needed)

### Step 4: Update getters
- [ ] Modify `VertexSet` trait - return guards instead of references
- [ ] Update `get_vertex`, `get_vertex_mut`, etc.
- [ ] Update `expect_vertex`, iterators

### Step 5: Update insert operations
- [ ] Change `&mut self` to `&self` on all insert methods
- [ ] Use DashMap's insert API
- [ ] Update parent reference updates to use per-vertex locks

### Step 6: Simplify HypergraphRef
- [ ] Change from `Arc<RwLock<Hypergraph>>` to `Arc<Hypergraph>`
- [ ] Remove Deref to RwLock, deref directly to Hypergraph
- [ ] Update all `.read()` / `.write()` call sites

### Step 7: Fix test_graph.rs
- [ ] Update GraphStringGetter to work with new structure
- [ ] Verify token strings work during inserts

### Step 8: Update dependent crates
- [ ] context-search
- [ ] context-insert
- [ ] context-read

## Risk Mitigation
- Keep old code commented initially for reference
- Test incrementally after each step
- Lock ordering: always acquire in VertexKey/VertexIndex order

## Validation
- Run `cargo test -p context-insert insert_postfix1`
- Verify token strings show in debug output during insert
- Run full test suite
