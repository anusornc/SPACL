//! Common parser utilities
use crate::core::error::OwlResult;

pub fn sanitize_iri(iri: &str) -> OwlResult<String> {
    Ok(iri.to_string())
}
