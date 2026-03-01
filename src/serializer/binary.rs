//! Binary serialization format for Ontologies
//!
//! This format is designed for fast loading of large ontologies.
//! It is NOT a standard format - it's an internal optimization.
//!
//! Format version 1 (legacy, partial fidelity):
//! ```text
//! [Header] (32 bytes)
//!   - magic: [u8; 4] = b"OWLB"
//!   - version: u32 = 1
//!   - class_count: u64
//!   - object_property_count: u64
//!   - data_property_count: u64
//!   - individual_count: u64
//!   - axiom_count: u64
//!
//! [String Table]
//!   - string_count: u64
//!   - strings: [length: u32, bytes: [u8; length]]*
//!
//! [Classes Section]
//!   - class_iris: [string_id: u64]*
//!
//! [Object Properties Section]
//!   - property_iris: [string_id: u64]*
//!
//! [Data Properties Section]
//!   - property_iris: [string_id: u64]*
//!
//! [Individuals Section]
//!   - individual_iris: [string_id: u64]*
//!
//! [Axioms Section]
//!   - axioms: [type: u8, data: ...]*
//! ```
//!
//! Format version 2 (full fidelity, legacy varint bincode):
//! ```text
//! [Header] (32 bytes)
//!   - magic: [u8; 4] = b"OWLB"
//!   - version: u32 = 2
//!   - class_count: u64
//!   - object_property_count: u64
//!   - data_property_count: u64
//!   - individual_count: u64
//!   - axiom_count: u64
//!
//! [Payload]
//!   - bincode (serde) encoding of OntologyPayload
//! ```
//!
//! Format version 3 (full fidelity, fixed-int bincode for faster decode):
//! ```text
//! [Header] (32 bytes)
//!   - magic: [u8; 4] = b"OWLB"
//!   - version: u32 = 3
//!   - class_count: u64
//!   - object_property_count: u64
//!   - data_property_count: u64
//!   - individual_count: u64
//!   - axiom_count: u64
//!
//! [Payload]
//!   - bincode (serde) encoding of OntologyPayload with fixed integer encoding
//! ```

use std::io::{Read, Result as IoResult, Write};
use std::time::Instant;

use crate::core::entities::{
    Annotation, AnnotationProperty, AnonymousIndividual, Class, DataProperty, NamedIndividual,
    ObjectProperty,
};
use crate::core::error::OwlResult;
use crate::core::iri::IRI;
use crate::core::ontology::Ontology;
use crate::logic::axioms::{
    class_expressions::ClassExpression, Axiom, ClassAssertionAxiom, SubClassOfAxiom,
};
use bincode::config;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

const MAGIC: &[u8; 4] = b"OWLB";
const VERSION_V1: u32 = 1;
const VERSION_V2: u32 = 2;
const VERSION_V3: u32 = 3;

/// Axiom type identifiers for binary format
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AxiomType {
    SubClassOf = 1,
    EquivalentClasses = 2,
    DisjointClasses = 3,
    ClassAssertion = 4,
    PropertyAssertion = 5,
    SubObjectProperty = 6,
    // Add more as needed
}

