# Problem

`RenderTarget` currently acts as a flat filter plus output path. That is not expressive enough to describe a document outline, per-section composition, or explicit ordering within a file.

# Scope

Design a hierarchical target schema for `rule-api` that represents document structure explicitly.

The design should cover:

- ordered outline nodes
- node-local filters
- exact file binding and path scoping
- node metadata needed for rendering or preview
- compatibility or migration rules from the flat target format

# Acceptance Criteria

- The target schema can represent a document as an ordered tree of nodes.
- Each node has explicit ordering semantics instead of relying on global entry ordering alone.
- Each node can define a local filter over rule metadata.
- The schema makes it obvious which portions of a file are assembled from which rule subsets.
- A migration story exists for existing flat `RenderTarget` configurations.
