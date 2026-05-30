## Why this matters

Without a shared resolver, every CLI/MCP/HTTP surface drifts in subtle ways (different minimum prefix lengths, different handling of slugs, different error codes). Pinning the resolver to `<x>-api` keeps the public contract identical across all transports.