impl AxiomType {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(AxiomType::SubClassOf),
            2 => Some(AxiomType::EquivalentClasses),
            3 => Some(AxiomType::DisjointClasses),
            4 => Some(AxiomType::ClassAssertion),
            5 => Some(AxiomType::PropertyAssertion),
            6 => Some(AxiomType::SubObjectProperty),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct OntologyPayload {
    iri: Option<IRI>,
    version_iri: Option<IRI>,
    imports: Vec<IRI>,
    classes: Vec<Class>,
    object_properties: Vec<ObjectProperty>,
    data_properties: Vec<DataProperty>,
    named_individuals: Vec<NamedIndividual>,
    anonymous_individuals: Vec<AnonymousIndividual>,
    annotation_properties: Vec<AnnotationProperty>,
    axioms: Vec<Axiom>,
    annotations: Vec<Annotation>,
}

impl From<&Ontology> for OntologyPayload {
    fn from(ontology: &Ontology) -> Self {
        OntologyPayload {
            iri: ontology.iri().cloned(),
            version_iri: ontology.version_iri().cloned(),
            imports: ontology
                .imports()
                .iter()
                .map(|iri| (**iri).clone())
                .collect(),
            classes: ontology
                .classes()
                .iter()
                .map(|class| (**class).clone())
                .collect(),
            object_properties: ontology
                .object_properties()
                .iter()
                .map(|prop| (**prop).clone())
                .collect(),
            data_properties: ontology
                .data_properties()
                .iter()
                .map(|prop| (**prop).clone())
                .collect(),
            named_individuals: ontology
                .named_individuals()
                .iter()
                .map(|ind| (**ind).clone())
                .collect(),
            anonymous_individuals: ontology
                .anonymous_individuals()
                .iter()
                .map(|ind| (**ind).clone())
                .collect(),
            annotation_properties: ontology
                .annotation_properties()
                .iter()
                .map(|prop| (**prop).clone())
                .collect(),
            axioms: ontology
                .axioms()
                .iter()
                .map(|axiom| axiom.as_ref().clone())
                .collect(),
            annotations: ontology.annotations().to_vec(),
        }
    }
}

impl OntologyPayload {
    fn into_ontology(self) -> OwlResult<Ontology> {
        let mut ontology = if let Some(iri) = self.iri {
            Ontology::with_iri(iri)
        } else {
            Ontology::new()
        };

        if let Some(version_iri) = self.version_iri {
            ontology.set_version_iri(version_iri);
        }

        for import in self.imports {
            ontology.add_import(import);
        }

        let _ = ontology.add_classes_bulk_trusted(self.classes);
        let _ = ontology.add_object_properties_bulk_trusted(self.object_properties);
        let _ = ontology.add_data_properties_bulk_trusted(self.data_properties);
        let _ = ontology.add_named_individuals_bulk_trusted(self.named_individuals);

        for individual in self.anonymous_individuals {
            ontology.add_anonymous_individual(individual)?;
        }

        for property in self.annotation_properties {
            ontology.add_annotation_property(property)?;
        }

        for annotation in self.annotations {
            ontology.add_annotation(annotation);
        }

        let _ = ontology.add_axioms_bulk_trusted(self.axioms)?;

        Ok(ontology)
    }
}

/// Binary ontology format for fast serialization
pub struct BinaryOntologyFormat;

impl BinaryOntologyFormat {
    fn stage_timing_enabled() -> bool {
        match std::env::var("OWL2_REASONER_STAGE_TIMING") {
            Ok(value) => {
                let value = value.trim().to_ascii_lowercase();
                !(value.is_empty() || value == "0" || value == "false" || value == "no")
            }
            Err(_) => false,
        }
    }

    fn stage_log(stage: &str, detail: &str) {
        if Self::stage_timing_enabled() {
            eprintln!("[stage] {} {}", stage, detail);
        }
    }

