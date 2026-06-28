use crate::matrix::DomainOps;

pub(crate) struct DocDomain;

impl DomainOps for DocDomain {
    fn domain(&self) -> &'static str {
        "doc"
    }
    // doc-api is a read-only cargo-doc analysis surface with no entity store, so
    // every operation falls through to the default blocked-with-reason cell.
}
