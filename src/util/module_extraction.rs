//! Lightweight module extraction utilities.
//!
//! This is intentionally conservative: we keep the full TBox and property axioms,
//! while dropping ABox assertions. It is meant to reduce memory pressure for
//! large ontologies in classification-style workloads.

use crate::core::error::OwlResult;
use crate::core::ontology::Ontology;
use crate::logic::axioms::Axiom;

/// Extract a TBox-focused module from an ontology.
///
/// This keeps:
/// - Class axioms (subclass, equivalence, disjointness)
/// - Property hierarchy + characteristics (object/data)
/// - Property domain/range axioms
/// - Property chains and keys
///
/// It drops ABox assertions (individual assertions), annotations, and imports.
pub fn extract_tbox_module(ontology: &Ontology) -> OwlResult<Ontology> {
    let mut module = if let Some(iri) = ontology.iri() {
        Ontology::with_iri(iri.clone())
    } else {
        Ontology::new()
    };

    if let Some(version_iri) = ontology.version_iri() {
        module.set_version_iri(version_iri.clone());
    }

    let classes: Vec<_> = ontology
        .classes()
        .iter()
        .map(|class| (**class).clone())
        .collect();
    let _ = module.add_classes_bulk_trusted(classes);

    let object_properties: Vec<_> = ontology
        .object_properties()
        .iter()
        .map(|prop| (**prop).clone())
        .collect();
    let _ = module.add_object_properties_bulk_trusted(object_properties);

    let data_properties: Vec<_> = ontology
        .data_properties()
        .iter()
        .map(|prop| (**prop).clone())
        .collect();
    let _ = module.add_data_properties_bulk_trusted(data_properties);

    for axiom in ontology.axioms() {
        match axiom.as_ref() {
            Axiom::SubClassOf(_)
            | Axiom::EquivalentClasses(_)
            | Axiom::DisjointClasses(_)
            | Axiom::SubObjectProperty(_)
            | Axiom::EquivalentObjectProperties(_)
            | Axiom::DisjointObjectProperties(_)
            | Axiom::InverseObjectProperties(_)
            | Axiom::SubDataProperty(_)
            | Axiom::EquivalentDataProperties(_)
            | Axiom::DisjointDataProperties(_)
            | Axiom::ObjectPropertyDomain(_)
            | Axiom::ObjectPropertyRange(_)
            | Axiom::DataPropertyDomain(_)
            | Axiom::DataPropertyRange(_)
            | Axiom::FunctionalProperty(_)
            | Axiom::FunctionalDataProperty(_)
            | Axiom::InverseFunctionalProperty(_)
            | Axiom::ReflexiveProperty(_)
            | Axiom::IrreflexiveProperty(_)
            | Axiom::SymmetricProperty(_)
            | Axiom::AsymmetricProperty(_)
            | Axiom::TransitiveProperty(_)
            | Axiom::SubPropertyChainOf(_)
            | Axiom::HasKey(_) => {
                module.add_axiom(axiom.as_ref().clone())?;
            }
            _ => {}
        }
    }

    Ok(module)
}
