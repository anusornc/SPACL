//! Storage backends for OWL2 ontologies

pub trait StorageBackend {
    fn save(&self, ontology: &crate::core::ontology::Ontology) -> crate::core::error::OwlResult<()>;
    fn load(&self) -> crate::core::error::OwlResult<crate::core::ontology::Ontology>;
}