    /// Serialize an ontology to binary format
    pub fn serialize<W: Write>(ontology: &Ontology, writer: &mut W) -> IoResult<()> {
        let version = match std::env::var("OWL2_REASONER_BIN_FORMAT").ok().as_deref() {
            Some("v1") => VERSION_V1,
            Some("v2") => VERSION_V2,
            Some("v3") | None => VERSION_V3,
            _ => VERSION_V3,
        };

        if version == VERSION_V1 {
            return Self::serialize_v1(ontology, writer);
        }

        let payload = OntologyPayload::from(ontology);
        Self::write_header(
            writer,
            version,
            ontology.classes().len() as u64,
            ontology.object_properties().len() as u64,
            ontology.data_properties().len() as u64,
            ontology.named_individuals().len() as u64,
            ontology.axioms().len() as u64,
        )?;

        if version == VERSION_V2 {
            let config = config::standard();
            bincode::serde::encode_into_std_write(&payload, writer, config)
                .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err.to_string()))?;
        } else {
            let config = config::standard().with_fixed_int_encoding();
            bincode::serde::encode_into_std_write(&payload, writer, config)
                .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err.to_string()))?;
        }

        Ok(())
    }

    fn serialize_v1<W: Write>(ontology: &Ontology, writer: &mut W) -> IoResult<()> {
        // Build string table from all IRIs in the ontology
        let mut string_table = StringTable::new();

        // Collect all IRIs
        for class in ontology.classes() {
            string_table.add(class.iri().as_str());
        }
        for prop in ontology.object_properties() {
            string_table.add(prop.iri().as_str());
        }
        for prop in ontology.data_properties() {
            string_table.add(prop.iri().as_str());
        }
        for ind in ontology.named_individuals() {
            string_table.add(ind.iri().as_str());
        }

        // Also add IRIs from axioms (for complex class expressions)
        Self::collect_axiom_strings(ontology, &mut string_table);

        // Write header
        Self::write_header(
            writer,
            VERSION_V1,
            ontology.classes().len() as u64,
            ontology.object_properties().len() as u64,
            ontology.data_properties().len() as u64,
            ontology.named_individuals().len() as u64,
            Self::supported_axiom_count(ontology),
        )?;

        // Write string table
        string_table.write(writer)?;

        // Write entity sections
        Self::write_classes(writer, ontology, &string_table)?;
        Self::write_object_properties(writer, ontology, &string_table)?;
        Self::write_data_properties(writer, ontology, &string_table)?;
        Self::write_individuals(writer, ontology, &string_table)?;

        // Write axioms
        Self::write_axioms(writer, ontology, &string_table)?;

        Ok(())
    }

    /// Deserialize an ontology from binary format
    pub fn deserialize<R: Read>(reader: &mut R) -> IoResult<Ontology> {
        // Read and verify header
        let header = Self::read_header(reader)?;

        if &header.magic != MAGIC {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid binary format: wrong magic number",
            ));
        }

        match header.version {
            VERSION_V1 => Self::deserialize_v1(reader, header),
            VERSION_V2 => {
                let config = config::standard();
                let decode_start = Instant::now();
                let payload: OntologyPayload = bincode::serde::decode_from_std_read(reader, config)
                    .map_err(|err| {
                        std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string())
                    })?;
                Self::stage_log(
                    "binary_payload_decode_done",
                    &format!(
                        "ms={} version=2 classes={} axioms={}",
                        decode_start.elapsed().as_millis(),
                        payload.classes.len(),
                        payload.axioms.len()
                    ),
                );
                let materialize_start = Instant::now();
                payload.into_ontology().map_err(|err| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{:?}", err))
                }).inspect(|ontology| {
                    Self::stage_log(
                        "binary_payload_materialize_done",
                        &format!(
                            "ms={} version=2 classes={} axioms={}",
                            materialize_start.elapsed().as_millis(),
                            ontology.classes().len(),
                            ontology.axioms().len()
                        ),
                    );
                })
            }
            VERSION_V3 => {
                let config = config::standard().with_fixed_int_encoding();
                let decode_start = Instant::now();
                let payload: OntologyPayload = bincode::serde::decode_from_std_read(reader, config)
                    .map_err(|err| {
                        std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string())
                    })?;
                Self::stage_log(
                    "binary_payload_decode_done",
                    &format!(
                        "ms={} version=3 classes={} axioms={}",
                        decode_start.elapsed().as_millis(),
                        payload.classes.len(),
                        payload.axioms.len()
                    ),
                );
                let materialize_start = Instant::now();
                payload.into_ontology().map_err(|err| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{:?}", err))
                }).inspect(|ontology| {
                    Self::stage_log(
                        "binary_payload_materialize_done",
                        &format!(
                            "ms={} version=3 classes={} axioms={}",
                            materialize_start.elapsed().as_millis(),
                            ontology.classes().len(),
                            ontology.axioms().len()
                        ),
                    );
                })
            }
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Unsupported version: {}", header.version),
            )),
        }
    }

    fn deserialize_v1<R: Read>(reader: &mut R, header: Header) -> IoResult<Ontology> {
        // Read string table
        let string_table = StringTable::read(reader)?;

        // Create new ontology
        let mut ontology = Ontology::new();

        // Read entities
        Self::read_classes(reader, &mut ontology, &string_table, header.class_count)?;
        Self::read_object_properties(
            reader,
            &mut ontology,
            &string_table,
            header.object_property_count,
        )?;
        Self::read_data_properties(
            reader,
            &mut ontology,
            &string_table,
            header.data_property_count,
        )?;
        Self::read_individuals(
            reader,
            &mut ontology,
            &string_table,
            header.individual_count,
        )?;

        // Read axioms
        Self::read_axioms(reader, &mut ontology, &string_table, header.axiom_count)?;

        Ok(ontology)
    }

    // === String Collection ===

    fn collect_axiom_strings(ontology: &Ontology, table: &mut StringTable) {
        for axiom in ontology.axioms() {
            match axiom.as_ref() {
                Axiom::SubClassOf(axiom) => {
                    Self::collect_class_expression_strings(axiom.sub_class(), table);
                    Self::collect_class_expression_strings(axiom.super_class(), table);
                }
                Axiom::ClassAssertion(axiom) => {
                    table.add(axiom.individual().as_str());
                    Self::collect_class_expression_strings(axiom.class_expr(), table);
                }
                _ => {
                    // For other axiom types, we'd need to extract IRIs
                    // This is simplified - full implementation would handle all types
                }
            }
        }
    }

    fn supported_axiom_count(ontology: &Ontology) -> u64 {
        ontology
            .axioms()
            .iter()
            .filter(|axiom| {
                matches!(
                    axiom.as_ref(),
                    Axiom::SubClassOf(_) | Axiom::ClassAssertion(_)
                )
            })
            .count() as u64
    }

    fn collect_class_expression_strings(expr: &ClassExpression, table: &mut StringTable) {
        match expr {
            ClassExpression::Class(class) => {
                table.add(class.iri().as_str());
            }
            ClassExpression::ObjectIntersectionOf(operands)
            | ClassExpression::ObjectUnionOf(operands) => {
                for op in operands {
                    Self::collect_class_expression_strings(op, table);
                }
            }
            ClassExpression::ObjectComplementOf(operand) => {
                Self::collect_class_expression_strings(operand, table);
            }
            ClassExpression::ObjectSomeValuesFrom(_, class)
            | ClassExpression::ObjectAllValuesFrom(_, class) => {
                Self::collect_class_expression_strings(class, table);
            }
            _ => {}
        }
    }

    // === Header ===

    fn write_header<W: Write>(
        writer: &mut W,
        version: u32,
        class_count: u64,
        object_property_count: u64,
        data_property_count: u64,
        individual_count: u64,
        axiom_count: u64,
    ) -> IoResult<()> {
        writer.write_all(MAGIC)?;
        writer.write_all(&version.to_le_bytes())?;
        writer.write_all(&class_count.to_le_bytes())?;
        writer.write_all(&object_property_count.to_le_bytes())?;
        writer.write_all(&data_property_count.to_le_bytes())?;
        writer.write_all(&individual_count.to_le_bytes())?;
        writer.write_all(&axiom_count.to_le_bytes())?;
        Ok(())
    }

    fn read_header<R: Read>(reader: &mut R) -> IoResult<Header> {
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;

        let mut version = [0u8; 4];
        reader.read_exact(&mut version)?;
        let version = u32::from_le_bytes(version);

        let mut class_count = [0u8; 8];
        reader.read_exact(&mut class_count)?;
        let class_count = u64::from_le_bytes(class_count);

        let mut object_property_count = [0u8; 8];
        reader.read_exact(&mut object_property_count)?;
        let object_property_count = u64::from_le_bytes(object_property_count);

        let mut data_property_count = [0u8; 8];
        reader.read_exact(&mut data_property_count)?;
        let data_property_count = u64::from_le_bytes(data_property_count);

        let mut individual_count = [0u8; 8];
        reader.read_exact(&mut individual_count)?;
        let individual_count = u64::from_le_bytes(individual_count);

        let mut axiom_count = [0u8; 8];
        reader.read_exact(&mut axiom_count)?;
        let axiom_count = u64::from_le_bytes(axiom_count);

        Ok(Header {
            magic,
            version,
            class_count,
            object_property_count,
            data_property_count,
            individual_count,
            axiom_count,
        })
    }

    // === Classes ===

    fn write_classes<W: Write>(
        writer: &mut W,
        ontology: &Ontology,
        string_table: &StringTable,
    ) -> IoResult<()> {
        for class in ontology.classes() {
            let id = string_table
                .get_id(class.iri().as_str())
                .expect("IRI not in string table");
            writer.write_all(&id.to_le_bytes())?;
        }
        Ok(())
    }

    fn read_classes<R: Read>(
        reader: &mut R,
        ontology: &mut Ontology,
        string_table: &StringTable,
        count: u64,
    ) -> IoResult<()> {
        // Phase 1: Read all string IDs
        let mut string_ids = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let id = Self::read_u64(reader)?;
            string_ids.push(id);
        }

        // Phase 2: Collect IRI strings (sequential - must read from string table)
        let iri_strings: Vec<String> = string_ids
            .iter()
            .map(|&id| {
                string_table
                    .get_string(id)
                    .map(|s| s.to_string())
                    .ok_or_else(|| {
                        std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid string ID")
                    })
            })
            .collect::<IoResult<Vec<_>>>()?;

        // Phase 3: Create IRIs in parallel using unchecked creation (no cache contention)
        // This is much faster for bulk loading from trusted binary data
        let iris = IRI::create_many_unchecked_parallel(iri_strings);

        // Phase 4: Create classes and add in bulk using trusted method (maximum speed)
        let classes: Vec<Class> = iris.into_iter().map(|iri| Class::new(iri)).collect();

        // Use trusted bulk insertion for maximum performance with binary data
        ontology.add_classes_bulk_trusted(classes);

        Ok(())
    }

    // === Object Properties ===

    fn write_object_properties<W: Write>(
        writer: &mut W,
        ontology: &Ontology,
        string_table: &StringTable,
    ) -> IoResult<()> {
        for prop in ontology.object_properties() {
            let id = string_table
                .get_id(prop.iri().as_str())
                .expect("IRI not in string table");
            writer.write_all(&id.to_le_bytes())?;
        }
        Ok(())
    }

    fn read_object_properties<R: Read>(
        reader: &mut R,
        ontology: &mut Ontology,
        string_table: &StringTable,
        count: u64,
    ) -> IoResult<()> {
        // Collect all properties first, then add in bulk
        let mut properties = Vec::with_capacity(count as usize);

        for _ in 0..count {
            let id = Self::read_u64(reader)?;
            let iri_str = string_table.get_string(id).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid string ID")
            })?;

            let prop = ObjectProperty::new(IRI::new(iri_str).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{:?}", e))
            })?);
            properties.push(prop);
        }

        // Use bulk insertion for better performance
        ontology.add_object_properties_bulk(properties.into_iter());

        Ok(())
    }

    // === Data Properties ===

    fn write_data_properties<W: Write>(
        writer: &mut W,
        ontology: &Ontology,
        string_table: &StringTable,
    ) -> IoResult<()> {
        for prop in ontology.data_properties() {
            let id = string_table
                .get_id(prop.iri().as_str())
                .expect("IRI not in string table");
            writer.write_all(&id.to_le_bytes())?;
        }
        Ok(())
    }

    fn read_data_properties<R: Read>(
        reader: &mut R,
        ontology: &mut Ontology,
        string_table: &StringTable,
        count: u64,
    ) -> IoResult<()> {
        // Collect all properties first, then add in bulk
        let mut properties = Vec::with_capacity(count as usize);

        for _ in 0..count {
            let id = Self::read_u64(reader)?;
            let iri_str = string_table.get_string(id).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid string ID")
            })?;

            let prop = DataProperty::new(IRI::new(iri_str).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{:?}", e))
            })?);
            properties.push(prop);
        }

        // Use bulk insertion for better performance
        ontology.add_data_properties_bulk(properties.into_iter());

        Ok(())
    }

    // === Individuals ===

    fn write_individuals<W: Write>(
        writer: &mut W,
        ontology: &Ontology,
        string_table: &StringTable,
    ) -> IoResult<()> {
        for ind in ontology.named_individuals() {
            let id = string_table
                .get_id(ind.iri().as_str())
                .expect("IRI not in string table");
            writer.write_all(&id.to_le_bytes())?;
        }
        Ok(())
    }

    fn read_individuals<R: Read>(
        reader: &mut R,
        ontology: &mut Ontology,
        string_table: &StringTable,
        count: u64,
    ) -> IoResult<()> {
        // Collect all individuals first, then add in bulk
        let mut individuals = Vec::with_capacity(count as usize);

        for _ in 0..count {
            let id = Self::read_u64(reader)?;
            let iri_str = string_table.get_string(id).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid string ID")
            })?;

            let ind = NamedIndividual::new(IRI::new(iri_str).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{:?}", e))
            })?);
            individuals.push(ind);
        }

        // Use bulk insertion for better performance
        ontology.add_named_individuals_bulk(individuals.into_iter());

        Ok(())
    }

    // === Axioms ===

    fn write_axioms<W: Write>(
        writer: &mut W,
        ontology: &Ontology,
        string_table: &StringTable,
    ) -> IoResult<()> {
        for axiom in ontology.axioms() {
            match axiom.as_ref() {
                Axiom::SubClassOf(axiom) => {
                    writer.write_all(&(AxiomType::SubClassOf as u8).to_le_bytes())?;
                    Self::write_class_expression(writer, axiom.sub_class(), string_table)?;
                    Self::write_class_expression(writer, axiom.super_class(), string_table)?;
                }
                Axiom::ClassAssertion(axiom) => {
                    writer.write_all(&(AxiomType::ClassAssertion as u8).to_le_bytes())?;
                    let ind_id = string_table.get_id(axiom.individual().as_str()).unwrap();
                    writer.write_all(&ind_id.to_le_bytes())?;
                    Self::write_class_expression(writer, axiom.class_expr(), string_table)?;
                }
                _ => {
                    // Skip other axiom types for now (placeholder)
                    // In full implementation, handle all types
                }
            }
        }
        Ok(())
    }

    fn read_axioms<R: Read>(
        reader: &mut R,
        ontology: &mut Ontology,
        string_table: &StringTable,
        count: u64,
    ) -> IoResult<()> {
        for _ in 0..count {
            let axiom_type = match Self::read_u8(reader) {
                Ok(value) => value,
                Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
                    break;
                }
                Err(err) => return Err(err),
            };
            let axiom_type = AxiomType::from_u8(axiom_type).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Unknown axiom type")
            })?;

            match axiom_type {
                AxiomType::SubClassOf => {
                    let sub = Self::read_class_expression(reader, string_table)?;
                    let sup = Self::read_class_expression(reader, string_table)?;
                    let axiom = SubClassOfAxiom::new(sub, sup);
                    ontology.add_subclass_axiom(axiom).map_err(|e| {
                        std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e))
                    })?;
                }
                AxiomType::ClassAssertion => {
                    let ind_id = Self::read_u64(reader)?;
                    let ind_iri = string_table.get_string(ind_id).unwrap();
                    let iri = IRI::new(ind_iri).map_err(|e| {
                        std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{:?}", e))
                    })?;
                    let cls = Self::read_class_expression(reader, string_table)?;
                    let axiom = ClassAssertionAxiom::new(std::sync::Arc::new(iri), cls);
                    ontology.add_class_assertion(axiom).map_err(|e| {
                        std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e))
                    })?;
                }
                _ => {
                    // Skip other types for now
                }
            }
        }
        Ok(())
    }

    // === Class Expression Serialization ===

    fn write_class_expression<W: Write>(
        writer: &mut W,
        expr: &ClassExpression,
        string_table: &StringTable,
    ) -> IoResult<()> {
        match expr {
            ClassExpression::Class(class) => {
                writer.write_all(&[1u8])?; // Type tag for Class
                let id = string_table.get_id(class.iri().as_str()).unwrap();
                writer.write_all(&id.to_le_bytes())?;
            }
            ClassExpression::ObjectIntersectionOf(operands) => {
                writer.write_all(&[2u8])?;
                writer.write_all(&(operands.len() as u32).to_le_bytes())?;
                for op in operands {
                    Self::write_class_expression(writer, op, string_table)?;
                }
            }
            ClassExpression::ObjectUnionOf(operands) => {
                writer.write_all(&[3u8])?;
                writer.write_all(&(operands.len() as u32).to_le_bytes())?;
                for op in operands {
                    Self::write_class_expression(writer, op, string_table)?;
                }
            }
            ClassExpression::ObjectComplementOf(operand) => {
                writer.write_all(&[4u8])?;
                Self::write_class_expression(writer, operand, string_table)?;
            }
            _ => {
                writer.write_all(&[0u8])?; // Unknown/unsupported
            }
        }
        Ok(())
    }

    fn read_class_expression<R: Read>(
        reader: &mut R,
        string_table: &StringTable,
    ) -> IoResult<ClassExpression> {
        let type_tag = Self::read_u8(reader)?;
        match type_tag {
            1 => {
                // Class
                let id = Self::read_u64(reader)?;
                let iri_str = string_table.get_string(id).unwrap();
                let iri = IRI::new(iri_str).map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, format!("{:?}", e))
                })?;
                Ok(ClassExpression::Class(Class::new(iri)))
            }
            2 => {
                // ObjectIntersectionOf
                let len = Self::read_u32(reader)?;
                let mut operands = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    operands.push(Self::read_class_expression(reader, string_table)?);
                }
                let operands: SmallVec<[Box<ClassExpression>; 4]> =
                    operands.into_iter().map(Box::new).collect();
                Ok(ClassExpression::ObjectIntersectionOf(operands))
            }
            3 => {
                // ObjectUnionOf
                let len = Self::read_u32(reader)?;
                let mut operands = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    operands.push(Self::read_class_expression(reader, string_table)?);
                }
                let operands: SmallVec<[Box<ClassExpression>; 4]> =
                    operands.into_iter().map(Box::new).collect();
                Ok(ClassExpression::ObjectUnionOf(operands))
            }
            4 => {
                // ObjectComplementOf
                let operand = Self::read_class_expression(reader, string_table)?;
                Ok(ClassExpression::ObjectComplementOf(Box::new(operand)))
            }
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Unknown class expression type",
            )),
        }
    }

    // === Helper Functions ===

    fn read_u8<R: Read>(reader: &mut R) -> IoResult<u8> {
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf)?;
        Ok(buf[0])
    }

    fn read_u32<R: Read>(reader: &mut R) -> IoResult<u32> {
        let mut buf = [0u8; 4];
        reader.read_exact(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    fn read_u64<R: Read>(reader: &mut R) -> IoResult<u64> {
        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }
}

#[derive(Debug)]
struct Header {
    magic: [u8; 4],
    version: u32,
    class_count: u64,
    object_property_count: u64,
    data_property_count: u64,
    individual_count: u64,
    axiom_count: u64,
}

/// String table for deduplication
struct StringTable {
    strings: Vec<String>,
    id_map: std::collections::HashMap<String, u64>,
}

impl StringTable {
    fn new() -> Self {
        Self {
            strings: Vec::new(),
            id_map: std::collections::HashMap::new(),
        }
    }

    fn add(&mut self, s: &str) -> u64 {
        if let Some(&id) = self.id_map.get(s) {
            return id;
        }
        let id = self.strings.len() as u64;
        self.strings.push(s.to_string());
        self.id_map.insert(s.to_string(), id);
        id
    }

    fn get_id(&self, s: &str) -> Option<u64> {
        self.id_map.get(s).copied()
    }

    fn get_string(&self, id: u64) -> Option<&str> {
        self.strings.get(id as usize).map(|s| s.as_str())
    }

    fn write<W: Write>(&self, writer: &mut W) -> IoResult<()> {
        writer.write_all(&(self.strings.len() as u64).to_le_bytes())?;
        for s in &self.strings {
            let bytes = s.as_bytes();
            writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
            writer.write_all(bytes)?;
        }
        Ok(())
    }

    fn read<R: Read>(reader: &mut R) -> IoResult<Self> {
        let count = Self::read_u64(reader)?;
        let mut strings = Vec::with_capacity(count as usize);
        let mut id_map = std::collections::HashMap::new();

        for i in 0..count {
            let len = Self::read_u32(reader)? as usize;
            let mut bytes = vec![0u8; len];
            reader.read_exact(&mut bytes)?;
            let s = String::from_utf8(bytes)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            id_map.insert(s.clone(), i);
            strings.push(s);
        }

        Ok(Self { strings, id_map })
    }

    fn read_u32<R: Read>(reader: &mut R) -> IoResult<u32> {
        let mut buf = [0u8; 4];
        reader.read_exact(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    fn read_u64<R: Read>(reader: &mut R) -> IoResult<u64> {
        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::entities::Class;

    #[test]
    fn test_serialize_deserialize_empty() {
        let ontology = Ontology::new();
        let mut buffer = Vec::new();

        BinaryOntologyFormat::serialize(&ontology, &mut buffer).unwrap();

        let mut reader = &buffer[..];
        let deserialized = BinaryOntologyFormat::deserialize(&mut reader).unwrap();

        assert_eq!(deserialized.classes().len(), 0);
    }

    #[test]
    fn test_serialize_deserialize_with_classes() {
        let mut ontology = Ontology::new();
        ontology
            .add_class(Class::new(IRI::new("http://example.org/A").unwrap()))
            .unwrap();
        ontology
            .add_class(Class::new(IRI::new("http://example.org/B").unwrap()))
            .unwrap();

        let mut buffer = Vec::new();
        BinaryOntologyFormat::serialize(&ontology, &mut buffer).unwrap();

        let mut reader = &buffer[..];
        let deserialized = BinaryOntologyFormat::deserialize(&mut reader).unwrap();

        assert_eq!(deserialized.classes().len(), 2);
    }

    #[test]
    fn test_serialize_deserialize_with_subclass_axioms() {
        let mut ontology = Ontology::new();
        let class_a = Class::new(IRI::new("http://example.org/A").unwrap());
        let class_b = Class::new(IRI::new("http://example.org/B").unwrap());
        ontology.add_class(class_a.clone()).unwrap();
        ontology.add_class(class_b.clone()).unwrap();

        let axiom = SubClassOfAxiom::new(
            ClassExpression::Class(class_a),
            ClassExpression::Class(class_b),
        );
        ontology.add_subclass_axiom(axiom).unwrap();

        let mut buffer = Vec::new();
        BinaryOntologyFormat::serialize(&ontology, &mut buffer).unwrap();

        let mut reader = &buffer[..];
        let deserialized = BinaryOntologyFormat::deserialize(&mut reader).unwrap();

        assert_eq!(deserialized.classes().len(), 2);
        assert_eq!(deserialized.axioms().len(), 1);
    }

    #[test]
    fn test_invalid_magic() {
        let data = b"XXXX\x01\x00\x00\x00"; // Wrong magic
        let mut reader = &data[..];

        assert!(BinaryOntologyFormat::deserialize(&mut reader).is_err());
    }
}
