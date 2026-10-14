//! Streaming RDF/XML parser using rio-xml library

use crate::core::entities::*;
use crate::core::error::OwlResult;
use crate::core::iri::IRI;
use crate::core::ontology::Ontology;
use crate::logic::axioms::class_expressions::{ClassExpression, DataRange, FacetRestriction};
use crate::logic::axioms::*;
use crate::parser::rdf_xml_common::{ERR_RIO_XML_PARSE, NS_OWL};
use crate::parser::{ParserArenaBuilder, ParserArenaTrait, ParserConfig};
#[cfg(feature = "rio-xml")]
use smallvec::SmallVec;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Cursor;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

#[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
use crossbeam::channel::bounded;
#[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
use crossbeam::thread;
#[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
use dashmap::DashMap;
#[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
use parking_lot::RwLock;
#[cfg(feature = "rio-xml")]
use rio_api::model::{Subject, Term, Triple};
#[cfg(feature = "rio-xml")]
use rio_api::parser::TriplesParser as _;
#[cfg(feature = "rio-xml")]
use rio_xml::RdfXmlParser as RioRdfXmlParser;
#[cfg(feature = "rio-xml")]
use rustc_hash::FxHashMap;
#[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
use std::borrow::Cow;
#[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
use std::cmp;
#[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

#[cfg(feature = "rio-xml")]
const RDF_TYPE_IRI: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
#[cfg(feature = "rio-xml")]
const RDFS_SUBCLASS_OF_IRI: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
#[cfg(feature = "rio-xml")]
const RDFS_SUBPROPERTY_OF_IRI: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
#[cfg(feature = "rio-xml")]
const RDFS_DOMAIN_IRI: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
#[cfg(feature = "rio-xml")]
const RDFS_RANGE_IRI: &str = "http://www.w3.org/2000/01/rdf-schema#range";
#[cfg(feature = "rio-xml")]
const OWL_CLASS_IRI: &str = "http://www.w3.org/2002/07/owl#Class";
#[cfg(feature = "rio-xml")]
const OWL_OBJECT_PROPERTY_IRI: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
#[cfg(feature = "rio-xml")]
const OWL_DATATYPE_PROPERTY_IRI: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
#[cfg(feature = "rio-xml")]
const OWL_FUNCTIONAL_PROPERTY_IRI: &str = "http://www.w3.org/2002/07/owl#FunctionalProperty";
#[cfg(feature = "rio-xml")]
const OWL_INVERSE_FUNCTIONAL_PROPERTY_IRI: &str =
    "http://www.w3.org/2002/07/owl#InverseFunctionalProperty";
#[cfg(feature = "rio-xml")]
const OWL_REFLEXIVE_PROPERTY_IRI: &str = "http://www.w3.org/2002/07/owl#ReflexiveProperty";
#[cfg(feature = "rio-xml")]
const OWL_IRREFLEXIVE_PROPERTY_IRI: &str = "http://www.w3.org/2002/07/owl#IrreflexiveProperty";
#[cfg(feature = "rio-xml")]
const OWL_SYMMETRIC_PROPERTY_IRI: &str = "http://www.w3.org/2002/07/owl#SymmetricProperty";
#[cfg(feature = "rio-xml")]
const OWL_ASYMMETRIC_PROPERTY_IRI: &str = "http://www.w3.org/2002/07/owl#AsymmetricProperty";
#[cfg(feature = "rio-xml")]
const OWL_TRANSITIVE_PROPERTY_IRI: &str = "http://www.w3.org/2002/07/owl#TransitiveProperty";
#[cfg(feature = "rio-xml")]
const OWL_BOTTOM_OBJECT_PROPERTY_IRI: &str = "http://www.w3.org/2002/07/owl#bottomObjectProperty";
#[cfg(feature = "rio-xml")]
const OWL_BOTTOM_DATA_PROPERTY_IRI: &str = "http://www.w3.org/2002/07/owl#bottomDataProperty";
#[cfg(feature = "rio-xml")]
const OWL_NAMED_INDIVIDUAL_IRI: &str = "http://www.w3.org/2002/07/owl#NamedIndividual";
#[cfg(feature = "rio-xml")]
const OWL_RESTRICTION_IRI: &str = "http://www.w3.org/2002/07/owl#Restriction";
#[cfg(feature = "rio-xml")]
const OWL_ON_PROPERTY_IRI: &str = "http://www.w3.org/2002/07/owl#onProperty";
#[cfg(feature = "rio-xml")]
const OWL_SOME_VALUES_FROM_IRI: &str = "http://www.w3.org/2002/07/owl#someValuesFrom";
#[cfg(feature = "rio-xml")]
const OWL_ALL_VALUES_FROM_IRI: &str = "http://www.w3.org/2002/07/owl#allValuesFrom";
#[cfg(feature = "rio-xml")]
const OWL_HAS_VALUE_IRI: &str = "http://www.w3.org/2002/07/owl#hasValue";
#[cfg(feature = "rio-xml")]
const OWL_COMPLEMENT_OF_IRI: &str = "http://www.w3.org/2002/07/owl#complementOf";
#[cfg(feature = "rio-xml")]
const OWL_UNION_OF_IRI: &str = "http://www.w3.org/2002/07/owl#unionOf";
const OWL_DISJOINT_UNION_OF_IRI: &str = "http://www.w3.org/2002/07/owl#disjointUnionOf";
#[cfg(feature = "rio-xml")]
const OWL_INTERSECTION_OF_IRI: &str = "http://www.w3.org/2002/07/owl#intersectionOf";
#[cfg(feature = "rio-xml")]
const OWL_NEGATIVE_PROPERTY_ASSERTION_IRI: &str =
    "http://www.w3.org/2002/07/owl#NegativePropertyAssertion";
#[cfg(feature = "rio-xml")]
const OWL_SOURCE_INDIVIDUAL_IRI: &str = "http://www.w3.org/2002/07/owl#sourceIndividual";
#[cfg(feature = "rio-xml")]
const OWL_ASSERTION_PROPERTY_IRI: &str = "http://www.w3.org/2002/07/owl#assertionProperty";
#[cfg(feature = "rio-xml")]
const OWL_TARGET_INDIVIDUAL_IRI: &str = "http://www.w3.org/2002/07/owl#targetIndividual";
#[cfg(feature = "rio-xml")]
const OWL_TARGET_VALUE_IRI: &str = "http://www.w3.org/2002/07/owl#targetValue";
#[cfg(feature = "rio-xml")]
const OWL_PROPERTY_DISJOINT_WITH_IRI: &str = "http://www.w3.org/2002/07/owl#propertyDisjointWith";
#[cfg(feature = "rio-xml")]
const OWL_SAME_AS_IRI: &str = "http://www.w3.org/2002/07/owl#sameAs";
#[cfg(feature = "rio-xml")]
const OWL_DIFFERENT_FROM_IRI: &str = "http://www.w3.org/2002/07/owl#differentFrom";
#[cfg(feature = "rio-xml")]
const OWL_MIN_CARDINALITY_IRI: &str = "http://www.w3.org/2002/07/owl#minCardinality";
#[cfg(feature = "rio-xml")]
const OWL_MAX_CARDINALITY_IRI: &str = "http://www.w3.org/2002/07/owl#maxCardinality";
#[cfg(feature = "rio-xml")]
const OWL_CARDINALITY_IRI: &str = "http://www.w3.org/2002/07/owl#cardinality";
#[cfg(feature = "rio-xml")]
const OWL_MIN_QUALIFIED_CARDINALITY_IRI: &str =
    "http://www.w3.org/2002/07/owl#minQualifiedCardinality";
#[cfg(feature = "rio-xml")]
const OWL_MAX_QUALIFIED_CARDINALITY_IRI: &str =
    "http://www.w3.org/2002/07/owl#maxQualifiedCardinality";
#[cfg(feature = "rio-xml")]
const OWL_QUALIFIED_CARDINALITY_IRI: &str = "http://www.w3.org/2002/07/owl#qualifiedCardinality";
#[cfg(feature = "rio-xml")]
const OWL_ON_CLASS_IRI: &str = "http://www.w3.org/2002/07/owl#onClass";
#[cfg(feature = "rio-xml")]
const OWL_ON_DATA_RANGE_IRI: &str = "http://www.w3.org/2002/07/owl#onDataRange";
#[cfg(feature = "rio-xml")]
const OWL_HAS_SELF_IRI: &str = "http://www.w3.org/2002/07/owl#hasSelf";
#[cfg(feature = "rio-xml")]
const OWL_ON_DATATYPE_IRI: &str = "http://www.w3.org/2002/07/owl#onDatatype";
#[cfg(feature = "rio-xml")]
const OWL_WITH_RESTRICTIONS_IRI: &str = "http://www.w3.org/2002/07/owl#withRestrictions";
#[cfg(feature = "rio-xml")]
const OWL_ONE_OF_IRI: &str = "http://www.w3.org/2002/07/owl#oneOf";
#[cfg(feature = "rio-xml")]
const OWL_HAS_KEY_IRI: &str = "http://www.w3.org/2002/07/owl#hasKey";
#[cfg(feature = "rio-xml")]
const OWL_EQUIVALENT_PROPERTY_IRI: &str = "http://www.w3.org/2002/07/owl#equivalentProperty";
#[cfg(feature = "rio-xml")]
const OWL_PROPERTY_CHAIN_AXIOM_IRI: &str = "http://www.w3.org/2002/07/owl#propertyChainAxiom";
#[cfg(feature = "rio-xml")]
const RDF_FIRST_IRI: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
#[cfg(feature = "rio-xml")]
const RDF_REST_IRI: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
#[cfg(feature = "rio-xml")]
const RDF_NIL_IRI: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
const RDF_NS_IRI: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
#[cfg(feature = "rio-xml")]
const OWL_DISJOINT_WITH_IRI: &str = "http://www.w3.org/2002/07/owl#disjointWith";
#[cfg(feature = "rio-xml")]
const OWL_ALL_DISJOINT_CLASSES_IRI: &str = "http://www.w3.org/2002/07/owl#AllDisjointClasses";
#[cfg(feature = "rio-xml")]
const OWL_ALL_DISJOINT_PROPERTIES_IRI: &str =
    "http://www.w3.org/2002/07/owl#AllDisjointProperties";
#[cfg(feature = "rio-xml")]
const OWL_ALL_DIFFERENT_IRI: &str = "http://www.w3.org/2002/07/owl#AllDifferent";
#[cfg(feature = "rio-xml")]
const OWL_MEMBERS_IRI: &str = "http://www.w3.org/2002/07/owl#members";
#[cfg(feature = "rio-xml")]
const OWL_DISTINCT_MEMBERS_IRI: &str = "http://www.w3.org/2002/07/owl#distinctMembers";
#[cfg(feature = "rio-xml")]
const OWL_EQUIVALENT_CLASS_IRI: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
#[cfg(feature = "rio-xml")]
const OWL_INVERSE_OF_IRI: &str = "http://www.w3.org/2002/07/owl#inverseOf";
#[cfg(feature = "rio-xml")]
const RDF_XML_BUF_CAPACITY: usize = 1024 * 1024;
#[cfg(feature = "rio-xml")]
const STRUCTURAL_AUTO_THRESHOLD_BYTES_DEFAULT: u64 = 4 * 1024 * 1024;
#[cfg(feature = "rio-xml")]
const CLASS_EXPRESSION_MAX_DEPTH: usize = 128;
#[cfg(feature = "rio-xml")]
const CLASS_EXPRESSION_MAX_GENERATED_NODES: usize = 100_000;
#[cfg(feature = "rio-xml")]
const XSD_BOOLEAN_IRI: &str = "http://www.w3.org/2001/XMLSchema#boolean";
#[cfg(feature = "rio-xml")]
const XSD_NAMESPACE_IRI: &str = "http://www.w3.org/2001/XMLSchema#";

#[cfg(feature = "rio-xml")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PredicateTag {
    RdfType,
    RdfsSubClassOf,
    RdfsSubPropertyOf,
    RdfsDomain,
    RdfsRange,
    OwlDisjointWith,
    OwlEquivalentClass,
    OwlInverseOf,
    OwlOther,
    Other,
}

/// Which axiom an owl:AllDisjoint* / owl:AllDifferent marker node
/// stands for; the members list resolves against this at the flush.
#[cfg(feature = "rio-xml")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AllDisjointKind {
    Classes,
    Properties,
    Individuals,
}

#[cfg(feature = "rio-xml")]
impl AllDisjointKind {
    /// The marker rdf:type IRIs the kind keys on; None is every other
    /// type, so the type-assertion dispatcher's guard and the mapping
    /// stay one table (#130).
    fn from_marker_iri(iri: &str) -> Option<Self> {
        match iri {
            OWL_ALL_DISJOINT_CLASSES_IRI => Some(Self::Classes),
            OWL_ALL_DISJOINT_PROPERTIES_IRI => Some(Self::Properties),
            OWL_ALL_DIFFERENT_IRI => Some(Self::Individuals),
            _ => None,
        }
    }
}

#[cfg(feature = "rio-xml")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ObjectPropertyCharacteristicKind {
    Functional,
    InverseFunctional,
    Reflexive,
    Irreflexive,
    Symmetric,
    Asymmetric,
    Transitive,
}

#[cfg(feature = "rio-xml")]
#[derive(Clone, Debug, PartialEq, Eq)]
enum SubjectCacheKind {
    Named,
    Blank,
}

#[cfg(feature = "rio-xml")]
#[derive(Debug, Clone)]
struct PendingClassAssertion {
    individual: IRI,
    restriction_node: String,
}

#[cfg(feature = "rio-xml")]
#[derive(Debug, Clone)]
struct PendingClassEdge {
    left: ProcessedObject,
    right: ProcessedObject,
    equivalent: bool,
}

#[cfg(feature = "rio-xml")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingCardinalityKind {
    Min,
    Max,
    Exact,
}

#[cfg(feature = "rio-xml")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingCardinality {
    kind: PendingCardinalityKind,
    qualified: bool,
    value: ProcessedObject,
}

#[cfg(feature = "rio-xml")]
#[derive(Debug, Clone, Default)]
struct PendingRestriction {
    on_property: Option<IRI>,
    some_values_from: Option<ProcessedObject>,
    all_values_from: Option<ProcessedObject>,
    has_value: Option<ProcessedObject>,
    complement_of: Option<ProcessedObject>,
    union_of: Option<ProcessedObject>,
    intersection_of: Option<ProcessedObject>,
    on_datatype: Option<IRI>,
    with_restrictions: Option<ProcessedObject>,
    one_of: Option<ProcessedObject>,
    on_class: Vec<ProcessedObject>,
    on_data_range: Vec<ProcessedObject>,
    has_self: Vec<ProcessedObject>,
    cardinalities: Vec<PendingCardinality>,
}

#[cfg(feature = "rio-xml")]
#[derive(Debug, Clone, Default)]
struct PendingRdfList {
    first: Option<ProcessedObject>,
    rest: Option<ProcessedObject>,
}

#[cfg(feature = "rio-xml")]
#[derive(Debug, Clone, Default)]
struct PendingNegativePropertyAssertion {
    source_individual: Option<IRI>,
    assertion_property: Option<IRI>,
    target: Option<ProcessedObject>,
}

#[cfg(feature = "rio-xml")]
struct ClassExpressionResolutionBudget {
    generated_nodes: usize,
}

#[cfg(feature = "rio-xml")]
impl ClassExpressionResolutionBudget {
    fn new() -> Self {
        Self { generated_nodes: 0 }
    }

    fn enter(&mut self, depth: usize) -> OwlResult<()> {
        if depth > CLASS_EXPRESSION_MAX_DEPTH {
            return Err(crate::core::error::OwlError::ResourceLimitExceeded {
                resource_type: "RDF/XML class expression depth".to_string(),
                limit: CLASS_EXPRESSION_MAX_DEPTH,
                message: "deferred blank-node class expression exceeds parser resolution depth"
                    .to_string(),
            });
        }
        self.generated_nodes = self.generated_nodes.saturating_add(1);
        if self.generated_nodes > CLASS_EXPRESSION_MAX_GENERATED_NODES {
            return Err(crate::core::error::OwlError::ResourceLimitExceeded {
                resource_type: "RDF/XML class expression generated nodes".to_string(),
                limit: CLASS_EXPRESSION_MAX_GENERATED_NODES,
                message:
                    "deferred blank-node class expression expansion exceeds parser resolution budget"
                        .to_string(),
            });
        }
        Ok(())
    }
}

#[cfg(feature = "rio-xml")]
type StructuralTermId = u32;

#[cfg(feature = "rio-xml")]
#[derive(Debug, Clone, Copy)]
enum StructuralSubjectRecord {
    Iri(StructuralTermId),
    BNode(StructuralTermId),
}

#[cfg(feature = "rio-xml")]
#[derive(Debug, Clone, Copy)]
enum StructuralObjectRecord {
    Iri(StructuralTermId),
    BNode(StructuralTermId),
    Literal {
        value: StructuralTermId,
        datatype: Option<StructuralTermId>,
        lang: Option<StructuralTermId>,
    },
}

#[cfg(feature = "rio-xml")]
#[derive(Debug, Clone, Copy)]
struct StructuralTripleRecord {
    subject: StructuralSubjectRecord,
    predicate: StructuralTermId,
    object: StructuralObjectRecord,
}

#[cfg(feature = "rio-xml")]
#[derive(Default)]
struct StructuralInterner {
    forward: FxHashMap<Rc<str>, StructuralTermId>,
    reverse: Vec<Rc<str>>,
}

/// Ingest-side subject identity cursor (#170): RDF/XML emits a
/// description block's triples consecutively, so the incoming subject
/// equals the previous triple's subject for ~4 of 5 triples on the OBO
/// corpora (measured: go-basic 83%, uberon 74%). A kind-discriminated
/// lexical compare reuses the interned id without another interner
/// round-trip; the kind keeps a blank-node label from matching an IRI
/// of the same spelling.
#[cfg(feature = "rio-xml")]
struct StructuralSubjectCursor {
    kind: SubjectCacheKind,
    lexical: String,
    term_id: StructuralTermId,
}

#[cfg(feature = "rio-xml")]
impl StructuralSubjectCursor {
    fn matches(&self, kind: &SubjectCacheKind, lexical: &str) -> bool {
        self.kind == *kind && self.lexical == lexical
    }
}

    /// An owned RDF graph term — the FULL lane's semantic input shape
    /// (ADR 0006): identity, order, and vocabulary survive exactly as
    /// the document states them.
    #[cfg(feature = "rio-xml")]
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RdfGraphTerm {
    Iri(String),
    Blank(String),
    Literal {
        lexical: String,
        datatype: Option<String>,
        language: Option<String>,
    },
}

/// One owned RDF triple of the document's complete graph.
    #[cfg(feature = "rio-xml")]
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RdfGraphTriple {
    pub subject: RdfGraphTerm,
    pub predicate: String,
    pub object: RdfGraphTerm,
    }



#[cfg(feature = "rio-xml")]
impl StructuralInterner {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            forward: FxHashMap::with_capacity_and_hasher(capacity, Default::default()),
            reverse: Vec::with_capacity(capacity),
        }
    }

    fn intern(&mut self, value: &str) -> StructuralTermId {
        if let Some(existing) = self.forward.get(value) {
            return *existing;
        }
        let id = self.reverse.len() as StructuralTermId;
        // One allocation per new term: the Rc header rides in front of the
        // string bytes, and the forward key shares it by refcount instead
        // of cloning (#170).
        let owned: Rc<str> = Rc::from(value);
        self.reverse.push(Rc::clone(&owned));
        self.forward.insert(owned, id);
        id
    }

    fn resolve(&self, id: StructuralTermId) -> Option<&str> {
        self.reverse.get(id as usize).map(|owned| &**owned)
    }
}

#[cfg(feature = "rio-xml")]
struct ProgressBufRead<R: std::io::BufRead> {
    inner: R,
    enabled: bool,
    bytes_consumed: u64,
    next_log_at: u64,
    every_bytes: u64,
    started_at: Instant,
}

#[cfg(feature = "rio-xml")]
impl<R: std::io::BufRead> ProgressBufRead<R> {
    fn new(inner: R, enabled: bool, every_bytes: u64, started_at: Instant) -> Self {
        Self {
            inner,
            enabled,
            bytes_consumed: 0,
            next_log_at: every_bytes.max(1),
            every_bytes: every_bytes.max(1),
            started_at,
        }
    }

    fn on_bytes(&mut self, delta: u64) {
        if delta == 0 {
            return;
        }
        self.bytes_consumed = self.bytes_consumed.saturating_add(delta);
        if self.enabled && self.bytes_consumed >= self.next_log_at {
            eprintln!(
                "[phase] parse_io_progress bytes={} elapsed_ms={}",
                self.bytes_consumed,
                self.started_at.elapsed().as_millis()
            );
            while self.next_log_at <= self.bytes_consumed {
                self.next_log_at = self.next_log_at.saturating_add(self.every_bytes);
            }
        }
    }
}

#[cfg(feature = "rio-xml")]
impl<R: std::io::BufRead> std::io::Read for ProgressBufRead<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.on_bytes(n as u64);
        Ok(n)
    }
}

#[cfg(feature = "rio-xml")]
impl<R: std::io::BufRead> std::io::BufRead for ProgressBufRead<R> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        self.inner.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.inner.consume(amt);
        self.on_bytes(amt as u64);
    }
}

#[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum RawTerm {
    Iri(String),
    BNode(String),
    Literal {
        value: String,
        datatype: Option<String>,
        lang: Option<String>,
    },
}

#[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
#[derive(Debug, Clone)]
struct RawTripleRecord {
    seq: u64,
    s: RawTerm,
    p: RawTerm,
    o: RawTerm,
}

#[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
#[derive(Debug)]
struct RawBatch {
    terms: Vec<RawTripleRecord>,
}

#[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
impl RawBatch {
    fn new(capacity: usize) -> Self {
        Self {
            terms: Vec::with_capacity(capacity),
        }
    }

    fn push(&mut self, record: RawTripleRecord) {
        self.terms.push(record);
    }

    fn is_full(&self, batch_size: usize) -> bool {
        self.terms.len() >= batch_size
    }
}

#[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
#[derive(Debug, Clone, Copy)]
struct CompactTripleRecord {
    seq: u64,
    s: TermId,
    p: TermId,
    o: TermId,
}

#[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
#[derive(Default)]
struct PipelineStats {
    total_triples: AtomicUsize,
    skipped_triples: AtomicUsize,
}

#[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
struct GenerationalCache {
    current: FxHashMap<RawTerm, TermId>,
    old: FxHashMap<RawTerm, TermId>,
    capacity: usize,
}

#[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
impl GenerationalCache {
    fn new(capacity: usize) -> Self {
        Self {
            current: FxHashMap::default(),
            old: FxHashMap::default(),
            capacity: capacity.max(1),
        }
    }

    fn get_or_resolve<F>(&mut self, term: RawTerm, resolve_fn: F) -> TermId
    where
        F: Fn(&RawTerm) -> TermId,
    {
        if let Some(&id) = self.current.get(&term) {
            return id;
        }

        if let Some(&id) = self.old.get(&term) {
            self.current.insert(term, id);
            return id;
        }

        let id = resolve_fn(&term);
        if self.current.len() >= self.capacity {
            self.old = std::mem::replace(&mut self.current, FxHashMap::default());
        }
        self.current.insert(term, id);
        id
    }
}

#[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
type TermId = u32;

#[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
struct ExperimentalInterner {
    // Global string -> ID map used concurrently by worker threads.
    forward: DashMap<Arc<str>, TermId>,
    // ID -> string table for deterministic decode when materializing ontology.
    // Indexed by TermId to avoid reorder races across worker threads.
    reverse: RwLock<Vec<Option<Arc<str>>>>,
    next_id: AtomicU32,
}

#[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
impl ExperimentalInterner {
    fn new() -> Self {
        Self {
            forward: DashMap::new(),
            reverse: RwLock::new(Vec::new()),
            next_id: AtomicU32::new(0),
        }
    }

    fn get_or_intern(&self, key: &str) -> TermId {
        if let Some(existing) = self.forward.get(key) {
            return *existing;
        }

        let key_arc: Arc<str> = Arc::from(key);
        let entry = self.forward.entry(Arc::clone(&key_arc)).or_insert_with(|| {
            let id = self.next_id.fetch_add(1, Ordering::Relaxed);
            let mut reverse = self.reverse.write();
            let idx = id as usize;
            if reverse.len() <= idx {
                reverse.resize_with(idx + 1, || None);
            }
            reverse[idx] = Some(Arc::clone(&key_arc));
            id
        });
        *entry
    }

    fn resolve(&self, id: TermId) -> Option<Arc<str>> {
        self.reverse
            .read()
            .get(id as usize)
            .and_then(|entry| entry.clone())
    }
}

/// Streaming RDF/XML parser for efficient large file processing
pub struct RdfXmlStreamingParser {
    pub config: ParserConfig,
    pub namespaces: HashMap<String, String>,
    pub base_iri: Option<IRI>,
    pub arena: Option<Box<dyn ParserArenaTrait>>,
    triple_counter: u64,
    parse_started_at: Option<Instant>,
    progress_every: u64,
    large_parse_enabled: bool,
    #[cfg(feature = "rio-xml")]
    predicate_iri_cache: FxHashMap<String, Arc<IRI>>,
    #[cfg(feature = "rio-xml")]
    predicate_iri_cache_capacity: usize,
    #[cfg(feature = "rio-xml")]
    object_iri_cache: FxHashMap<String, Arc<IRI>>,
    #[cfg(feature = "rio-xml")]
    object_iri_cache_capacity: usize,
    #[cfg(feature = "rio-xml")]
    blank_node_label_cache: FxHashMap<String, Arc<str>>,
    #[cfg(feature = "rio-xml")]
    blank_node_iri_cache: FxHashMap<String, Arc<IRI>>,
    #[cfg(feature = "rio-xml")]
    blank_node_cache_capacity: usize,
    #[cfg(feature = "rio-xml")]
    rdf_type_shared: Arc<IRI>,
    #[cfg(feature = "rio-xml")]
    rdfs_subclass_shared: Arc<IRI>,
    #[cfg(feature = "rio-xml")]
    rdfs_domain_shared: Arc<IRI>,
    #[cfg(feature = "rio-xml")]
    rdfs_range_shared: Arc<IRI>,
    #[cfg(feature = "rio-xml")]
    owl_class_shared: Arc<IRI>,
    #[cfg(feature = "rio-xml")]
    owl_obj_prop_shared: Arc<IRI>,
    #[cfg(feature = "rio-xml")]
    owl_data_prop_shared: Arc<IRI>,
    #[cfg(feature = "rio-xml")]
    owl_named_ind_shared: Arc<IRI>,
    #[cfg(feature = "rio-xml")]
    owl_disjoint_shared: Arc<IRI>,
    #[cfg(feature = "rio-xml")]
    owl_equivalent_shared: Arc<IRI>,
    #[cfg(feature = "rio-xml")]
    last_subject_kind: Option<SubjectCacheKind>,
    #[cfg(feature = "rio-xml")]
    last_subject_value: String,
    #[cfg(feature = "rio-xml")]
    last_subject_iri: Option<IRI>,
    #[cfg(feature = "rio-xml")]
    pending_functional_properties: HashSet<IRI>,
    #[cfg(feature = "rio-xml")]
    pending_class_assertions: Vec<PendingClassAssertion>,
    #[cfg(feature = "rio-xml")]
    pending_class_edges: Vec<PendingClassEdge>,
    /// Deferred rdfs:range edges whose object is not an IRI: the range
    /// expression's own triples may arrive after the range triple, so
    /// resolution waits for the materialize flush (#150).
    #[cfg(feature = "rio-xml")]
    pending_range_edges: Vec<(IRI, ProcessedObject)>,
    #[cfg(feature = "rio-xml")]
    pending_restrictions: HashMap<String, PendingRestriction>,
    #[cfg(feature = "rio-xml")]
    pending_named_class_descriptions: HashMap<String, IRI>,
    /// Named-class frames carrying owl:oneOf: every list head, in
    /// document order (#115). Two or more oneOf lists on one class are
    /// N class equalities under the RDF-based reading (the W3C
    /// description-logic-501/502 TorF lattice idiom), each flushed to
    /// a bidirectional SubClassOf pair; single-list frames keep
    /// today's behavior.
    #[cfg(feature = "rio-xml")]
    pending_named_class_enumerations: HashMap<String, Vec<ProcessedObject>>,
    /// Named-class frames carrying owl:disjointUnionOf: subject key ->
    /// every list head (N lists are N axioms, each lowering to its own
    /// equivalence-plus-disjointness pair — the #115 multi-list
    /// precedent); the flush drops frames without two named members
    /// (#120).
    #[cfg(feature = "rio-xml")]
    pending_named_class_disjoint_unions: HashMap<String, Vec<ProcessedObject>>,
    #[cfg(feature = "rio-xml")]
    pending_negative_assertions: HashMap<String, PendingNegativePropertyAssertion>,
    #[cfg(feature = "rio-xml")]
    pending_disjoint_property_pairs: Vec<(IRI, IRI)>,
    #[cfg(feature = "rio-xml")]
    pending_different_individual_pairs: Vec<(IRI, IRI)>,
    #[cfg(feature = "rio-xml")]
    pending_property_chains: Vec<(IRI, ProcessedObject)>,
    #[cfg(feature = "rio-xml")]
    pending_inverse_property_expressions: HashMap<String, IRI>,
    #[cfg(feature = "rio-xml")]
    pending_equivalent_property_pairs: Vec<(IRI, IRI)>,
    #[cfg(feature = "rio-xml")]
    pending_same_individual_pairs: Vec<(IRI, IRI)>,
    #[cfg(feature = "rio-xml")]
    pending_rdf_lists: HashMap<String, PendingRdfList>,
    #[cfg(feature = "rio-xml")]
    pending_facet_restrictions: HashMap<String, Vec<(IRI, Literal)>>,
    #[cfg(feature = "rio-xml")]
    pending_expression_conflicts: HashMap<String, Vec<String>>,
    #[cfg(feature = "rio-xml")]
    pending_has_key_pairs: Vec<(IRI, ProcessedObject)>,
    /// Anonymous nodes typed owl:AllDisjointClasses /
    /// owl:AllDisjointProperties / owl:AllDifferent: node key -> the
    /// axiom kind the marker stands for; the flush joins it with the
    /// node's members list head (#130).
    #[cfg(feature = "rio-xml")]
    pending_all_disjoint_kinds: HashMap<String, AllDisjointKind>,
    /// The owl:members / owl:distinctMembers list heads those marker
    /// nodes carry: node key -> list head. Heads on nodes that never
    /// receive a marker typing are dropped at the flush (#130).
    #[cfg(feature = "rio-xml")]
    pending_all_disjoint_heads: HashMap<String, ProcessedObject>,
}

impl RdfXmlStreamingParser {
    #[cfg(feature = "rio-xml")]
    fn shared_iri_const(value: &'static str) -> Arc<IRI> {
        Arc::new(IRI::new_unchecked(value).expect("static IRI constant must be valid"))
    }

    #[cfg(feature = "rio-xml")]
    fn predicate_cache_capacity() -> usize {
        std::env::var("OWL2_REASONER_PREDICATE_CACHE")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|&v| v > 0)
            .unwrap_or(16_384)
    }

    #[cfg(feature = "rio-xml")]
    fn object_cache_capacity() -> usize {
        std::env::var("OWL2_REASONER_OBJECT_IRI_CACHE")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|&v| v > 0)
            .unwrap_or(65_536)
    }

    #[cfg(feature = "rio-xml")]
    fn blank_node_cache_capacity() -> usize {
        std::env::var("OWL2_REASONER_BNODE_CACHE")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|&v| v > 0)
            .unwrap_or(65_536)
    }

    #[cfg(feature = "rio-xml")]
    fn shared_hot_iri(&self, iri: &str) -> Option<IRI> {
        match iri {
            RDF_TYPE_IRI => Some(self.rdf_type_shared.as_ref().clone()),
            RDFS_SUBCLASS_OF_IRI => Some(self.rdfs_subclass_shared.as_ref().clone()),
            RDFS_DOMAIN_IRI => Some(self.rdfs_domain_shared.as_ref().clone()),
            RDFS_RANGE_IRI => Some(self.rdfs_range_shared.as_ref().clone()),
            OWL_CLASS_IRI => Some(self.owl_class_shared.as_ref().clone()),
            OWL_OBJECT_PROPERTY_IRI => Some(self.owl_obj_prop_shared.as_ref().clone()),
            OWL_DATATYPE_PROPERTY_IRI => Some(self.owl_data_prop_shared.as_ref().clone()),
            OWL_NAMED_INDIVIDUAL_IRI => Some(self.owl_named_ind_shared.as_ref().clone()),
            OWL_DISJOINT_WITH_IRI => Some(self.owl_disjoint_shared.as_ref().clone()),
            OWL_EQUIVALENT_CLASS_IRI => Some(self.owl_equivalent_shared.as_ref().clone()),
            _ => None,
        }
    }

    #[cfg(feature = "rio-xml")]
    fn cached_predicate_iri(&mut self, iri: &str) -> OwlResult<IRI> {
        if let Some(shared) = self.shared_hot_iri(iri) {
            return Ok(shared);
        }
        if let Some(existing) = self.predicate_iri_cache.get(iri) {
            return Ok(existing.as_ref().clone());
        }

        let parsed = Arc::new(IRI::new(iri)?);
        if self.predicate_iri_cache.len() >= self.predicate_iri_cache_capacity {
            self.predicate_iri_cache.clear();
        }
        self.predicate_iri_cache
            .insert(iri.to_string(), Arc::clone(&parsed));
        Ok(parsed.as_ref().clone())
    }

    #[cfg(feature = "rio-xml")]
    fn cached_object_iri(&mut self, iri: &str) -> OwlResult<IRI> {
        if let Some(shared) = self.shared_hot_iri(iri) {
            return Ok(shared);
        }
        if let Some(existing) = self.object_iri_cache.get(iri) {
            return Ok(existing.as_ref().clone());
        }

        let parsed = Arc::new(IRI::new(iri)?);
        if self.object_iri_cache.len() >= self.object_iri_cache_capacity {
            self.object_iri_cache.clear();
        }
        self.object_iri_cache
            .insert(iri.to_string(), Arc::clone(&parsed));
        Ok(parsed.as_ref().clone())
    }

    #[cfg(feature = "rio-xml")]
    fn cached_blank_node_label(&mut self, raw_id: &str) -> Arc<str> {
        if let Some(existing) = self.blank_node_label_cache.get(raw_id) {
            return Arc::clone(existing);
        }

        let mut full = String::with_capacity(raw_id.len() + 2);
        full.push_str("_:");
        full.push_str(raw_id);
        let label: Arc<str> = Arc::from(full.into_boxed_str());

        if self.blank_node_label_cache.len() >= self.blank_node_cache_capacity {
            self.blank_node_label_cache.clear();
        }
        self.blank_node_label_cache
            .insert(raw_id.to_string(), Arc::clone(&label));
        label
    }

    #[cfg(feature = "rio-xml")]
    fn cached_blank_node_iri(&mut self, raw_id: &str) -> OwlResult<IRI> {
        if let Some(existing) = self.blank_node_iri_cache.get(raw_id) {
            return Ok(existing.as_ref().clone());
        }

        let label = self.cached_blank_node_label(raw_id);
        let iri = Arc::new(IRI::new(label.as_ref())?);
        if self.blank_node_iri_cache.len() >= self.blank_node_cache_capacity {
            self.blank_node_iri_cache.clear();
        }
        self.blank_node_iri_cache
            .insert(raw_id.to_string(), Arc::clone(&iri));
        Ok(iri.as_ref().clone())
    }

    #[cfg(feature = "rio-xml")]
    #[inline]
    fn predicate_tag(predicate_iri: &str) -> PredicateTag {
        match predicate_iri {
            RDF_TYPE_IRI => PredicateTag::RdfType,
            RDFS_SUBCLASS_OF_IRI => PredicateTag::RdfsSubClassOf,
            RDFS_SUBPROPERTY_OF_IRI => PredicateTag::RdfsSubPropertyOf,
            RDFS_DOMAIN_IRI => PredicateTag::RdfsDomain,
            RDFS_RANGE_IRI => PredicateTag::RdfsRange,
            OWL_DISJOINT_WITH_IRI => PredicateTag::OwlDisjointWith,
            OWL_EQUIVALENT_CLASS_IRI => PredicateTag::OwlEquivalentClass,
            OWL_INVERSE_OF_IRI => PredicateTag::OwlInverseOf,
            other if other.starts_with(NS_OWL) => PredicateTag::OwlOther,
            _ => PredicateTag::Other,
        }
    }

    /// Create a new streaming parser
    pub fn new(config: ParserConfig) -> Self {
        let namespaces = crate::parser::rdf_xml_common::initialize_namespaces(&config.prefixes);

        let arena = if config.use_arena_allocation {
            Some(
                ParserArenaBuilder::new()
                    .with_capacity(config.arena_capacity)
                    .build(),
            )
        } else {
            None
        };

        let progress_every = std::env::var("OWL2_REASONER_PARSE_PROGRESS_EVERY")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(1_000_000);
        let large_parse_enabled = Self::env_truthy("OWL2_REASONER_LARGE_PARSE");
        #[cfg(feature = "rio-xml")]
        let predicate_iri_cache_capacity = Self::predicate_cache_capacity();
        #[cfg(feature = "rio-xml")]
        let object_iri_cache_capacity = Self::object_cache_capacity();
        #[cfg(feature = "rio-xml")]
        let blank_node_cache_capacity = Self::blank_node_cache_capacity();

        Self {
            config,
            namespaces,
            base_iri: None,
            arena,
            triple_counter: 0,
            parse_started_at: None,
            progress_every,
            large_parse_enabled,
            #[cfg(feature = "rio-xml")]
            predicate_iri_cache: FxHashMap::with_capacity_and_hasher(
                predicate_iri_cache_capacity,
                Default::default(),
            ),
            #[cfg(feature = "rio-xml")]
            predicate_iri_cache_capacity,
            #[cfg(feature = "rio-xml")]
            object_iri_cache: FxHashMap::with_capacity_and_hasher(
                object_iri_cache_capacity,
                Default::default(),
            ),
            #[cfg(feature = "rio-xml")]
            object_iri_cache_capacity,
            #[cfg(feature = "rio-xml")]
            blank_node_label_cache: FxHashMap::with_capacity_and_hasher(
                blank_node_cache_capacity,
                Default::default(),
            ),
            #[cfg(feature = "rio-xml")]
            blank_node_iri_cache: FxHashMap::with_capacity_and_hasher(
                blank_node_cache_capacity,
                Default::default(),
            ),
            #[cfg(feature = "rio-xml")]
            blank_node_cache_capacity,
            #[cfg(feature = "rio-xml")]
            rdf_type_shared: Self::shared_iri_const(RDF_TYPE_IRI),
            #[cfg(feature = "rio-xml")]
            rdfs_subclass_shared: Self::shared_iri_const(RDFS_SUBCLASS_OF_IRI),
            #[cfg(feature = "rio-xml")]
            rdfs_domain_shared: Self::shared_iri_const(RDFS_DOMAIN_IRI),
            #[cfg(feature = "rio-xml")]
            rdfs_range_shared: Self::shared_iri_const(RDFS_RANGE_IRI),
            #[cfg(feature = "rio-xml")]
            owl_class_shared: Self::shared_iri_const(OWL_CLASS_IRI),
            #[cfg(feature = "rio-xml")]
            owl_obj_prop_shared: Self::shared_iri_const(OWL_OBJECT_PROPERTY_IRI),
            #[cfg(feature = "rio-xml")]
            owl_data_prop_shared: Self::shared_iri_const(OWL_DATATYPE_PROPERTY_IRI),
            #[cfg(feature = "rio-xml")]
            owl_named_ind_shared: Self::shared_iri_const(OWL_NAMED_INDIVIDUAL_IRI),
            #[cfg(feature = "rio-xml")]
            owl_disjoint_shared: Self::shared_iri_const(OWL_DISJOINT_WITH_IRI),
            #[cfg(feature = "rio-xml")]
            owl_equivalent_shared: Self::shared_iri_const(OWL_EQUIVALENT_CLASS_IRI),
            #[cfg(feature = "rio-xml")]
            last_subject_kind: None,
            #[cfg(feature = "rio-xml")]
            last_subject_value: String::new(),
            #[cfg(feature = "rio-xml")]
            last_subject_iri: None,
            #[cfg(feature = "rio-xml")]
            pending_functional_properties: HashSet::new(),
            #[cfg(feature = "rio-xml")]
            pending_class_assertions: Vec::new(),
            #[cfg(feature = "rio-xml")]
            pending_class_edges: Vec::new(),
            #[cfg(feature = "rio-xml")]
            pending_range_edges: Vec::new(),
            #[cfg(feature = "rio-xml")]
            pending_restrictions: HashMap::new(),
            #[cfg(feature = "rio-xml")]
            pending_named_class_descriptions: HashMap::new(),
            #[cfg(feature = "rio-xml")]
            pending_named_class_enumerations: HashMap::new(),
            pending_named_class_disjoint_unions: HashMap::new(),
            #[cfg(feature = "rio-xml")]
            pending_negative_assertions: HashMap::new(),
            #[cfg(feature = "rio-xml")]
            pending_disjoint_property_pairs: Vec::new(),
            #[cfg(feature = "rio-xml")]
            pending_different_individual_pairs: Vec::new(),
            pending_property_chains: Vec::new(),
            pending_inverse_property_expressions: HashMap::new(),
            pending_equivalent_property_pairs: Vec::new(),
            pending_same_individual_pairs: Vec::new(),
            #[cfg(feature = "rio-xml")]
            pending_rdf_lists: HashMap::new(),
            #[cfg(feature = "rio-xml")]
            pending_facet_restrictions: HashMap::new(),
            #[cfg(feature = "rio-xml")]
            pending_expression_conflicts: HashMap::new(),
            #[cfg(feature = "rio-xml")]
            pending_has_key_pairs: Vec::new(),
            pending_all_disjoint_kinds: HashMap::new(),
            pending_all_disjoint_heads: HashMap::new(),
        }
    }

    fn env_truthy(key: &str) -> bool {
        match std::env::var(key) {
            Ok(value) => {
                let value = value.trim().to_ascii_lowercase();
                !(value.is_empty() || value == "0" || value == "false" || value == "no")
            }
            Err(_) => false,
        }
    }

    fn env_is_set(key: &str) -> bool {
        match std::env::var(key) {
            Ok(value) => !value.trim().is_empty(),
            Err(_) => false,
        }
    }

    fn parse_io_progress_bytes() -> u64 {
        std::env::var("OWL2_REASONER_PARSE_IO_PROGRESS_BYTES")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(256 * 1024 * 1024)
    }

    #[cfg(feature = "rio-xml")]
    fn structural_enabled() -> bool {
        Self::env_truthy("OWL2_REASONER_STRUCTURAL_XML_PARSER")
    }

    #[cfg(feature = "rio-xml")]
    fn structural_auto_enabled() -> bool {
        if Self::env_is_set("OWL2_REASONER_STRUCTURAL_XML_AUTO") {
            return Self::env_truthy("OWL2_REASONER_STRUCTURAL_XML_AUTO");
        }
        true
    }

    #[cfg(feature = "rio-xml")]
    fn structural_auto_threshold_bytes() -> u64 {
        std::env::var("OWL2_REASONER_STRUCTURAL_XML_AUTO_THRESHOLD")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|&v| v > 0)
            .unwrap_or(STRUCTURAL_AUTO_THRESHOLD_BYTES_DEFAULT)
    }

    #[cfg(feature = "rio-xml")]
    fn structural_enabled_for_path(path: &Path) -> bool {
        if Self::env_is_set("OWL2_REASONER_STRUCTURAL_XML_PARSER") {
            return Self::structural_enabled();
        }
        if !Self::structural_auto_enabled() {
            return false;
        }
        std::fs::metadata(path)
            .map(|meta| meta.len() >= Self::structural_auto_threshold_bytes())
            .unwrap_or(false)
    }

    #[cfg(feature = "rio-xml")]
    fn structural_interner_capacity() -> usize {
        std::env::var("OWL2_REASONER_STRUCTURAL_XML_INTERNER")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|&v| v > 0)
            .unwrap_or(262_144)
    }

    #[cfg(feature = "rio-xml")]
    fn structural_breakdown_enabled() -> bool {
        Self::env_truthy("OWL2_REASONER_STRUCTURAL_BREAKDOWN")
    }

    #[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
    fn experimental_enabled() -> bool {
        Self::env_truthy("OWL2_REASONER_EXPERIMENTAL_XML_PARSER")
    }

    #[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
    fn experimental_strict_enabled() -> bool {
        Self::env_truthy("OWL2_REASONER_EXPERIMENTAL_XML_STRICT")
    }

    #[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
    fn experimental_batch_size() -> usize {
        std::env::var("OWL2_REASONER_EXPERIMENTAL_XML_BATCH")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|&v| v > 0)
            .unwrap_or(1000)
    }

    #[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
    fn experimental_queue_bound() -> usize {
        std::env::var("OWL2_REASONER_EXPERIMENTAL_XML_QUEUE")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|&v| v > 0)
            .unwrap_or(50)
    }

    #[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
    fn experimental_worker_count() -> usize {
        std::env::var("OWL2_REASONER_EXPERIMENTAL_XML_WORKERS")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|&v| v > 0)
            .unwrap_or_else(|| cmp::max(1, num_cpus::get().saturating_sub(1)))
    }

    #[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
    fn experimental_cache_size() -> usize {
        std::env::var("OWL2_REASONER_EXPERIMENTAL_XML_CACHE")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|&v| v > 0)
            .unwrap_or(20_000)
    }

    #[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
    fn encode_term_key(term: &RawTerm) -> Cow<'_, str> {
        match term {
            RawTerm::Iri(iri) => Cow::Owned(format!("I|{}", iri)),
            RawTerm::BNode(id) => Cow::Owned(format!("B|{}", id)),
            RawTerm::Literal {
                value,
                datatype,
                lang,
            } => {
                let datatype = datatype.as_deref().unwrap_or("");
                let lang = lang.as_deref().unwrap_or("");
                Cow::Owned(format!(
                    "L|{}|{}|{}|{}|{}|{}",
                    value.len(),
                    value,
                    datatype.len(),
                    datatype,
                    lang.len(),
                    lang
                ))
            }
        }
    }

    #[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
    fn read_len_prefixed_component<'a>(mut input: &'a str) -> OwlResult<(&'a str, &'a str)> {
        let sep = input.find('|').ok_or_else(|| {
            crate::core::error::OwlError::ParseError(
                "Invalid experimental key: missing length separator".to_string(),
            )
        })?;
        let len = input[..sep].parse::<usize>().map_err(|_| {
            crate::core::error::OwlError::ParseError(
                "Invalid experimental key: invalid component length".to_string(),
            )
        })?;
        input = &input[sep + 1..];
        if input.len() < len {
            return Err(crate::core::error::OwlError::ParseError(
                "Invalid experimental key: component shorter than declared length".to_string(),
            ));
        }
        let (component, rest) = input.split_at(len);
        Ok((component, rest))
    }

    #[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
    fn decode_term_key(key: &str) -> OwlResult<RawTerm> {
        if let Some(rest) = key.strip_prefix("I|") {
            return Ok(RawTerm::Iri(rest.to_string()));
        }
        if let Some(rest) = key.strip_prefix("B|") {
            return Ok(RawTerm::BNode(rest.to_string()));
        }
        if let Some(rest) = key.strip_prefix("L|") {
            let (value, rest) = Self::read_len_prefixed_component(rest)?;
            let rest = rest.strip_prefix('|').ok_or_else(|| {
                crate::core::error::OwlError::ParseError(
                    "Invalid experimental key: missing value separator".to_string(),
                )
            })?;

            let (datatype, rest) = Self::read_len_prefixed_component(rest)?;
            let rest = rest.strip_prefix('|').ok_or_else(|| {
                crate::core::error::OwlError::ParseError(
                    "Invalid experimental key: missing datatype separator".to_string(),
                )
            })?;

            let (lang, rest) = Self::read_len_prefixed_component(rest)?;
            if !rest.is_empty() {
                return Err(crate::core::error::OwlError::ParseError(
                    "Invalid experimental key: unexpected trailing data".to_string(),
                ));
            }

            return Ok(RawTerm::Literal {
                value: value.to_string(),
                datatype: if datatype.is_empty() {
                    None
                } else {
                    Some(datatype.to_string())
                },
                lang: if lang.is_empty() {
                    None
                } else {
                    Some(lang.to_string())
                },
            });
        }

        Err(crate::core::error::OwlError::ParseError(
            "Invalid experimental key prefix".to_string(),
        ))
    }

    #[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
    fn raw_term_from_subject(subject: Subject<'_>) -> Option<RawTerm> {
        match subject {
            Subject::NamedNode(nn) => Some(RawTerm::Iri(nn.iri.to_string())),
            Subject::BlankNode(bn) => Some(RawTerm::BNode(bn.id.to_string())),
            Subject::Triple(_) => None,
        }
    }

    #[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
    fn raw_term_from_object(object: Term<'_>) -> Option<RawTerm> {
        match object {
            Term::NamedNode(nn) => Some(RawTerm::Iri(nn.iri.to_string())),
            Term::BlankNode(bn) => Some(RawTerm::BNode(bn.id.to_string())),
            Term::Literal(lit) => match lit {
                rio_api::model::Literal::Simple { value } => Some(RawTerm::Literal {
                    value: value.to_string(),
                    datatype: None,
                    lang: None,
                }),
                rio_api::model::Literal::LanguageTaggedString { value, language } => {
                    Some(RawTerm::Literal {
                        value: value.to_string(),
                        datatype: None,
                        lang: Some(language.to_string()),
                    })
                }
                rio_api::model::Literal::Typed { value, datatype } => Some(RawTerm::Literal {
                    value: value.to_string(),
                    datatype: Some(datatype.iri.to_string()),
                    lang: None,
                }),
            },
            Term::Triple(_) => None,
        }
    }

    #[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
    fn ensure_experimental_strict_mode(strict_mode: bool, skipped: usize) -> OwlResult<()> {
        if strict_mode && skipped > 0 {
            return Err(crate::core::error::OwlError::ParseError(format!(
                "Experimental RDF/XML strict mode failed: {} unsupported triples skipped",
                skipped
            )));
        }
        Ok(())
    }

    #[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
    fn resolve_raw_term(
        interner: &ExperimentalInterner,
        term_id: TermId,
        raw_cache: &mut FxHashMap<TermId, RawTerm>,
    ) -> OwlResult<RawTerm> {
        if let Some(term) = raw_cache.get(&term_id) {
            return Ok(term.clone());
        }

        let term_key = interner.resolve(term_id).ok_or_else(|| {
            crate::core::error::OwlError::ParseError(format!(
                "Missing term id in interner: {}",
                term_id
            ))
        })?;
        let decoded = Self::decode_term_key(term_key.as_ref())?;
        raw_cache.insert(term_id, decoded.clone());
        Ok(decoded)
    }

    #[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
    fn resolve_subject_iri(
        interner: &ExperimentalInterner,
        term_id: TermId,
        raw_cache: &mut FxHashMap<TermId, RawTerm>,
        subject_cache: &mut FxHashMap<TermId, IRI>,
    ) -> OwlResult<IRI> {
        if let Some(subject_iri) = subject_cache.get(&term_id) {
            return Ok(subject_iri.clone());
        }

        let term = Self::resolve_raw_term(interner, term_id, raw_cache)?;
        let subject_iri = match term {
            RawTerm::Iri(iri) => IRI::new(iri)?,
            RawTerm::BNode(id) => IRI::new(format!("_:{}", id))?,
            RawTerm::Literal { .. } => {
                return Err(crate::core::error::OwlError::ParseError(
                    "Subject cannot be a literal".to_string(),
                ));
            }
        };
        subject_cache.insert(term_id, subject_iri.clone());
        Ok(subject_iri)
    }

    #[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
    fn resolve_predicate_iri(
        interner: &ExperimentalInterner,
        term_id: TermId,
        raw_cache: &mut FxHashMap<TermId, RawTerm>,
        predicate_cache: &mut FxHashMap<TermId, IRI>,
    ) -> OwlResult<IRI> {
        if let Some(predicate_iri) = predicate_cache.get(&term_id) {
            return Ok(predicate_iri.clone());
        }

        let term = Self::resolve_raw_term(interner, term_id, raw_cache)?;
        let predicate_iri = match term {
            RawTerm::Iri(iri) => IRI::new(iri)?,
            RawTerm::BNode(_) | RawTerm::Literal { .. } => {
                return Err(crate::core::error::OwlError::ParseError(
                    "Predicate must be an IRI".to_string(),
                ));
            }
        };
        predicate_cache.insert(term_id, predicate_iri.clone());
        Ok(predicate_iri)
    }

    #[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
    fn resolve_object(
        interner: &ExperimentalInterner,
        term_id: TermId,
        raw_cache: &mut FxHashMap<TermId, RawTerm>,
        object_cache: &mut FxHashMap<TermId, ProcessedObject>,
    ) -> OwlResult<ProcessedObject> {
        if let Some(object) = object_cache.get(&term_id) {
            return Ok(object.clone());
        }

        let term = Self::resolve_raw_term(interner, term_id, raw_cache)?;
        let object = match term {
            RawTerm::Iri(iri) => ProcessedObject::Iri(IRI::new(iri)?),
            RawTerm::BNode(id) => ProcessedObject::BlankNode(id),
            RawTerm::Literal {
                value,
                datatype,
                lang,
            } => {
                let literal = if let Some(lang) = lang {
                    Literal::lang_tagged(value, lang)
                } else if let Some(datatype) = datatype {
                    Literal::typed(value, IRI::new(datatype)?)
                } else {
                    Literal::simple(value)
                };
                ProcessedObject::Literal(literal)
            }
        };
        object_cache.insert(term_id, object.clone());
        Ok(object)
    }

    #[cfg(feature = "rio-xml")]
    fn resolve_structural_term<'a>(
        interner: &'a StructuralInterner,
        id: StructuralTermId,
    ) -> OwlResult<&'a str> {
        interner.resolve(id).ok_or_else(|| {
            crate::core::error::OwlError::ParseError(format!(
                "Missing structural term id in interner: {}",
                id
            ))
        })
    }

    #[cfg(feature = "rio-xml")]
    fn resolve_structural_object_iri(
        &mut self,
        interner: &StructuralInterner,
        id: StructuralTermId,
        iri_cache: &mut [Option<Arc<IRI>>],
    ) -> OwlResult<Arc<IRI>> {
        if let Some(Some(existing)) = iri_cache.get(id as usize) {
            return Ok(existing.clone());
        }

        let arc = Arc::new(IRI::new_unchecked(Self::resolve_structural_term(interner, id)?)?);
        let slot = iri_cache.get_mut(id as usize).ok_or_else(|| {
            crate::core::error::OwlError::ParseError(format!(
                "Missing structural cache slot for term id: {}",
                id
            ))
        })?;
        *slot = Some(arc.clone());
        Ok(arc)
    }

    #[cfg(feature = "rio-xml")]
    fn resolve_structural_predicate_iri(
        &mut self,
        interner: &StructuralInterner,
        id: StructuralTermId,
        predicate_cache: &mut [Option<Arc<IRI>>],
    ) -> OwlResult<Arc<IRI>> {
        if let Some(Some(existing)) = predicate_cache.get(id as usize) {
            return Ok(existing.clone());
        }

        let arc = Arc::new(IRI::new_unchecked(Self::resolve_structural_term(interner, id)?)?);
        let slot = predicate_cache.get_mut(id as usize).ok_or_else(|| {
            crate::core::error::OwlError::ParseError(format!(
                "Missing structural predicate cache slot for term id: {}",
                id
            ))
        })?;
        *slot = Some(arc.clone());
        Ok(arc)
    }

    #[cfg(feature = "rio-xml")]
    fn resolve_structural_subject_iri(
        &mut self,
        interner: &StructuralInterner,
        subject: StructuralSubjectRecord,
        bnode_cache: &mut [Option<Arc<IRI>>],
        iri_cache: &mut [Option<Arc<IRI>>],
    ) -> OwlResult<Arc<IRI>> {
        match subject {
            StructuralSubjectRecord::Iri(id) => {
                self.resolve_structural_object_iri(interner, id, iri_cache)
            }
            StructuralSubjectRecord::BNode(id) => {
                if let Some(Some(existing)) = bnode_cache.get(id as usize) {
                    return Ok(existing.clone());
                }

                let raw_id = Self::resolve_structural_term(interner, id)?;
                let mut label = String::with_capacity(raw_id.len() + 2);
                label.push_str("_:");
                label.push_str(raw_id);
                let arc = Arc::new(IRI::new_unchecked(label)?);
                let slot = bnode_cache.get_mut(id as usize).ok_or_else(|| {
                    crate::core::error::OwlError::ParseError(format!(
                        "Missing structural bnode cache slot for term id: {}",
                        id
                    ))
                })?;
                *slot = Some(arc.clone());
                Ok(arc)
            }
        }
    }

    #[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
    pub fn parse_content_experimental(&mut self, content: &str) -> OwlResult<Ontology> {
        let reader = std::io::BufReader::new(std::io::Cursor::new(content.as_bytes().to_vec()));
        self.parse_stream_experimental(reader)
    }

    #[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
    pub fn parse_file_experimental(&mut self, path: &Path) -> OwlResult<Ontology> {
        use std::fs::File;
        use std::io::BufReader;

        let file = File::open(path).map_err(crate::core::error::OwlError::IoError)?;
        let reader = BufReader::with_capacity(RDF_XML_BUF_CAPACITY, file);
        self.parse_stream_experimental(reader)
    }

    #[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
    pub fn parse_stream_experimental<R>(&mut self, reader: R) -> OwlResult<Ontology>
    where
        R: std::io::BufRead + Send,
    {
        self.triple_counter = 0;
        self.pending_functional_properties.clear();
        self.pending_class_assertions.clear();
        self.pending_class_edges.clear();
        self.pending_range_edges.clear();
        self.pending_restrictions.clear();
        self.pending_named_class_descriptions.clear();
        #[cfg(feature = "rio-xml")]
        self.pending_named_class_enumerations.clear();
        self.pending_named_class_disjoint_unions.clear();
        self.pending_negative_assertions.clear();
        self.pending_disjoint_property_pairs.clear();
        self.pending_all_disjoint_kinds.clear();
        self.pending_all_disjoint_heads.clear();
        self.pending_different_individual_pairs.clear();
        self.pending_property_chains.clear();
        self.pending_inverse_property_expressions.clear();
        self.pending_equivalent_property_pairs.clear();
        self.pending_same_individual_pairs.clear();
        self.pending_rdf_lists.clear();
        self.pending_facet_restrictions.clear();
        self.pending_expression_conflicts.clear();
        self.pending_has_key_pairs.clear();
        let started_at = Instant::now();
        self.parse_started_at = Some(started_at);

        let batch_size = Self::experimental_batch_size();
        let queue_bound = Self::experimental_queue_bound();
        let worker_count = Self::experimental_worker_count();
        let cache_size = Self::experimental_cache_size();
        let strict_mode = Self::experimental_strict_enabled();

        let stats = Arc::new(PipelineStats::default());
        let interner = Arc::new(ExperimentalInterner::new());
        let (tx_batch, rx_batch) = bounded::<RawBatch>(queue_bound);

        let mut worker_results: Vec<Vec<CompactTripleRecord>> = thread::scope(|scope| {
            let tx_producer = tx_batch.clone();
            let stats_producer = Arc::clone(&stats);
            let producer_handle = scope.spawn(move |_| -> Result<(), String> {
                let mut parser = RioRdfXmlParser::new(reader, None);
                let mut seq: u64 = 0;
                let mut current_batch = RawBatch::new(batch_size);

                let parse_result = parser.parse_all(&mut |triple| {
                    seq = seq.saturating_add(1);

                    let Some(s) = Self::raw_term_from_subject(triple.subject) else {
                        stats_producer
                            .skipped_triples
                            .fetch_add(1, Ordering::Relaxed);
                        if strict_mode {
                            return Err(std::io::Error::other(
                                "Unsupported RDF-star subject in strict mode",
                            ));
                        }
                        return Ok(());
                    };

                    let p = RawTerm::Iri(triple.predicate.iri.to_string());

                    let Some(o) = Self::raw_term_from_object(triple.object) else {
                        stats_producer
                            .skipped_triples
                            .fetch_add(1, Ordering::Relaxed);
                        if strict_mode {
                            return Err(std::io::Error::other(
                                "Unsupported RDF-star object in strict mode",
                            ));
                        }
                        return Ok(());
                    };

                    current_batch.push(RawTripleRecord { seq, s, p, o });

                    if current_batch.is_full(batch_size) {
                        let full_batch =
                            std::mem::replace(&mut current_batch, RawBatch::new(batch_size));
                        if tx_producer.send(full_batch).is_err() {
                            return Ok(());
                        }
                    }

                    Ok(())
                });

                if let Err(err) = parse_result {
                    return Err(format!("{}: {}", ERR_RIO_XML_PARSE, err));
                }

                if !current_batch.terms.is_empty() {
                    let _ = tx_producer.send(current_batch);
                }
                Ok(())
            });

            drop(tx_batch);

            let mut worker_handles = Vec::with_capacity(worker_count);
            for _ in 0..worker_count {
                let rx = rx_batch.clone();
                let interner = Arc::clone(&interner);
                let stats_worker = Arc::clone(&stats);
                worker_handles.push(scope.spawn(
                    move |_| -> Result<Vec<CompactTripleRecord>, String> {
                        let mut local = Vec::with_capacity(batch_size * 32);
                        let mut cache = GenerationalCache::new(cache_size);

                        while let Ok(batch) = rx.recv() {
                            for rec in batch.terms {
                                let s = cache.get_or_resolve(rec.s, |term| {
                                    let key = Self::encode_term_key(term);
                                    interner.get_or_intern(key.as_ref())
                                });
                                let p = cache.get_or_resolve(rec.p, |term| {
                                    let key = Self::encode_term_key(term);
                                    interner.get_or_intern(key.as_ref())
                                });
                                let o = cache.get_or_resolve(rec.o, |term| {
                                    let key = Self::encode_term_key(term);
                                    interner.get_or_intern(key.as_ref())
                                });

                                local.push(CompactTripleRecord {
                                    seq: rec.seq,
                                    s,
                                    p,
                                    o,
                                });
                                stats_worker.total_triples.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        Ok(local)
                    },
                ));
            }

            let producer_res = producer_handle.join().map_err(|_| {
                crate::core::error::OwlError::ParseError(
                    "Experimental producer thread panicked".to_string(),
                )
            })?;
            if let Err(err) = producer_res {
                return Err(crate::core::error::OwlError::ParseError(err));
            }

            let mut outputs = Vec::with_capacity(worker_handles.len());
            for handle in worker_handles {
                let worker_res = handle.join().map_err(|_| {
                    crate::core::error::OwlError::ParseError(
                        "Experimental worker thread panicked".to_string(),
                    )
                })?;
                match worker_res {
                    Ok(records) => outputs.push(records),
                    Err(err) => return Err(crate::core::error::OwlError::ParseError(err)),
                }
            }

            Ok(outputs)
        })
        .map_err(|_| {
            crate::core::error::OwlError::ParseError(
                "Experimental RDF/XML pipeline scope panicked".to_string(),
            )
        })??;

        Self::ensure_experimental_strict_mode(
            strict_mode,
            stats.skipped_triples.load(Ordering::Relaxed),
        )?;

        let mut compact_triples: Vec<CompactTripleRecord> =
            worker_results.drain(..).flatten().collect();
        compact_triples.sort_unstable_by_key(|t| t.seq);

        let large_parse_enabled = Self::env_truthy("OWL2_REASONER_LARGE_PARSE");
        if large_parse_enabled {
            eprintln!(
                "[phase] experimental_compact_ready triples={} skipped={} elapsed_ms={}",
                compact_triples.len(),
                stats.skipped_triples.load(Ordering::Relaxed),
                started_at.elapsed().as_millis()
            );
        }

        let mut raw_term_cache: FxHashMap<TermId, RawTerm> = FxHashMap::default();
        let mut subject_cache: FxHashMap<TermId, IRI> = FxHashMap::default();
        let mut predicate_cache: FxHashMap<TermId, IRI> = FxHashMap::default();
        let mut object_cache: FxHashMap<TermId, ProcessedObject> = FxHashMap::default();
        let materialize_started_at = Instant::now();
        let total_triples = compact_triples.len();
        let mut ontology = Ontology::new();
        for (idx, triple) in compact_triples.into_iter().enumerate() {
            let subject_iri = Self::resolve_subject_iri(
                &interner,
                triple.s,
                &mut raw_term_cache,
                &mut subject_cache,
            )?;
            let predicate_iri = Self::resolve_predicate_iri(
                &interner,
                triple.p,
                &mut raw_term_cache,
                &mut predicate_cache,
            )?;
            let object =
                Self::resolve_object(&interner, triple.o, &mut raw_term_cache, &mut object_cache)?;

            self.apply_triple_terms(&mut ontology, subject_iri, predicate_iri, object)?;

            if large_parse_enabled && (idx + 1).is_multiple_of(1_000_000) {
                eprintln!(
                    "[phase] experimental_materialize_progress triples={} elapsed_ms={}",
                    idx + 1,
                    materialize_started_at.elapsed().as_millis()
                );
            }
        }

        if large_parse_enabled {
            eprintln!(
                "[phase] experimental_materialize_done triples={} elapsed_ms={} cache_sizes(raw={} subject={} predicate={} object={})",
                total_triples,
                materialize_started_at.elapsed().as_millis(),
                raw_term_cache.len(),
                subject_cache.len(),
                predicate_cache.len(),
                object_cache.len()
            );
        }

        self.finalize_pending_functional_properties(&mut ontology)?;
        Ok(ontology)
    }

    /// Parse RDF/XML content using structural two-phase pipeline.
    #[cfg(feature = "rio-xml")]
    pub fn parse_content_structural(&mut self, content: &str) -> OwlResult<Ontology> {
        self.parse_stream_structural(Cursor::new(content.as_bytes()))
    }

    /// Parse RDF/XML file using structural two-phase pipeline.
    #[cfg(feature = "rio-xml")]
    pub fn parse_file_structural(&mut self, path: &Path) -> OwlResult<Ontology> {
        use std::fs::File;
        use std::io::BufReader;

        let file = File::open(path).map_err(crate::core::error::OwlError::IoError)?;
        let reader = BufReader::with_capacity(RDF_XML_BUF_CAPACITY, file);
        self.parse_stream_structural(reader)
    }

    /// The document's complete RDF graph as owned triples — every IRI,
    /// blank node, literal, list edge, and vocabulary triple, before
    /// any Direct-semantics lowering (ADR 0006's owned-graph boundary;
    /// the same stream Phase A interns for the structural pipeline).
    #[cfg(feature = "rio-xml")]
    pub fn extract_rdf_graph(&mut self, path: &Path) -> OwlResult<Vec<RdfGraphTriple>> {
        use std::fs::File;
        use std::io::BufReader;

        let file = File::open(path).map_err(crate::core::error::OwlError::IoError)?;
        let reader = BufReader::with_capacity(RDF_XML_BUF_CAPACITY, file);
        let base_iri = self
            .base_iri
            .as_ref()
            .and_then(|iri| oxiri::Iri::parse(iri.as_str().to_string()).ok());
        let mut parser = RioRdfXmlParser::new(reader, base_iri);
        let mut triples = Vec::new();
        let mut handler = |triple: Triple| -> Result<(), std::io::Error> {
            let subject = match triple.subject {
                Subject::NamedNode(node) => RdfGraphTerm::Iri(node.iri.to_string()),
                Subject::BlankNode(node) => RdfGraphTerm::Blank(node.id.to_string()),
                Subject::Triple(_) => {
                    return Err(std::io::Error::other(
                        "RDF-star triple subjects are outside the owned graph",
                    ))
                }
            };
            let object = match triple.object {
                Term::NamedNode(node) => RdfGraphTerm::Iri(node.iri.to_string()),
                Term::BlankNode(node) => RdfGraphTerm::Blank(node.id.to_string()),
                Term::Literal(rio_api::model::Literal::Simple { value }) => RdfGraphTerm::Literal {
                    lexical: value.to_string(),
                    datatype: None,
                    language: None,
                },
                Term::Literal(rio_api::model::Literal::LanguageTaggedString { value, language }) => {
                    RdfGraphTerm::Literal {
                        lexical: value.to_string(),
                        datatype: None,
                        language: Some(language.to_string()),
                    }
                }
                Term::Literal(rio_api::model::Literal::Typed { value, datatype }) => {
                    RdfGraphTerm::Literal {
                        lexical: value.to_string(),
                        datatype: Some(datatype.iri.to_string()),
                        language: None,
                    }
                }
                Term::Triple(_) => {
                    return Err(std::io::Error::other(
                        "RDF-star triple objects are outside the owned graph",
                    ))
                }
            };
            triples.push(RdfGraphTriple {
                subject,
                predicate: triple.predicate.iri.to_string(),
                object,
            });
            Ok(())
        };
        parser.parse_all(&mut handler).map_err(|e| {
            crate::core::error::OwlError::ParseError(format!("{}: {}", ERR_RIO_XML_PARSE, e))
        })?;
        Ok(triples)
    }

    /// Structural parser: phase A (ingest+intern), phase B (materialize axioms).
    #[cfg(feature = "rio-xml")]
    pub fn parse_stream_structural(
        &mut self,
        reader: impl std::io::BufRead,
    ) -> OwlResult<Ontology> {
        self.triple_counter = 0;
        self.pending_functional_properties.clear();
        self.pending_class_assertions.clear();
        self.pending_class_edges.clear();
        self.pending_range_edges.clear();
        self.pending_restrictions.clear();
        self.pending_named_class_descriptions.clear();
        #[cfg(feature = "rio-xml")]
        self.pending_named_class_enumerations.clear();
        self.pending_named_class_disjoint_unions.clear();
        self.pending_negative_assertions.clear();
        self.pending_disjoint_property_pairs.clear();
        self.pending_all_disjoint_kinds.clear();
        self.pending_all_disjoint_heads.clear();
        self.pending_different_individual_pairs.clear();
        self.pending_property_chains.clear();
        self.pending_inverse_property_expressions.clear();
        self.pending_equivalent_property_pairs.clear();
        self.pending_same_individual_pairs.clear();
        self.pending_rdf_lists.clear();
        self.pending_facet_restrictions.clear();
        self.pending_expression_conflicts.clear();
        self.pending_has_key_pairs.clear();
        let started_at = Instant::now();
        self.parse_started_at = Some(started_at);
        self.last_subject_kind = None;
        self.last_subject_value.clear();
        self.last_subject_iri = None;

        let base_iri = self
            .base_iri
            .as_ref()
            .and_then(|iri| oxiri::Iri::parse(iri.as_str().to_string()).ok());

        let reader = ProgressBufRead::new(
            reader,
            self.large_parse_enabled,
            Self::parse_io_progress_bytes(),
            started_at,
        );
        let mut parser = RioRdfXmlParser::new(reader, base_iri);

        let mut interner = StructuralInterner::with_capacity(Self::structural_interner_capacity());
        let mut triples: Vec<StructuralTripleRecord> = Vec::new();

        let mut subject_cursor: Option<StructuralSubjectCursor> = None;
        let intern_subject = |interner: &mut StructuralInterner,
                              cursor: &mut Option<StructuralSubjectCursor>,
                              kind: SubjectCacheKind,
                              lexical: &str|
         -> StructuralTermId {
            if let Some(state) = cursor.as_ref() {
                if state.matches(&kind, lexical) {
                    return state.term_id;
                }
            }
            let id = interner.intern(lexical);
            *cursor = Some(StructuralSubjectCursor {
                kind,
                lexical: lexical.to_string(),
                term_id: id,
            });
            id
        };
        let mut handler = |triple: Triple| -> Result<(), std::io::Error> {
            self.triple_counter = self.triple_counter.saturating_add(1);
            if self.large_parse_enabled
                && self.progress_every > 0
                && self.triple_counter.is_multiple_of(self.progress_every)
            {
                if let Some(started) = self.parse_started_at {
                    eprintln!(
                        "[phase] parse_progress triples={} elapsed_ms={}",
                        self.triple_counter,
                        started.elapsed().as_millis()
                    );
                }
            }

            let subject = match triple.subject {
                Subject::NamedNode(node) => StructuralSubjectRecord::Iri(intern_subject(
                    &mut interner,
                    &mut subject_cursor,
                    SubjectCacheKind::Named,
                    node.iri,
                )),
                Subject::BlankNode(node) => StructuralSubjectRecord::BNode(intern_subject(
                    &mut interner,
                    &mut subject_cursor,
                    SubjectCacheKind::Blank,
                    node.id,
                )),
                Subject::Triple(_) => {
                    return Err(std::io::Error::other(
                        "Structural RDF/XML parser mode does not support RDF-star triple subjects",
                    ))
                }
            };

            let predicate = interner.intern(triple.predicate.iri);
            let object = match triple.object {
                Term::NamedNode(node) => StructuralObjectRecord::Iri(interner.intern(node.iri)),
                Term::BlankNode(node) => StructuralObjectRecord::BNode(interner.intern(node.id)),
                Term::Literal(literal) => match literal {
                    rio_api::model::Literal::Simple { value } => StructuralObjectRecord::Literal {
                        value: interner.intern(value),
                        datatype: None,
                        lang: None,
                    },
                    rio_api::model::Literal::LanguageTaggedString { value, language } => {
                        StructuralObjectRecord::Literal {
                            value: interner.intern(value),
                            datatype: None,
                            lang: Some(interner.intern(language)),
                        }
                    }
                    rio_api::model::Literal::Typed { value, datatype } => {
                        StructuralObjectRecord::Literal {
                            value: interner.intern(value),
                            datatype: Some(interner.intern(datatype.iri)),
                            lang: None,
                        }
                    }
                },
                Term::Triple(_) => {
                    return Err(std::io::Error::other(
                        "Structural RDF/XML parser mode does not support RDF-star triple objects",
                    ))
                }
            };

            triples.push(StructuralTripleRecord {
                subject,
                predicate,
                object,
            });
            Ok(())
        };

        parser.parse_all(&mut handler).map_err(|e| {
            crate::core::error::OwlError::ParseError(format!("{}: {}", ERR_RIO_XML_PARSE, e))
        })?;

        if self.large_parse_enabled {
            eprintln!(
                "[phase] structural_ingest_done triples={} terms={} elapsed_ms={}",
                triples.len(),
                interner.reverse.len(),
                started_at.elapsed().as_millis()
            );
        }
        let materialize_started_at = Instant::now();
        let mut ontology = Ontology::new();
        // Millions of corpus assertions through add_axiom would pay
        // the per-axiom runtime-index churn each; one rebuild at the
        // end leaves the identical index state (#162).
        ontology.begin_deferred_runtime_indexes();
        let term_capacity = interner.reverse.len();
        let mut bnode_subject_cache: Vec<Option<Arc<IRI>>> = vec![None; term_capacity];
        let mut object_iri_cache: Vec<Option<Arc<IRI>>> = vec![None; term_capacity];
        let mut predicate_cache: Vec<Option<Arc<IRI>>> = vec![None; term_capacity];
        let mut bnode_object_cache: Vec<Option<String>> = vec![None; term_capacity];
        let breakdown_enabled = Self::structural_breakdown_enabled();
        let mut subject_ns: u128 = 0;
        let mut predicate_ns: u128 = 0;
        let mut object_ns: u128 = 0;
        let mut apply_ns: u128 = 0;
        let mut apply_rdf_type_ns: u128 = 0;
        let mut apply_subclass_ns: u128 = 0;
        let mut apply_subproperty_ns: u128 = 0;
        let mut apply_domain_ns: u128 = 0;
        let mut apply_range_ns: u128 = 0;
        let mut apply_owl_family_ns: u128 = 0;
        let mut apply_other_ns: u128 = 0;
        let mut apply_rdf_type_count: u64 = 0;
        let mut apply_subclass_count: u64 = 0;
        let mut apply_subproperty_count: u64 = 0;
        let mut apply_domain_count: u64 = 0;
        let mut apply_range_count: u64 = 0;
        let mut apply_owl_family_count: u64 = 0;
        let mut apply_other_count: u64 = 0;

        for (idx, triple) in triples.into_iter().enumerate() {
            let mut phase_started = if breakdown_enabled {
                Some(Instant::now())
            } else {
                None
            };
            let subject_arc = self.resolve_structural_subject_iri(
                &interner,
                triple.subject,
                &mut bnode_subject_cache,
                &mut object_iri_cache,
            )?;
            if let Some(started) = phase_started.take() {
                subject_ns = subject_ns.saturating_add(started.elapsed().as_nanos());
            }

            phase_started = if breakdown_enabled {
                Some(Instant::now())
            } else {
                None
            };
            let predicate_iri_str = Self::resolve_structural_term(&interner, triple.predicate)?;
            let predicate_tag = Self::predicate_tag(predicate_iri_str);
            let parsed_predicate = if matches!(predicate_tag, PredicateTag::Other) {
                Some(self.resolve_structural_predicate_iri(
                    &interner,
                    triple.predicate,
                    &mut predicate_cache,
                )?)
            } else {
                None
            };
            if let Some(started) = phase_started.take() {
                predicate_ns = predicate_ns.saturating_add(started.elapsed().as_nanos());
            }

            phase_started = if breakdown_enabled {
                Some(Instant::now())
            } else {
                None
            };
            let object = match triple.object {
                StructuralObjectRecord::Iri(id) => {
                    // Objects stay owned in ProcessedObject this round;
                    // unwrapping the cached Arc costs the same clone the
                    // pre-Arc cache already paid (#164).
                    let arc =
                        self.resolve_structural_object_iri(&interner, id, &mut object_iri_cache)?;
                    ProcessedObject::Iri((*arc).clone())
                }
                StructuralObjectRecord::BNode(id) => {
                    if let Some(Some(existing)) = bnode_object_cache.get(id as usize) {
                        ProcessedObject::BlankNode(existing.clone())
                    } else {
                        let label = Self::resolve_structural_term(&interner, id)?.to_string();
                        let slot = bnode_object_cache.get_mut(id as usize).ok_or_else(|| {
                            crate::core::error::OwlError::ParseError(format!(
                                "Missing structural bnode object cache slot for term id: {}",
                                id
                            ))
                        })?;
                        *slot = Some(label.clone());
                        ProcessedObject::BlankNode(label)
                    }
                }
                StructuralObjectRecord::Literal {
                    value,
                    datatype,
                    lang,
                } => {
                    let value = Self::resolve_structural_term(&interner, value)?.to_string();
                    let literal = if let Some(lang_id) = lang {
                        Literal::lang_tagged(
                            value,
                            Self::resolve_structural_term(&interner, lang_id)?.to_string(),
                        )
                    } else if let Some(datatype_id) = datatype {
                        let datatype_iri = self.resolve_structural_object_iri(
                            &interner,
                            datatype_id,
                            &mut object_iri_cache,
                        )?;
                        Literal::typed(value, datatype_iri)
                    } else {
                        Literal::simple(value)
                    };
                    ProcessedObject::Literal(literal)
                }
            };
            if let Some(started) = phase_started.take() {
                object_ns = object_ns.saturating_add(started.elapsed().as_nanos());
            }

            phase_started = if breakdown_enabled {
                Some(Instant::now())
            } else {
                None
            };
            self.apply_triple_terms_core(
                &mut ontology,
                subject_arc,
                predicate_iri_str,
                predicate_tag,
                parsed_predicate,
                object,
            )?;
            if let Some(started) = phase_started.take() {
                let elapsed_ns = started.elapsed().as_nanos();
                apply_ns = apply_ns.saturating_add(elapsed_ns);
                match predicate_tag {
                    PredicateTag::RdfType => {
                        apply_rdf_type_ns = apply_rdf_type_ns.saturating_add(elapsed_ns);
                        apply_rdf_type_count = apply_rdf_type_count.saturating_add(1);
                    }
                    PredicateTag::RdfsSubClassOf => {
                        apply_subclass_ns = apply_subclass_ns.saturating_add(elapsed_ns);
                        apply_subclass_count = apply_subclass_count.saturating_add(1);
                    }
                    PredicateTag::RdfsSubPropertyOf => {
                        apply_subproperty_ns = apply_subproperty_ns.saturating_add(elapsed_ns);
                        apply_subproperty_count = apply_subproperty_count.saturating_add(1);
                    }
                    PredicateTag::RdfsDomain => {
                        apply_domain_ns = apply_domain_ns.saturating_add(elapsed_ns);
                        apply_domain_count = apply_domain_count.saturating_add(1);
                    }
                    PredicateTag::RdfsRange => {
                        apply_range_ns = apply_range_ns.saturating_add(elapsed_ns);
                        apply_range_count = apply_range_count.saturating_add(1);
                    }
                    PredicateTag::OwlDisjointWith
                    | PredicateTag::OwlEquivalentClass
                    | PredicateTag::OwlInverseOf
                    | PredicateTag::OwlOther => {
                        apply_owl_family_ns = apply_owl_family_ns.saturating_add(elapsed_ns);
                        apply_owl_family_count = apply_owl_family_count.saturating_add(1);
                    }
                    PredicateTag::Other => {
                        apply_other_ns = apply_other_ns.saturating_add(elapsed_ns);
                        apply_other_count = apply_other_count.saturating_add(1);
                    }
                }
            }

            if self.large_parse_enabled && (idx + 1).is_multiple_of(1_000_000) {
                eprintln!(
                    "[phase] structural_materialize_progress triples={} elapsed_ms={}",
                    idx + 1,
                    materialize_started_at.elapsed().as_millis()
                );
            }
        }

        if self.large_parse_enabled {
            eprintln!(
                "[phase] structural_materialize_done elapsed_ms={} caches(subject_bnode={} iri={} predicate={} bnode_obj={})",
                materialize_started_at.elapsed().as_millis(),
                bnode_subject_cache
                    .iter()
                    .filter(|slot| slot.is_some())
                    .count(),
                object_iri_cache.iter().filter(|slot| slot.is_some()).count(),
                predicate_cache.iter().filter(|slot| slot.is_some()).count(),
                bnode_object_cache
                    .iter()
                    .filter(|slot| slot.is_some())
                    .count()
            );
        }

        if breakdown_enabled {
            let total_ns = materialize_started_at.elapsed().as_nanos();
            let tracked_ns = subject_ns
                .saturating_add(predicate_ns)
                .saturating_add(object_ns)
                .saturating_add(apply_ns);
            let other_ns = total_ns.saturating_sub(tracked_ns);
            let pct = |part: u128, total: u128| -> f64 {
                if total == 0 {
                    0.0
                } else {
                    (part as f64) * 100.0 / (total as f64)
                }
            };
            eprintln!(
                "[phase] structural_materialize_breakdown total_ns={} subject_ns={} predicate_ns={} object_ns={} apply_ns={} other_ns={} subject_pct={:.2} predicate_pct={:.2} object_pct={:.2} apply_pct={:.2} other_pct={:.2}",
                total_ns,
                subject_ns,
                predicate_ns,
                object_ns,
                apply_ns,
                other_ns,
                pct(subject_ns, total_ns),
                pct(predicate_ns, total_ns),
                pct(object_ns, total_ns),
                pct(apply_ns, total_ns),
                pct(other_ns, total_ns),
            );
            eprintln!(
                "[phase] structural_apply_breakdown total_apply_ns={} rdf_type_ns={} rdf_type_count={} subclass_ns={} subclass_count={} subproperty_ns={} subproperty_count={} domain_ns={} domain_count={} range_ns={} range_count={} owl_family_ns={} owl_family_count={} other_ns={} other_count={} rdf_type_pct={:.2} subclass_pct={:.2} subproperty_pct={:.2} domain_pct={:.2} range_pct={:.2} owl_family_pct={:.2} other_pct={:.2}",
                apply_ns,
                apply_rdf_type_ns,
                apply_rdf_type_count,
                apply_subclass_ns,
                apply_subclass_count,
                apply_subproperty_ns,
                apply_subproperty_count,
                apply_domain_ns,
                apply_domain_count,
                apply_range_ns,
                apply_range_count,
                apply_owl_family_ns,
                apply_owl_family_count,
                apply_other_ns,
                apply_other_count,
                pct(apply_rdf_type_ns, apply_ns),
                pct(apply_subclass_ns, apply_ns),
                pct(apply_subproperty_ns, apply_ns),
                pct(apply_domain_ns, apply_ns),
                pct(apply_range_ns, apply_ns),
                pct(apply_owl_family_ns, apply_ns),
                pct(apply_other_ns, apply_ns),
            );
        }

        // The pending flushes below add their own axioms, which must
        // land in an eagerly indexed ontology — close the deferred
        // window before them (#162).
        ontology.finish_deferred_runtime_indexes();
        self.finalize_pending_functional_properties(&mut ontology)?;
        Ok(ontology)
    }

    /// Parse RDF/XML content using streaming approach
    #[cfg(feature = "rio-xml")]
    pub fn parse_content(&mut self, content: &str) -> OwlResult<Ontology> {
        #[cfg(feature = "experimental-xml-parser")]
        if Self::experimental_enabled() {
            return self.parse_content_experimental(content);
        }
        if Self::structural_enabled() {
            return self.parse_content_structural(content);
        }

        self.triple_counter = 0;
        self.pending_functional_properties.clear();
        self.pending_class_assertions.clear();
        self.pending_class_edges.clear();
        self.pending_range_edges.clear();
        self.pending_restrictions.clear();
        self.pending_named_class_descriptions.clear();
        #[cfg(feature = "rio-xml")]
        self.pending_named_class_enumerations.clear();
        self.pending_named_class_disjoint_unions.clear();
        self.pending_negative_assertions.clear();
        self.pending_disjoint_property_pairs.clear();
        self.pending_all_disjoint_kinds.clear();
        self.pending_all_disjoint_heads.clear();
        self.pending_different_individual_pairs.clear();
        self.pending_property_chains.clear();
        self.pending_inverse_property_expressions.clear();
        self.pending_equivalent_property_pairs.clear();
        self.pending_same_individual_pairs.clear();
        self.pending_rdf_lists.clear();
        self.pending_facet_restrictions.clear();
        self.pending_expression_conflicts.clear();
        self.pending_has_key_pairs.clear();
        self.parse_started_at = Some(Instant::now());
        self.last_subject_kind = None;
        self.last_subject_value.clear();
        self.last_subject_iri = None;
        let mut ontology = Ontology::new();

        let base_iri = self
            .base_iri
            .as_ref()
            .and_then(|iri| oxiri::Iri::parse(iri.as_str().to_string()).ok());

        let mut parser = RioRdfXmlParser::new(Cursor::new(content), base_iri);

        let mut handler = |triple: Triple| -> Result<(), std::io::Error> {
            self.process_triple(&mut ontology, triple)
                .map_err(std::io::Error::other)
        };

        parser.parse_all(&mut handler).map_err(|e| {
            crate::core::error::OwlError::ParseError(format!("{}: {}", ERR_RIO_XML_PARSE, e))
        })?;

        self.finalize_pending_functional_properties(&mut ontology)?;
        Ok(ontology)
    }

    /// Parse RDF/XML file using streaming approach
    #[cfg(feature = "rio-xml")]
    pub fn parse_file(&mut self, path: &Path) -> OwlResult<Ontology> {
        use std::fs::File;
        use std::io::BufReader;

        #[cfg(feature = "experimental-xml-parser")]
        if Self::experimental_enabled() {
            return self.parse_file_experimental(path);
        }
        if Self::structural_enabled_for_path(path) {
            return self.parse_file_structural(path);
        }

        let file = File::open(path).map_err(crate::core::error::OwlError::IoError)?;

        let reader = BufReader::with_capacity(RDF_XML_BUF_CAPACITY, file);
        self.parse_stream(reader)
    }

    /// Parse RDF/XML from a reader using streaming approach
    #[cfg(feature = "rio-xml")]
    pub fn parse_stream(&mut self, reader: impl std::io::BufRead) -> OwlResult<Ontology> {
        if Self::structural_enabled() {
            return self.parse_stream_structural(reader);
        }

        self.triple_counter = 0;
        self.pending_functional_properties.clear();
        self.pending_class_assertions.clear();
        self.pending_class_edges.clear();
        self.pending_range_edges.clear();
        self.pending_restrictions.clear();
        self.pending_named_class_descriptions.clear();
        #[cfg(feature = "rio-xml")]
        self.pending_named_class_enumerations.clear();
        self.pending_named_class_disjoint_unions.clear();
        self.pending_negative_assertions.clear();
        self.pending_disjoint_property_pairs.clear();
        self.pending_all_disjoint_kinds.clear();
        self.pending_all_disjoint_heads.clear();
        self.pending_different_individual_pairs.clear();
        self.pending_property_chains.clear();
        self.pending_inverse_property_expressions.clear();
        self.pending_equivalent_property_pairs.clear();
        self.pending_same_individual_pairs.clear();
        self.pending_rdf_lists.clear();
        self.pending_facet_restrictions.clear();
        self.pending_expression_conflicts.clear();
        self.pending_has_key_pairs.clear();
        let started_at = Instant::now();
        self.parse_started_at = Some(started_at);
        self.last_subject_kind = None;
        self.last_subject_value.clear();
        self.last_subject_iri = None;
        let mut ontology = Ontology::new();

        let base_iri = self
            .base_iri
            .as_ref()
            .and_then(|iri| oxiri::Iri::parse(iri.as_str().to_string()).ok());

        let reader = ProgressBufRead::new(
            reader,
            self.large_parse_enabled,
            Self::parse_io_progress_bytes(),
            started_at,
        );
        let mut parser = RioRdfXmlParser::new(reader, base_iri);

        let mut handler = |triple: Triple| -> Result<(), std::io::Error> {
            self.process_triple(&mut ontology, triple)
                .map_err(std::io::Error::other)
        };

        parser.parse_all(&mut handler).map_err(|e| {
            crate::core::error::OwlError::ParseError(format!("{}: {}", ERR_RIO_XML_PARSE, e))
        })?;

        self.finalize_pending_functional_properties(&mut ontology)?;
        Ok(ontology)
    }

    /// Process a single triple and add to ontology
    #[cfg(feature = "rio-xml")]
    fn process_triple(&mut self, ontology: &mut Ontology, triple: Triple) -> OwlResult<()> {
        self.triple_counter += 1;
        if self.large_parse_enabled
            && self.progress_every > 0
            && self.triple_counter.is_multiple_of(self.progress_every)
        {
            if let Some(started) = self.parse_started_at {
                eprintln!(
                    "[phase] parse_progress triples={} elapsed_ms={}",
                    self.triple_counter,
                    started.elapsed().as_millis()
                );
            } else {
                eprintln!("[phase] parse_progress triples={}", self.triple_counter);
            }
        }

        let subject_iri = self.subject_to_iri_with_cursor(&triple.subject)?;
        let predicate_iri_str = triple.predicate.iri;
        let predicate_tag = Self::predicate_tag(predicate_iri_str);
        let object = self.process_object(&triple.object)?;

        self.apply_triple_terms_core(
            ontology,
            Arc::new(subject_iri),
            predicate_iri_str,
            predicate_tag,
            None,
            object,
        )
    }

    #[cfg(feature = "rio-xml")]
    fn subject_to_iri_with_cursor(&mut self, subject: &Subject) -> OwlResult<IRI> {
        match subject {
            Subject::NamedNode(node) => {
                if self.last_subject_kind == Some(SubjectCacheKind::Named)
                    && self.last_subject_value == node.iri
                {
                    if let Some(cached_iri) = &self.last_subject_iri {
                        return Ok(cached_iri.clone());
                    }
                }

                let iri = self.cached_object_iri(node.iri)?;
                self.last_subject_kind = Some(SubjectCacheKind::Named);
                self.last_subject_value.clear();
                self.last_subject_value.push_str(node.iri);
                self.last_subject_iri = Some(iri.clone());
                Ok(iri)
            }
            Subject::BlankNode(node) => {
                if self.last_subject_kind == Some(SubjectCacheKind::Blank)
                    && self.last_subject_value == node.id
                {
                    if let Some(cached_iri) = &self.last_subject_iri {
                        return Ok(cached_iri.clone());
                    }
                }

                let iri = self.cached_blank_node_iri(node.id)?;
                self.last_subject_kind = Some(SubjectCacheKind::Blank);
                self.last_subject_value.clear();
                self.last_subject_value.push_str(node.id);
                self.last_subject_iri = Some(iri.clone());
                Ok(iri)
            }
            Subject::Triple(_) => self.subject_to_iri(subject),
        }
    }

    #[cfg(feature = "rio-xml")]
    fn apply_triple_terms_core(
        &mut self,
        ontology: &mut Ontology,
        subject_arc: Arc<IRI>,
        predicate_iri_str: &str,
        predicate_tag: PredicateTag,
        parsed_predicate: Option<Arc<IRI>>,
        object: ProcessedObject,
    ) -> OwlResult<()> {
        match predicate_tag {
            PredicateTag::RdfType => {
                self.handle_type_assertion(ontology, &subject_arc, &object)?;
            }
            PredicateTag::RdfsSubClassOf => {
                self.handle_subclass_of(ontology, &subject_arc, &object)?;
            }
            PredicateTag::RdfsSubPropertyOf => {
                if let Some(object_iri) = object.as_iri() {
                    self.handle_subproperty_of(ontology, &subject_arc, object_iri)?;
                }
            }
            PredicateTag::OwlInverseOf => {
                if let Some(object_iri) = object.as_iri() {
                    if subject_arc.as_str().starts_with("_:") {
                        // Anonymous inverse property expression: recorded
                        // for chain-member materialization (#165) alongside
                        // the existing named-property handling.
                        self.pending_inverse_property_expressions.insert(
                            Self::restriction_node_key(subject_arc.as_str()),
                            object_iri.clone(),
                        );
                    }
                    self.handle_inverse_of(ontology, &subject_arc, object_iri)?;
                }
            }
            PredicateTag::RdfsDomain => {
                if let Some(object_iri) = object.as_iri() {
                    self.handle_domain(ontology, &subject_arc, object_iri)?;
                }
            }
            PredicateTag::RdfsRange => {
                if let Some(object_iri) = object.as_iri() {
                    self.handle_range(ontology, &subject_arc, object_iri)?;
                } else {
                    // Expression ranges resolve at the flush, once the
                    // object's oneOf/list triples are all recorded (#150).
                    self.pending_range_edges
                        .push(((*subject_arc).clone(), object));
                }
            }
            PredicateTag::OwlDisjointWith
            | PredicateTag::OwlEquivalentClass
            | PredicateTag::OwlOther => {
                self.handle_owl_property(
                    ontology,
                    &subject_arc,
                    predicate_iri_str,
                    predicate_tag,
                    &object,
                )?;
            }
            PredicateTag::Other => {
                let predicate_arc = match parsed_predicate {
                    Some(arc) => arc,
                    None => Arc::new(self.cached_predicate_iri(predicate_iri_str)?),
                };
                if self
                    .record_expression_other_property(&subject_arc, predicate_iri_str, &object)
                {
                    return Ok(());
                }
                self.handle_property_assertion(ontology, &subject_arc, &predicate_arc, &object)?;
            }
        }

        Ok(())
    }

    #[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
    fn apply_triple_terms(
        &mut self,
        ontology: &mut Ontology,
        subject_iri: IRI,
        predicate_iri: IRI,
        object: ProcessedObject,
    ) -> OwlResult<()> {
        let predicate_str = predicate_iri.as_str().to_string();
        let predicate_tag = Self::predicate_tag(&predicate_str);
        self.apply_triple_terms_core(
            ontology,
            Arc::new(subject_iri),
            &predicate_str,
            predicate_tag,
            Some(Arc::new(predicate_iri)),
            object,
        )
    }

    /// Convert Rio subject to IRI
    #[cfg(feature = "rio-xml")]
    fn subject_to_iri(&mut self, subject: &Subject) -> OwlResult<IRI> {
        match subject {
            Subject::NamedNode(node) => self.cached_object_iri(node.iri),
            Subject::BlankNode(node) => self.cached_blank_node_iri(node.id),
            Subject::Triple(triple) => {
                // Handle RDF-star triple subjects by creating a reification IRI
                // This represents a statement about another statement
                let subject_iri = self.subject_to_iri(&triple.subject)?;
                let predicate_iri = self.cached_predicate_iri(triple.predicate.iri)?;
                let object = self.process_object(&triple.object)?;

                // Create a reified statement identifier
                // Format: _:reified_<subject>_<predicate>_<object>
                let object_str = match &object {
                    ProcessedObject::Iri(iri) => iri.as_str().to_string(),
                    ProcessedObject::BlankNode(id) => {
                        self.cached_blank_node_label(id).as_ref().to_string()
                    }
                    ProcessedObject::Literal(lit) => {
                        // For literals, create a simple representation
                        format!("\"{}\"", lit.lexical_form())
                    }
                };

                // Create a unique identifier for the reified statement
                let reified_id = format!(
                    "_:reified_{}_{}_{}",
                    subject_iri
                        .as_str()
                        .replace("http://", "")
                        .replace("https://", "")
                        .replace("/", "_")
                        .replace("#", "_"),
                    predicate_iri
                        .as_str()
                        .replace("http://", "")
                        .replace("https://", "")
                        .replace("/", "_")
                        .replace("#", "_"),
                    object_str
                        .replace("http://", "")
                        .replace("https://", "")
                        .replace("/", "_")
                        .replace("#", "_")
                        .replace("\"", "")
                );

                // Truncate if too long to avoid unreasonable IRIs
                let reified_id = if reified_id.len() > 200 {
                    use std::hash::{Hash, Hasher};
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    reified_id.hash(&mut hasher);
                    format!("_:reified_triple_{:x}", hasher.finish())
                } else {
                    reified_id
                };

                IRI::new(reified_id)
            }
        }
    }

    /// Process object term
    #[cfg(feature = "rio-xml")]
    fn process_object(&mut self, term: &Term) -> OwlResult<ProcessedObject> {
        match term {
            Term::NamedNode(node) => Ok(ProcessedObject::Iri(self.cached_object_iri(node.iri)?)),
            Term::BlankNode(node) => Ok(ProcessedObject::BlankNode(node.id.to_string())),
            Term::Literal(literal) => {
                let processed_literal = match literal {
                    rio_api::model::Literal::Simple { value } => Literal::simple(value.to_string()),
                    rio_api::model::Literal::LanguageTaggedString { value, language } => {
                        Literal::lang_tagged(value.to_string(), language.to_string())
                    }
                    rio_api::model::Literal::Typed { value, datatype } => {
                        Literal::typed(value.to_string(), self.cached_object_iri(datatype.iri)?)
                    }
                };
                Ok(ProcessedObject::Literal(processed_literal))
            }
            Term::Triple(triple) => {
                // Handle RDF-star triple terms by creating a reified statement object
                // This represents a statement used as an object
                let subject_iri = self.subject_to_iri(&triple.subject)?;
                let predicate_iri = self.cached_predicate_iri(triple.predicate.iri)?;
                let object = self.process_object(&triple.object)?;

                // Create a reified statement identifier for the triple term
                let object_str = match &object {
                    ProcessedObject::Iri(iri) => iri.as_str().to_string(),
                    ProcessedObject::BlankNode(id) => {
                        self.cached_blank_node_label(id).as_ref().to_string()
                    }
                    ProcessedObject::Literal(lit) => {
                        format!("\"{}\"", lit.lexical_form())
                    }
                };

                // Create a unique identifier for the reified statement
                let reified_id = format!(
                    "_:reified_triple_term_{}_{}_{}",
                    subject_iri
                        .as_str()
                        .replace("http://", "")
                        .replace("https://", "")
                        .replace("/", "_")
                        .replace("#", "_"),
                    predicate_iri
                        .as_str()
                        .replace("http://", "")
                        .replace("https://", "")
                        .replace("/", "_")
                        .replace("#", "_"),
                    object_str
                        .replace("http://", "")
                        .replace("https://", "")
                        .replace("/", "_")
                        .replace("#", "_")
                        .replace("\"", "")
                );

                // Truncate if too long to avoid unreasonable IRIs
                let reified_id = if reified_id.len() > 200 {
                    use std::hash::{Hash, Hasher};
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    reified_id.hash(&mut hasher);
                    format!("_:reified_triple_term_{:x}", hasher.finish())
                } else {
                    reified_id
                };

                // Return as a blank node with the reified statement identifier
                Ok(ProcessedObject::BlankNode(reified_id))
            }
        }
    }

    /// Handle type assertions (rdf:type)
    #[cfg(feature = "rio-xml")]
    fn handle_type_assertion(
        &mut self,
        ontology: &mut Ontology,
        subject: &Arc<IRI>,
        object: &ProcessedObject,
    ) -> OwlResult<()> {
        let Some(object_iri) = object.as_iri() else {
            if let ProcessedObject::BlankNode(node_id) = object {
                self.pending_class_assertions.push(PendingClassAssertion {
                    individual: (**subject).clone(),
                    restriction_node: Self::restriction_node_key(node_id),
                });
            }
            return Ok(());
        };

        // An owl:AllDisjoint*/owl:AllDifferent marker node stands for one
        // axiom over its members collection; record the kind and let the
        // flush lower it. Named marker nodes keep the generic residue —
        // #130's narrow gate.
        if let Some(kind) = AllDisjointKind::from_marker_iri(object_iri.as_str()) {
            if subject.as_str().starts_with("_:") {
                self.pending_all_disjoint_kinds
                    .entry(Self::restriction_node_key(subject.as_str()))
                    .or_insert(kind);
            } else {
                Self::add_generic_type_assertion(ontology, subject, object_iri)?;
            }
            return Ok(());
        }

        match object_iri.as_str() {
            OWL_CLASS_IRI => {
                if !ontology.contains_class_iri(subject) {
                    let class = <Class as Entity>::from_shared_iri(subject.clone());
                    ontology.add_class(class)?;
                }
            }
            OWL_OBJECT_PROPERTY_IRI => {
                let property =
                    <ObjectProperty as Entity>::from_shared_iri(subject.clone());
                ontology.add_object_property(property)?;
                self.resolve_pending_functional_property(ontology, subject, false)?;
            }
            OWL_DATATYPE_PROPERTY_IRI => {
                let property = <DataProperty as Entity>::from_shared_iri(subject.clone());
                ontology.add_data_property(property)?;
                self.resolve_pending_functional_property(ontology, subject, true)?;
            }
            OWL_FUNCTIONAL_PROPERTY_IRI => {
                self.handle_functional_property_type(ontology, subject)?;
            }
            OWL_INVERSE_FUNCTIONAL_PROPERTY_IRI => self.handle_object_property_characteristic(
                ontology,
                subject,
                ObjectPropertyCharacteristicKind::InverseFunctional,
            )?,
            OWL_REFLEXIVE_PROPERTY_IRI => self.handle_object_property_characteristic(
                ontology,
                subject,
                ObjectPropertyCharacteristicKind::Reflexive,
            )?,
            OWL_IRREFLEXIVE_PROPERTY_IRI => self.handle_object_property_characteristic(
                ontology,
                subject,
                ObjectPropertyCharacteristicKind::Irreflexive,
            )?,
            OWL_SYMMETRIC_PROPERTY_IRI => self.handle_object_property_characteristic(
                ontology,
                subject,
                ObjectPropertyCharacteristicKind::Symmetric,
            )?,
            OWL_ASYMMETRIC_PROPERTY_IRI => self.handle_object_property_characteristic(
                ontology,
                subject,
                ObjectPropertyCharacteristicKind::Asymmetric,
            )?,
            OWL_TRANSITIVE_PROPERTY_IRI => self.handle_object_property_characteristic(
                ontology,
                subject,
                ObjectPropertyCharacteristicKind::Transitive,
            )?,
            OWL_NAMED_INDIVIDUAL_IRI => {
                if !ontology.contains_named_individual_iri(subject) {
                    let individual =
                        <NamedIndividual as Entity>::from_shared_iri(subject.clone());
                    ontology.add_named_individual(individual)?;
                }
                // A typed node element is an rdf:type triple like any other;
                // the declaration participates in downstream reasoning (the
                // patched track's reflexive-role universe is exactly the
                // declared individuals).
                let named_individual_class =
                    <Class as Entity>::from_shared_iri(Arc::new(crate::core::iri::IRI::new(
                        OWL_NAMED_INDIVIDUAL_IRI,
                    )?));
                ontology.add_class_assertion(ClassAssertionAxiom::new(
                    subject.clone(),
                    ClassExpression::Class(named_individual_class),
                ))?;
            }
            OWL_RESTRICTION_IRI => {
                self.pending_restrictions
                    .entry(Self::restriction_node_key(subject.as_str()))
                    .or_default();
            }
            OWL_NEGATIVE_PROPERTY_ASSERTION_IRI => {
                self.pending_negative_assertions
                    .entry(Self::restriction_node_key(subject.as_str()))
                    .or_default();
            }
            _ => {
                Self::add_generic_type_assertion(ontology, subject, object_iri)?;
            }
        }
        Ok(())
    }

    /// The generic type assertion: the subject becomes a named
    /// individual asserted into the type's class.
    fn add_generic_type_assertion(
        ontology: &mut Ontology,
        subject: &IRI,
        object_iri: &IRI,
    ) -> OwlResult<()> {
        if !ontology.contains_named_individual_iri(subject) {
            ontology.add_named_individual(<NamedIndividual as Entity>::from_shared_iri(
                Arc::new(subject.clone()),
            ))?;
        }
        let class = <Class as Entity>::from_shared_iri(object_iri.clone().into());
        let assertion = ClassAssertionAxiom::new(
            Arc::new(subject.clone()),
            ClassExpression::Class(class),
        );
        ontology.add_class_assertion(assertion)?;
        Ok(())
    }

    /// Materialize the ambiguous RDF `owl:FunctionalProperty` type after the
    /// property's object/data kind is known.
    #[cfg(feature = "rio-xml")]
    fn handle_functional_property_type(
        &mut self,
        ontology: &mut Ontology,
        subject: &IRI,
    ) -> OwlResult<()> {
        if ontology
            .data_properties()
            .iter()
            .any(|property| property.iri().as_ref() == subject)
        {
            return self.add_functional_property_axiom(ontology, subject, true);
        }
        if ontology
            .object_properties()
            .iter()
            .any(|property| property.iri().as_ref() == subject)
        {
            return self.add_functional_property_axiom(ontology, subject, false);
        }

        self.pending_functional_properties.insert(subject.clone());
        Ok(())
    }

    #[cfg(feature = "rio-xml")]
    fn resolve_pending_functional_property(
        &mut self,
        ontology: &mut Ontology,
        subject: &IRI,
        is_data_property: bool,
    ) -> OwlResult<()> {
        if self.pending_functional_properties.remove(subject) {
            self.add_functional_property_axiom(ontology, subject, is_data_property)?;
        }
        Ok(())
    }

    #[cfg(feature = "rio-xml")]
    fn restriction_node_key(value: &str) -> String {
        value.strip_prefix("_:").unwrap_or(value).to_string()
    }

    #[cfg(feature = "rio-xml")]
    fn class_term_from_subject(subject: &IRI) -> ProcessedObject {
        if subject.as_str().starts_with("_:") {
            ProcessedObject::BlankNode(Self::restriction_node_key(subject.as_str()))
        } else {
            ProcessedObject::Iri(subject.clone())
        }
    }

    #[cfg(feature = "rio-xml")]
    fn parse_error(message: impl Into<String>) -> crate::core::error::OwlError {
        crate::core::error::OwlError::AxiomValidationError {
            axiom_type: "RDF/XML class expression".to_string(),
            message: message.into(),
        }
    }

    #[cfg(feature = "rio-xml")]
    fn describe_processed_object(value: &ProcessedObject) -> String {
        match value {
            ProcessedObject::Iri(iri) => format!("iri({})", iri.as_str()),
            ProcessedObject::BlankNode(node_id) => format!("blank({})", node_id),
            ProcessedObject::Literal(literal) => format!("literal({})", literal.lexical_form()),
        }
    }

    #[cfg(feature = "rio-xml")]
    fn push_unique_processed(values: &mut Vec<ProcessedObject>, value: &ProcessedObject) {
        if !values.iter().any(|existing| existing == value) {
            values.push(value.clone());
        }
    }

    #[cfg(feature = "rio-xml")]
    fn push_unique_cardinality(
        values: &mut Vec<PendingCardinality>,
        kind: PendingCardinalityKind,
        qualified: bool,
        value: &ProcessedObject,
    ) {
        let cardinality = PendingCardinality {
            kind,
            qualified,
            value: value.clone(),
        };
        if !values.iter().any(|existing| existing == &cardinality) {
            values.push(cardinality);
        }
    }

    #[cfg(feature = "rio-xml")]
    fn set_once_or_same<T: Clone + PartialEq>(slot: &mut Option<T>, value: T) -> bool {
        match slot {
            Some(existing) if existing == &value => false,
            Some(_) => true,
            None => {
                *slot = Some(value);
                false
            }
        }
    }

    #[cfg(feature = "rio-xml")]
    fn record_pending_expression_conflict(
        &mut self,
        node_key: &str,
        predicate_iri: &str,
        existing: &str,
        incoming: &str,
    ) {
        self.pending_expression_conflicts
            .entry(node_key.to_string())
            .or_default()
            .push(format!(
                "Conflicting RDF/XML values for {predicate_iri} at blank node {node_key}: existing {existing}, incoming {incoming}"
            ));
    }

    #[cfg(feature = "rio-xml")]
    fn ensure_no_pending_expression_conflict(&self, node_key: &str) -> OwlResult<()> {
        if let Some(conflicts) = self.pending_expression_conflicts.get(node_key) {
            if let Some(message) = conflicts.first() {
                return Err(Self::parse_error(message.clone()));
            }
        }
        Ok(())
    }

    #[cfg(feature = "rio-xml")]
    fn record_expression_other_property(
        &mut self,
        subject: &IRI,
        predicate_iri: &str,
        object: &ProcessedObject,
    ) -> bool {
        if !subject.as_str().starts_with("_:") {
            return false;
        }

        if matches!(predicate_iri, RDF_FIRST_IRI | RDF_REST_IRI) {
            let key = Self::restriction_node_key(subject.as_str());
            let list = self.pending_rdf_lists.entry(key.clone()).or_default();
            let conflict = match predicate_iri {
                RDF_FIRST_IRI => Self::set_once_or_same(&mut list.first, object.clone()),
                RDF_REST_IRI => Self::set_once_or_same(&mut list.rest, object.clone()),
                _ => unreachable!(),
            };
            if conflict {
                self.record_pending_expression_conflict(
                    &key,
                    predicate_iri,
                    "different value",
                    &Self::describe_processed_object(object),
                );
            }
            return true;
        }

        if predicate_iri.starts_with("http://www.w3.org/2001/XMLSchema#") {
            if let ProcessedObject::Literal(value) = object {
                if let Ok(facet) = IRI::new(predicate_iri) {
                    let key = Self::restriction_node_key(subject.as_str());
                    let restrictions = self
                        .pending_facet_restrictions
                        .entry(key.clone())
                        .or_default();
                    if restrictions.iter().any(|(existing_facet, existing_value)| {
                        existing_facet == &facet && existing_value == value
                    }) {
                        return true;
                    }
                    if restrictions
                        .iter()
                        .any(|(existing_facet, _)| existing_facet == &facet)
                    {
                        self.record_pending_expression_conflict(
                            &key,
                            predicate_iri,
                            "different literal",
                            value.lexical_form(),
                        );
                        return true;
                    }
                    restrictions.push((facet, value.clone()));
                    return true;
                }
            }
        }
        false
    }

    #[cfg(feature = "rio-xml")]
    fn record_restriction_property(
        &mut self,
        subject: &IRI,
        predicate_iri: &str,
        object: &ProcessedObject,
    ) {
        if predicate_iri == OWL_PROPERTY_DISJOINT_WITH_IRI {
            if let Some(object) = object.as_iri() {
                self.pending_disjoint_property_pairs
                    .push((subject.clone(), object.clone()));
            }
            return;
        }

        if predicate_iri == OWL_SAME_AS_IRI {
            if let Some(object) = object.as_iri() {
                self.pending_same_individual_pairs
                    .push((subject.clone(), object.clone()));
            }
            return;
        }

        if predicate_iri == OWL_DIFFERENT_FROM_IRI {
            if let Some(object) = object.as_iri() {
                self.pending_different_individual_pairs
                    .push((subject.clone(), object.clone()));
            }
            return;
        }

        if predicate_iri == OWL_HAS_KEY_IRI {
            self.pending_has_key_pairs
                .push((subject.clone(), object.clone()));
            return;
        }

        if predicate_iri == OWL_EQUIVALENT_PROPERTY_IRI {
            if let (true, Some(object)) = (!subject.as_str().starts_with("_:"), object.as_iri()) {
                self.pending_equivalent_property_pairs
                    .push((subject.clone(), object.clone()));
            }
            return;
        }

        if predicate_iri == OWL_PROPERTY_CHAIN_AXIOM_IRI {
            // The chain's list cells are captured by the shared rdf:first/
            // rdf:rest machinery; the axiom is materialized once parsing
            // completes.
            if !subject.as_str().starts_with("_:") {
                self.pending_property_chains
                    .push((subject.clone(), object.clone()));
            }
            return;
        }

        if matches!(
            predicate_iri,
            OWL_SOURCE_INDIVIDUAL_IRI
                | OWL_ASSERTION_PROPERTY_IRI
                | OWL_TARGET_INDIVIDUAL_IRI
                | OWL_TARGET_VALUE_IRI
        ) {
            let assertion = self
                .pending_negative_assertions
                .entry(Self::restriction_node_key(subject.as_str()))
                .or_default();
            match predicate_iri {
                OWL_SOURCE_INDIVIDUAL_IRI => {
                    if let Some(value) = object.as_iri() {
                        assertion.source_individual = Some(value.clone());
                    }
                }
                OWL_ASSERTION_PROPERTY_IRI => {
                    if let Some(value) = object.as_iri() {
                        assertion.assertion_property = Some(value.clone());
                    }
                }
                OWL_TARGET_INDIVIDUAL_IRI | OWL_TARGET_VALUE_IRI => {
                    assertion.target = Some(object.clone());
                }
                _ => unreachable!(),
            }
            return;
        }

        let restriction = self
            .pending_restrictions
            .entry(Self::restriction_node_key(subject.as_str()))
            .or_default();
        match predicate_iri {
            OWL_ON_PROPERTY_IRI => {
                if let Some(property) = object.as_iri() {
                    if Self::set_once_or_same(&mut restriction.on_property, property.clone()) {
                        self.record_pending_expression_conflict(
                            &Self::restriction_node_key(subject.as_str()),
                            predicate_iri,
                            "different property",
                            property.as_str(),
                        );
                    }
                }
            }
            OWL_SOME_VALUES_FROM_IRI => {
                if Self::set_once_or_same(&mut restriction.some_values_from, object.clone()) {
                    self.record_pending_expression_conflict(
                        &Self::restriction_node_key(subject.as_str()),
                        predicate_iri,
                        "different filler",
                        &Self::describe_processed_object(object),
                    );
                }
            }
            OWL_ALL_VALUES_FROM_IRI => {
                if Self::set_once_or_same(&mut restriction.all_values_from, object.clone()) {
                    self.record_pending_expression_conflict(
                        &Self::restriction_node_key(subject.as_str()),
                        predicate_iri,
                        "different filler",
                        &Self::describe_processed_object(object),
                    );
                }
            }
            OWL_HAS_VALUE_IRI => {
                if Self::set_once_or_same(&mut restriction.has_value, object.clone()) {
                    self.record_pending_expression_conflict(
                        &Self::restriction_node_key(subject.as_str()),
                        predicate_iri,
                        "different value",
                        &Self::describe_processed_object(object),
                    );
                }
            }
            OWL_COMPLEMENT_OF_IRI => {
                if Self::set_once_or_same(&mut restriction.complement_of, object.clone()) {
                    self.record_pending_expression_conflict(
                        &Self::restriction_node_key(subject.as_str()),
                        predicate_iri,
                        "different operand",
                        &Self::describe_processed_object(object),
                    );
                }
            }
            OWL_UNION_OF_IRI => {
                if Self::set_once_or_same(&mut restriction.union_of, object.clone()) {
                    self.record_pending_expression_conflict(
                        &Self::restriction_node_key(subject.as_str()),
                        predicate_iri,
                        "different list head",
                        &Self::describe_processed_object(object),
                    );
                }
            }
            OWL_INTERSECTION_OF_IRI => {
                // Description-form class definitions (a named class
                // carrying a bare owl:intersectionOf triple) are flushed
                // as equivalences at the end of the parse.
                if !subject.as_str().starts_with("_:") {
                    self.pending_named_class_descriptions
                        .entry(Self::restriction_node_key(subject.as_str()))
                        .or_insert_with(|| subject.clone());
                }
                if Self::set_once_or_same(&mut restriction.intersection_of, object.clone()) {
                    self.record_pending_expression_conflict(
                        &Self::restriction_node_key(subject.as_str()),
                        predicate_iri,
                        "different list head",
                        &Self::describe_processed_object(object),
                    );
                }
            }
            OWL_ON_DATATYPE_IRI => {
                if let Some(datatype) = object.as_iri() {
                    if Self::set_once_or_same(&mut restriction.on_datatype, datatype.clone()) {
                        self.record_pending_expression_conflict(
                            &Self::restriction_node_key(subject.as_str()),
                            predicate_iri,
                            "different datatype",
                            datatype.as_str(),
                        );
                    }
                }
            }
            OWL_WITH_RESTRICTIONS_IRI => {
                if Self::set_once_or_same(&mut restriction.with_restrictions, object.clone()) {
                    self.record_pending_expression_conflict(
                        &Self::restriction_node_key(subject.as_str()),
                        predicate_iri,
                        "different restrictions list",
                        &Self::describe_processed_object(object),
                    );
                }
            }
            OWL_ONE_OF_IRI => {
                if !subject.as_str().starts_with("_:") {
                    // A named-class frame's oneOf lists collect here;
                    // the flush lowers frames carrying two or more (the
                    // lattice idiom). The bnode set-once path below
                    // stays for anonymous descriptions used as
                    // expressions.
                    let key = Self::restriction_node_key(subject.as_str());
                    self.pending_named_class_descriptions
                        .entry(key.clone())
                        .or_insert_with(|| subject.clone());
                    self.pending_named_class_enumerations
                        .entry(key)
                        .or_default()
                        .push(object.clone());
                } else if Self::set_once_or_same(&mut restriction.one_of, object.clone()) {
                    self.record_pending_expression_conflict(
                        &Self::restriction_node_key(subject.as_str()),
                        predicate_iri,
                        "different list head",
                        &Self::describe_processed_object(object),
                    );
                }
            }
            OWL_DISJOINT_UNION_OF_IRI => {
                if !subject.as_str().starts_with("_:") {
                    // A named-class frame's disjointUnionOf list head
                    // collects here; the flush lowers frames whose
                    // members are all named classes to the
                    // equivalence-plus-disjointness pair (#120). The
                    // bnode path keeps today's silently-ignored
                    // behavior.
                    let key = Self::restriction_node_key(subject.as_str());
                    self.pending_named_class_descriptions
                        .entry(key.clone())
                        .or_insert_with(|| subject.clone());
                    self.pending_named_class_disjoint_unions
                        .entry(key)
                        .or_default()
                        .push(object.clone());
                }
            }
            OWL_MEMBERS_IRI | OWL_DISTINCT_MEMBERS_IRI => {
                // The members collections of owl:AllDisjoint* /
                // owl:AllDifferent marker nodes collect here; the flush
                // joins them with the typed kinds (#130). Heads on nodes
                // without a marker typing — and named subjects — keep
                // today's silently-ignored behavior.
                if subject.as_str().starts_with("_:") {
                    self.pending_all_disjoint_heads
                        .entry(Self::restriction_node_key(subject.as_str()))
                        .or_insert_with(|| object.clone());
                }
            }
            OWL_ON_CLASS_IRI => {
                Self::push_unique_processed(&mut restriction.on_class, object);
            }
            OWL_ON_DATA_RANGE_IRI => {
                Self::push_unique_processed(&mut restriction.on_data_range, object);
            }
            OWL_HAS_SELF_IRI => {
                Self::push_unique_processed(&mut restriction.has_self, object);
            }
            OWL_MIN_CARDINALITY_IRI => {
                Self::push_unique_cardinality(
                    &mut restriction.cardinalities,
                    PendingCardinalityKind::Min,
                    false,
                    object,
                );
            }
            OWL_MAX_CARDINALITY_IRI => {
                Self::push_unique_cardinality(
                    &mut restriction.cardinalities,
                    PendingCardinalityKind::Max,
                    false,
                    object,
                );
            }
            OWL_CARDINALITY_IRI => {
                Self::push_unique_cardinality(
                    &mut restriction.cardinalities,
                    PendingCardinalityKind::Exact,
                    false,
                    object,
                );
            }
            OWL_MIN_QUALIFIED_CARDINALITY_IRI => {
                Self::push_unique_cardinality(
                    &mut restriction.cardinalities,
                    PendingCardinalityKind::Min,
                    true,
                    object,
                );
            }
            OWL_MAX_QUALIFIED_CARDINALITY_IRI => {
                Self::push_unique_cardinality(
                    &mut restriction.cardinalities,
                    PendingCardinalityKind::Max,
                    true,
                    object,
                );
            }
            OWL_QUALIFIED_CARDINALITY_IRI => {
                Self::push_unique_cardinality(
                    &mut restriction.cardinalities,
                    PendingCardinalityKind::Exact,
                    true,
                    object,
                );
            }
            _ => {}
        }
    }

    #[cfg(feature = "rio-xml")]
    fn restriction_uses_data_property(
        ontology: &Ontology,
        property: &IRI,
        value: &ProcessedObject,
    ) -> bool {
        if let Some(is_data) = Self::declared_data_property(ontology, property) {
            return is_data;
        }

        matches!(
            value,
            ProcessedObject::Iri(iri)
                if iri.as_str() == "http://www.w3.org/2000/01/rdf-schema#Literal"
                    || iri.as_str().starts_with("http://www.w3.org/2001/XMLSchema#")
        )
    }

    #[cfg(feature = "rio-xml")]
    fn declared_data_property(ontology: &Ontology, property: &IRI) -> Option<bool> {
        if property.as_str() == OWL_BOTTOM_DATA_PROPERTY_IRI {
            return Some(true);
        }
        if property.as_str() == OWL_BOTTOM_OBJECT_PROPERTY_IRI {
            return Some(false);
        }
        if ontology
            .data_properties()
            .iter()
            .any(|candidate| candidate.iri().as_ref() == property)
        {
            return Some(true);
        }
        if ontology
            .object_properties()
            .iter()
            .any(|candidate| candidate.iri().as_ref() == property)
        {
            return Some(false);
        }
        None
    }

    /// Materialize captured owl:sameAs declarations as property assertions
    /// so the closure engine's equality rules see them.
    #[cfg(feature = "rio-xml")]
    fn materialize_pending_same_individual_pairs(
        &mut self,
        ontology: &mut Ontology,
    ) -> OwlResult<()> {
        let pending = std::mem::take(&mut self.pending_same_individual_pairs);
        for (subject, object) in pending {
            if !ontology.contains_named_individual_iri(&subject) {
                ontology.add_named_individual(<NamedIndividual as Entity>::from_shared_iri(
                    Arc::new(subject.clone()),
                ))?;
            }
            if !ontology.contains_named_individual_iri(&object) {
                ontology.add_named_individual(<NamedIndividual as Entity>::from_shared_iri(
                    Arc::new(object.clone()),
                ))?;
            }
            let predicate = IRI::new(OWL_SAME_AS_IRI)?;
            let assertion = PropertyAssertionAxiom::new(
                Arc::new(subject.clone()),
                Arc::new(predicate),
                Arc::new(object.clone()),
            );
            ontology.add_property_assertion(assertion)?;
        }
        Ok(())
    }

    /// Materialize captured owl:equivalentProperty declarations between
    /// object properties; the closure engine's equivalence machinery
    /// (scm-eqp1/2, prp-eqp1/2) takes it from there.
    #[cfg(feature = "rio-xml")]
    fn materialize_pending_equivalent_property_pairs(
        &mut self,
        ontology: &mut Ontology,
    ) -> OwlResult<()> {
        let pending = std::mem::take(&mut self.pending_equivalent_property_pairs);
        for (left, right) in pending {
            let axiom = crate::logic::axioms::EquivalentObjectPropertiesAxiom::new(vec![
                Arc::new(left.clone()),
                Arc::new(right.clone()),
            ]);
            ontology.add_axiom(crate::logic::axioms::Axiom::EquivalentObjectProperties(
                Box::new(axiom),
            ))?;
        }
        Ok(())
    }

    /// Materialize captured owl:propertyChainAxiom declarations into
    /// sub-property-chain axioms; the chain list must resolve to property
    /// IRIs. A blank-node member wrapped by owl:inverseOf is a legal OWL 2
    /// object-property expression; each one rides a fresh auxiliary role
    /// declared inverse to the wrapped property, a conservative rewrite the
    /// closure engine resolves by composing prp-inv1/2 with prp-spo2 (#165).
    #[cfg(feature = "rio-xml")]
    fn materialize_pending_property_chains(&mut self, ontology: &mut Ontology) -> OwlResult<()> {
        let pending = std::mem::take(&mut self.pending_property_chains);
        // BTree keyed on the wrapped IRI: one auxiliary role per distinct
        // inverse expression, materialized in a deterministic order. The
        // wrapped IRI rides along so emission never re-parses the key.
        let mut inverse_roles: BTreeMap<String, (IRI, IRI)> = BTreeMap::new();
        for (super_property, head) in pending {
            let members = self.resolve_rdf_list(&head)?;
            let mut chain = Vec::with_capacity(members.len());
            for member in members {
                let property = match member {
                    ProcessedObject::Iri(iri) => iri,
                    ProcessedObject::BlankNode(node_id) => {
                        let key = Self::restriction_node_key(&node_id);
                        let wrapped = self
                            .pending_inverse_property_expressions
                            .get(&key)
                            .cloned()
                            .ok_or_else(|| {
                                Self::parse_error(
                                    "Malformed owl:propertyChainAxiom: chain member is not a property expression",
                                )
                            })?;
                        let auxiliary = match inverse_roles
                            .entry(wrapped.as_str().to_string())
                        {
                            std::collections::btree_map::Entry::Occupied(entry) => {
                                entry.get().1.clone()
                            }
                            std::collections::btree_map::Entry::Vacant(entry) => {
                                let auxiliary =
                                    Self::inverse_property_expression_role(&wrapped)?;
                                entry.insert((wrapped.clone(), auxiliary.clone()));
                                auxiliary
                            }
                        };
                        auxiliary
                    }
                    ProcessedObject::Literal(_) => {
                        return Err(Self::parse_error(
                            "Malformed owl:propertyChainAxiom: chain member is not a property expression",
                        ));
                    }
                };
                chain.push(std::sync::Arc::new(property));
            }
            if chain.is_empty() {
                return Err(Self::parse_error(
                    "Malformed owl:propertyChainAxiom: empty property chain",
                ));
            }
            ontology.add_axiom(crate::logic::axioms::Axiom::SubPropertyChainOf(Box::new(
                crate::logic::axioms::SubPropertyChainOfAxiom::new(
                    chain,
                    std::sync::Arc::new(super_property.clone()),
                ),
            )))?;
        }
        for (_wrapped, (wrapped_iri, auxiliary)) in inverse_roles {
            let auxiliary_arc: std::sync::Arc<IRI> = std::sync::Arc::new(auxiliary);
            let role = <ObjectProperty as Entity>::from_shared_iri(auxiliary_arc.clone());
            ontology.add_object_property(role)?;
            let axiom = InverseObjectPropertiesAxiom::new(
                auxiliary_arc,
                std::sync::Arc::new(wrapped_iri),
            );
            ontology.add_axiom(crate::logic::axioms::Axiom::InverseObjectProperties(Box::new(
                axiom,
            )))?;
        }
        Ok(())
    }

    /// Deterministic auxiliary role IRI for an inverse property expression:
    /// FNV-1a over the wrapped IRI keeps repeated parses of the same
    /// ontology minting identical role names (collision-resistant, not
    /// collision-free — a 64-bit hash over property IRIs).
    #[cfg(feature = "rio-xml")]
    fn inverse_property_expression_role(wrapped: &IRI) -> OwlResult<IRI> {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in wrapped.as_str().bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        IRI::new(&format!("urn:tableauxx:inverse-role:{hash:016x}"))
    }

    #[cfg(feature = "rio-xml")]
    fn resolve_rdf_list(&self, head: &ProcessedObject) -> OwlResult<Vec<ProcessedObject>> {
        let mut values = Vec::new();
        let mut current = head.clone();
        let mut visited = HashSet::new();

        for _ in 0..100_000 {
            match current {
                ProcessedObject::Iri(iri) if iri.as_str() == RDF_NIL_IRI => return Ok(values),
                // The W3C New-Feature-Rational premises spell rdf:nil's
                // IRI without the local name (the bare rdf namespace
                // prefix); read it as the terminator the same way the
                // r13 parser did (#113).
                ProcessedObject::Iri(iri) if iri.as_str() == RDF_NS_IRI => return Ok(values),
                ProcessedObject::Iri(iri) => {
                    return Err(Self::parse_error(format!(
                        "Malformed RDF list: rest points to non-nil IRI {}",
                        iri.as_str()
                    )));
                }
                ProcessedObject::Literal(_) => {
                    return Err(Self::parse_error(
                        "Malformed RDF list: rest points to a literal",
                    ));
                }
                ProcessedObject::BlankNode(node_id) => {
                    let key = Self::restriction_node_key(&node_id);
                    self.ensure_no_pending_expression_conflict(&key)?;
                    if !visited.insert(key.clone()) {
                        return Err(Self::parse_error(format!(
                            "Cyclic RDF list at blank node {key}"
                        )));
                    }
                    let list = self.pending_rdf_lists.get(&key).ok_or_else(|| {
                        Self::parse_error(format!("Malformed RDF list: missing node {key}"))
                    })?;
                    let first = list.first.as_ref().ok_or_else(|| {
                        Self::parse_error(format!(
                            "Malformed RDF list: missing rdf:first for {key}"
                        ))
                    })?;
                    values.push(first.clone());
                    current = list
                        .rest
                        .as_ref()
                        .ok_or_else(|| {
                            Self::parse_error(format!(
                                "Malformed RDF list: missing rdf:rest for {key}"
                            ))
                        })?
                        .clone();
                }
            };
        }
        Err(Self::parse_error(
            "RDF list exceeds maximum traversal depth",
        ))
    }

    #[cfg(feature = "rio-xml")]
    fn resolve_data_range(&self, value: &ProcessedObject) -> OwlResult<Option<DataRange>> {
        match value {
            ProcessedObject::Iri(iri) => Ok(Some(DataRange::Datatype(iri.clone()))),
            ProcessedObject::BlankNode(node_id) => {
                self.ensure_no_pending_expression_conflict(&Self::restriction_node_key(node_id))?;
                let restriction = self
                    .pending_restrictions
                    .get(&Self::restriction_node_key(node_id));
                let Some(restriction) = restriction else {
                    return Ok(None);
                };
                if let Some(head) = &restriction.one_of {
                    let values = self.resolve_rdf_list(head)?;
                    let literals = values
                        .into_iter()
                        .map(|value| match value {
                            ProcessedObject::Literal(literal) => Some(literal),
                            _ => None,
                        })
                        .collect::<Option<Vec<_>>>()
                        .ok_or_else(|| {
                            Self::parse_error("Malformed owl:oneOf data range: non-literal item")
                        })?;
                    return Ok(Some(DataRange::DataOneOf(literals)));
                }
                if let Some(datatype) = &restriction.on_datatype {
                    let facets = if let Some(head) = &restriction.with_restrictions {
                        let mut facets = Vec::new();
                        for value in self.resolve_rdf_list(head)? {
                            let ProcessedObject::BlankNode(node_id) = value else {
                                return Err(Self::parse_error(
                                    "Malformed owl:withRestrictions list: non-facet item",
                                ));
                            };
                            let key = Self::restriction_node_key(&node_id);
                            self.ensure_no_pending_expression_conflict(&key)?;
                            let Some(restrictions) = self.pending_facet_restrictions.get(&key)
                            else {
                                return Err(Self::parse_error(
                                    "Malformed owl:withRestrictions list: non-facet item",
                                ));
                            };
                            facets.extend(restrictions.iter().cloned());
                        }
                        facets
                            .into_iter()
                            .map(|(facet, value)| FacetRestriction::new(facet, value))
                            .collect()
                    } else {
                        Vec::new()
                    };
                    return Ok(Some(DataRange::DatatypeRestriction(
                        datatype.clone(),
                        facets,
                    )));
                }
                Ok(None)
            }
            ProcessedObject::Literal(_) => Ok(None),
        }
    }

    #[cfg(feature = "rio-xml")]
    fn collapse_xml_schema_whitespace(value: &str) -> String {
        let mut collapsed = String::with_capacity(value.len());
        let mut in_whitespace = true;
        for ch in value.chars() {
            if matches!(ch, ' ' | '\t' | '\r' | '\n') {
                if !in_whitespace {
                    collapsed.push(' ');
                    in_whitespace = true;
                }
            } else {
                collapsed.push(ch);
                in_whitespace = false;
            }
        }
        if collapsed.ends_with(' ') {
            collapsed.pop();
        }
        collapsed
    }

    #[cfg(feature = "rio-xml")]
    fn supported_integer_datatype_bounds(datatype: &str) -> Option<(u128, u128)> {
        let local = datatype.strip_prefix(XSD_NAMESPACE_IRI)?;
        match local {
            "integer" | "nonNegativeInteger" => Some((0, u128::MAX)),
            // xsd:decimal with an integral lexical form (the W3C corpus
            // types cardinality literals this way); non-integral lexicals
            // are rejected by the digits validation after this lookup.
            "decimal" => Some((0, u128::MAX)),
            "positiveInteger" => Some((1, u128::MAX)),
            "nonPositiveInteger" => Some((0, 0)),
            "negativeInteger" => Some((1, 0)),
            "long" => Some((0, i64::MAX as u128)),
            "int" => Some((0, i32::MAX as u128)),
            "short" => Some((0, i16::MAX as u128)),
            "byte" => Some((0, i8::MAX as u128)),
            "unsignedLong" => Some((0, u64::MAX as u128)),
            "unsignedInt" => Some((0, u32::MAX as u128)),
            "unsignedShort" => Some((0, u16::MAX as u128)),
            "unsignedByte" => Some((0, u8::MAX as u128)),
            _ => None,
        }
    }

    #[cfg(feature = "rio-xml")]
    fn parse_non_negative_cardinality(node_id: &str, value: &ProcessedObject) -> OwlResult<u32> {
        let ProcessedObject::Literal(literal) = value else {
            return Err(Self::parse_error(format!(
                "Malformed cardinality at blank node {node_id}: expected literal"
            )));
        };
        let datatype = literal.datatype().as_str();
        let Some((datatype_min, datatype_max)) = Self::supported_integer_datatype_bounds(datatype)
        else {
            return Err(Self::parse_error(format!(
                "Malformed cardinality at blank node {node_id}: expected supported xsd integer datatype, got {}",
                datatype
            )));
        };
        let lexical = Self::collapse_xml_schema_whitespace(literal.lexical_form());
        let digits = lexical
            .strip_prefix('+')
            .or_else(|| lexical.strip_prefix('-'))
            .unwrap_or(&lexical);
        if digits.is_empty() || !digits.chars().all(|ch| ch.is_ascii_digit()) {
            return Err(Self::parse_error(format!(
                "Malformed cardinality at blank node {node_id}: invalid integer lexical value {}",
                literal.lexical_form()
            )));
        }
        let is_negative = lexical.starts_with('-');
        let magnitude = digits.parse::<u128>().map_err(|_| {
            crate::core::error::OwlError::ResourceLimitExceeded {
                resource_type: "RDF/XML cardinality value".to_string(),
                limit: u32::MAX as usize,
                message: format!(
                    "Cardinality value {lexical} at blank node {node_id} exceeds supported u32 bound"
                ),
            }
        })?;
        if is_negative && magnitude != 0 {
            return Err(Self::parse_error(format!(
                "Malformed cardinality at blank node {node_id}: value {} is not a nonnegative integer",
                lexical
            )));
        }
        if datatype_min > datatype_max || magnitude < datatype_min || magnitude > datatype_max {
            return Err(Self::parse_error(format!(
                "Malformed cardinality at blank node {node_id}: integer value {} is outside datatype range {}",
                lexical, datatype
            )));
        }
        u32::try_from(magnitude).map_err(|_| crate::core::error::OwlError::ResourceLimitExceeded {
            resource_type: "RDF/XML cardinality value".to_string(),
            limit: u32::MAX as usize,
            message: format!(
                "Cardinality value {lexical} at blank node {node_id} exceeds supported u32 bound"
            ),
        })
    }

    #[cfg(feature = "rio-xml")]
    fn parse_has_self(node_id: &str, value: &ProcessedObject) -> OwlResult<()> {
        let ProcessedObject::Literal(literal) = value else {
            return Err(Self::parse_error(format!(
                "Malformed owl:hasSelf at blank node {node_id}: expected boolean literal"
            )));
        };
        if literal.datatype().as_str() != XSD_BOOLEAN_IRI || literal.lexical_form() != "true" {
            return Err(Self::parse_error(format!(
                "Malformed owl:hasSelf at blank node {node_id}: expected xsd:boolean true"
            )));
        }
        Ok(())
    }

    #[cfg(feature = "rio-xml")]
    fn single_cardinality<'a>(
        node_id: &str,
        restriction: &'a PendingRestriction,
    ) -> OwlResult<Option<&'a PendingCardinality>> {
        match restriction.cardinalities.as_slice() {
            [] => Ok(None),
            [cardinality] => Ok(Some(cardinality)),
            _ => Err(Self::parse_error(format!(
                "Malformed cardinality restriction at blank node {node_id}: expected exactly one cardinality predicate"
            ))),
        }
    }

    #[cfg(feature = "rio-xml")]
    fn object_property_expression(property: IRI) -> ObjectPropertyExpression {
        ObjectPropertyExpression::ObjectProperty(Box::new(
            <ObjectProperty as Entity>::from_shared_iri(Arc::new(property)),
        ))
    }

    #[cfg(feature = "rio-xml")]
    fn data_property_expression(property: IRI) -> DataPropertyExpression {
        DataPropertyExpression::DataProperty(<DataProperty as Entity>::from_shared_iri(Arc::new(
            property,
        )))
    }

    #[cfg(feature = "rio-xml")]
    fn validate_single_restriction_family(
        node_id: &str,
        restriction: &PendingRestriction,
    ) -> OwlResult<()> {
        let families = usize::from(restriction.complement_of.is_some())
            + usize::from(restriction.union_of.is_some())
            + usize::from(restriction.intersection_of.is_some())
            + usize::from(restriction.one_of.is_some())
            + restriction.has_self.len().min(1)
            + restriction.cardinalities.len().min(1)
            + usize::from(restriction.some_values_from.is_some())
            + usize::from(restriction.all_values_from.is_some())
            + usize::from(restriction.has_value.is_some());
        if families > 1 {
            return Err(Self::parse_error(format!(
                "Ambiguous RDF/XML class expression at blank node {node_id}: mixed restriction predicates"
            )));
        }
        Ok(())
    }

    #[cfg(feature = "rio-xml")]
    fn resolve_property_restriction_expression(
        &self,
        ontology: &Ontology,
        node_id: &str,
        property: IRI,
        restriction: &PendingRestriction,
        visiting: &mut HashSet<String>,
        budget: &mut ClassExpressionResolutionBudget,
        depth: usize,
    ) -> OwlResult<Option<ClassExpression>> {
        if restriction.has_self.len() > 1 {
            return Err(Self::parse_error(format!(
                "Malformed owl:hasSelf restriction at blank node {node_id}: duplicate owl:hasSelf predicates"
            )));
        }
        if restriction.on_class.len() > 1 || restriction.on_data_range.len() > 1 {
            return Err(Self::parse_error(format!(
                "Malformed qualified restriction at blank node {node_id}: duplicate qualifier predicates"
            )));
        }

        if let Some(value) = restriction.has_self.first() {
            if Self::single_cardinality(node_id, restriction)?.is_some()
                || !restriction.on_class.is_empty()
                || !restriction.on_data_range.is_empty()
            {
                return Err(Self::parse_error(format!(
                    "Malformed owl:hasSelf restriction at blank node {node_id}: mixed restriction predicates"
                )));
            }
            Self::parse_has_self(node_id, value)?;
            if Self::declared_data_property(ontology, &property) == Some(true) {
                return Err(Self::parse_error(format!(
                    "Malformed owl:hasSelf restriction at blank node {node_id}: data property is not valid"
                )));
            }
            return Ok(Some(ClassExpression::ObjectHasSelf(Box::new(
                Self::object_property_expression(property),
            ))));
        }

        if let Some(expression) = self.resolve_cardinality_expression(
            ontology,
            node_id,
            property.clone(),
            restriction,
            visiting,
            budget,
            depth,
        )? {
            return Ok(Some(expression));
        }

        if let Some(value) = &restriction.has_value {
            match value {
                ProcessedObject::Iri(individual) => {
                    if Self::declared_data_property(ontology, &property) == Some(true) {
                        return Err(Self::parse_error(format!(
                            "Malformed owl:hasValue restriction at blank node {node_id}: data property cannot use an individual value"
                        )));
                    }
                    let property = Self::object_property_expression(property);
                    return Ok(Some(ClassExpression::ObjectHasValue(
                        Box::new(property),
                        Individual::Named(<NamedIndividual as Entity>::from_shared_iri(
                            Arc::new(individual.clone()),
                        )),
                    )));
                }
                ProcessedObject::Literal(literal) => {
                    if Self::declared_data_property(ontology, &property) == Some(false) {
                        return Err(Self::parse_error(format!(
                            "Malformed owl:hasValue restriction at blank node {node_id}: object property cannot use a literal value"
                        )));
                    }
                    let property = Self::data_property_expression(property);
                    return Ok(Some(ClassExpression::DataHasValue(
                        Box::new(property),
                        literal.clone(),
                    )));
                }
                ProcessedObject::BlankNode(_) => {
                    return Err(Self::parse_error(format!(
                        "Malformed owl:hasValue restriction at blank node {node_id}: anonymous value"
                    )));
                }
            }
        }

        let (value, existential) = if let Some(value) = &restriction.some_values_from {
            (value, true)
        } else if let Some(value) = &restriction.all_values_from {
            (value, false)
        } else {
            return Ok(None);
        };

        if Self::restriction_uses_data_property(ontology, &property, value) {
            let range = self.resolve_data_range(value)?.ok_or_else(|| {
                Self::parse_error(format!(
                    "Malformed data range in restriction blank node {node_id}"
                ))
            })?;
            let property = Self::data_property_expression(property);
            return Ok(Some(if existential {
                ClassExpression::DataSomeValuesFrom(Box::new(property), Box::new(range))
            } else {
                ClassExpression::DataAllValuesFrom(Box::new(property), Box::new(range))
            }));
        }

        let filler = self
            .resolve_class_expression_term(ontology, value, visiting, budget, depth + 1)?
            .ok_or_else(|| {
                Self::parse_error(format!(
                    "Malformed class filler in restriction blank node {node_id}"
                ))
            })?;
        let property = Self::object_property_expression(property);
        Ok(Some(if existential {
            ClassExpression::ObjectSomeValuesFrom(Box::new(property), Box::new(filler))
        } else {
            ClassExpression::ObjectAllValuesFrom(Box::new(property), Box::new(filler))
        }))
    }

    #[cfg(feature = "rio-xml")]
    fn resolve_cardinality_expression(
        &self,
        ontology: &Ontology,
        node_id: &str,
        property: IRI,
        restriction: &PendingRestriction,
        visiting: &mut HashSet<String>,
        budget: &mut ClassExpressionResolutionBudget,
        depth: usize,
    ) -> OwlResult<Option<ClassExpression>> {
        let Some(cardinality) = Self::single_cardinality(node_id, restriction)? else {
            return Ok(None);
        };
        let count = Self::parse_non_negative_cardinality(node_id, &cardinality.value)?;
        let has_on_class = !restriction.on_class.is_empty();
        let has_on_data_range = !restriction.on_data_range.is_empty();
        if has_on_class && has_on_data_range {
            return Err(Self::parse_error(format!(
                "Malformed qualified cardinality at blank node {node_id}: both owl:onClass and owl:onDataRange are present"
            )));
        }
        if cardinality.qualified && !has_on_class && !has_on_data_range {
            return Err(Self::parse_error(format!(
                "Malformed qualified cardinality at blank node {node_id}: missing qualifier"
            )));
        }
        if !cardinality.qualified && (has_on_class || has_on_data_range) {
            return Err(Self::parse_error(format!(
                "Malformed unqualified cardinality at blank node {node_id}: unexpected qualifier"
            )));
        }

        let declared_kind = Self::declared_data_property(ontology, &property);
        if has_on_data_range {
            if declared_kind == Some(false) {
                return Err(Self::parse_error(format!(
                    "Malformed qualified cardinality at blank node {node_id}: object property cannot use owl:onDataRange"
                )));
            }
            let qualifier = restriction
                .on_data_range
                .first()
                .map(|value| self.resolve_data_range(value))
                .transpose()?
                .flatten()
                .ok_or_else(|| {
                    Self::parse_error(format!(
                        "Malformed owl:onDataRange qualifier at blank node {node_id}"
                    ))
                })?;
            let property = Self::data_property_expression(property);
            return Ok(Some(match cardinality.kind {
                PendingCardinalityKind::Exact => ClassExpression::DataExactCardinality(
                    count,
                    Box::new(property),
                    Some(Box::new(qualifier)),
                ),
                PendingCardinalityKind::Min => ClassExpression::DataMinCardinality(
                    count,
                    Box::new(property),
                    Some(Box::new(qualifier)),
                ),
                PendingCardinalityKind::Max => ClassExpression::DataMaxCardinality(
                    count,
                    Box::new(property),
                    Some(Box::new(qualifier)),
                ),
            }));
        }

        if has_on_class {
            if declared_kind == Some(true) {
                return Err(Self::parse_error(format!(
                    "Malformed qualified cardinality at blank node {node_id}: data property cannot use owl:onClass"
                )));
            }
            let qualifier = restriction
                .on_class
                .first()
                .map(|value| {
                    self.resolve_class_expression_term(ontology, value, visiting, budget, depth + 1)
                })
                .transpose()?
                .flatten()
                .ok_or_else(|| {
                    Self::parse_error(format!(
                        "Malformed owl:onClass qualifier at blank node {node_id}"
                    ))
                })?;
            let property = Self::object_property_expression(property);
            return Ok(Some(match cardinality.kind {
                PendingCardinalityKind::Exact => ClassExpression::ObjectExactCardinality(
                    count,
                    Box::new(property),
                    Some(Box::new(qualifier)),
                ),
                PendingCardinalityKind::Min => ClassExpression::ObjectMinCardinality(
                    count,
                    Box::new(property),
                    Some(Box::new(qualifier)),
                ),
                PendingCardinalityKind::Max => ClassExpression::ObjectMaxCardinality(
                    count,
                    Box::new(property),
                    Some(Box::new(qualifier)),
                ),
            }));
        }

        let is_data = declared_kind.unwrap_or(false);
        if is_data {
            let property = Self::data_property_expression(property);
            return Ok(Some(match cardinality.kind {
                PendingCardinalityKind::Exact => {
                    ClassExpression::DataExactCardinality(count, Box::new(property), None)
                }
                PendingCardinalityKind::Min => {
                    ClassExpression::DataMinCardinality(count, Box::new(property), None)
                }
                PendingCardinalityKind::Max => {
                    ClassExpression::DataMaxCardinality(count, Box::new(property), None)
                }
            }));
        }

        let property = Self::object_property_expression(property);
        Ok(Some(match cardinality.kind {
            PendingCardinalityKind::Exact => {
                ClassExpression::ObjectExactCardinality(count, Box::new(property), None)
            }
            PendingCardinalityKind::Min => {
                ClassExpression::ObjectMinCardinality(count, Box::new(property), None)
            }
            PendingCardinalityKind::Max => {
                ClassExpression::ObjectMaxCardinality(count, Box::new(property), None)
            }
        }))
    }

    #[cfg(feature = "rio-xml")]
    fn resolve_class_expression_term(
        &self,
        ontology: &Ontology,
        value: &ProcessedObject,
        visiting: &mut HashSet<String>,
        budget: &mut ClassExpressionResolutionBudget,
        depth: usize,
    ) -> OwlResult<Option<ClassExpression>> {
        budget.enter(depth)?;
        match value {
            ProcessedObject::Iri(iri) => Ok(Some(ClassExpression::Class(
                <Class as Entity>::from_shared_iri(Arc::new(iri.clone())),
            ))),
            ProcessedObject::BlankNode(node_id) => self.resolve_restriction_expression(
                ontology,
                &Self::restriction_node_key(node_id),
                visiting,
                budget,
                depth + 1,
            ),
            ProcessedObject::Literal(_) => Ok(None),
        }
    }

    #[cfg(feature = "rio-xml")]
    fn resolve_restriction_expression(
        &self,
        ontology: &Ontology,
        node_id: &str,
        visiting: &mut HashSet<String>,
        budget: &mut ClassExpressionResolutionBudget,
        depth: usize,
    ) -> OwlResult<Option<ClassExpression>> {
        budget.enter(depth)?;
        if !visiting.insert(node_id.to_string()) {
            return Err(Self::parse_error(format!(
                "Cyclic OWL class expression at blank node {node_id}"
            )));
        }

        let result = (|| {
            let Some(restriction) = self.pending_restrictions.get(node_id) else {
                return Ok(None);
            };
            self.ensure_no_pending_expression_conflict(node_id)?;
            Self::validate_single_restriction_family(node_id, restriction)?;
            if let Some(value) = &restriction.complement_of {
                let expression = self
                    .resolve_class_expression_term(ontology, value, visiting, budget, depth + 1)?
                    .ok_or_else(|| {
                        Self::parse_error(format!(
                            "Malformed owl:complementOf at blank node {node_id}"
                        ))
                    })?;
                return Ok(Some(ClassExpression::ObjectComplementOf(Box::new(
                    expression,
                ))));
            }
            if let Some(head) = &restriction.union_of {
                let operands = self.resolve_class_expression_list(
                    ontology,
                    head,
                    visiting,
                    budget,
                    depth + 1,
                )?;
                return Ok(Some(ClassExpression::ObjectUnionOf(operands)));
            }
            if let Some(head) = &restriction.intersection_of {
                let operands = self.resolve_class_expression_list(
                    ontology,
                    head,
                    visiting,
                    budget,
                    depth + 1,
                )?;
                return Ok(Some(ClassExpression::ObjectIntersectionOf(operands)));
            }
            if let Some(head) = &restriction.one_of {
                return self
                    .resolve_object_enumeration(head, budget, depth + 1)
                    .map(Some);
            }
            let Some(property) = restriction.on_property.as_ref().cloned() else {
                return Ok(None);
            };

            self.resolve_property_restriction_expression(
                ontology,
                node_id,
                property,
                restriction,
                visiting,
                budget,
                depth,
            )
        })();
        visiting.remove(node_id);
        result
    }

    #[cfg(feature = "rio-xml")]
    fn resolve_object_enumeration(
        &self,
        head: &ProcessedObject,
        budget: &mut ClassExpressionResolutionBudget,
        depth: usize,
    ) -> OwlResult<ClassExpression> {
        let values = self.resolve_rdf_list(head)?;
        if values.is_empty() {
            return Err(Self::parse_error(
                "Unsupported empty object enumeration in bounded owl:oneOf parser",
            ));
        }
        // Keep the large inline Individual storage outside the recursive
        // expression dispatcher's stack frame.
        let mut individuals = Box::new(SmallVec::new());
        for value in values {
            budget.enter(depth)?;
            match value {
                ProcessedObject::Iri(iri) => individuals.push(Individual::Named(
                    <NamedIndividual as Entity>::from_shared_iri(Arc::new(iri)),
                )),
                ProcessedObject::BlankNode(_) => {
                    return Err(Self::parse_error(
                        "Unsupported anonymous individual in owl:oneOf object enumeration",
                    ));
                }
                ProcessedObject::Literal(_) => {
                    return Err(Self::parse_error(
                        "Malformed owl:oneOf object enumeration: literal member",
                    ));
                }
            }
        }
        Ok(ClassExpression::ObjectOneOf(individuals))
    }

    #[cfg(feature = "rio-xml")]
    fn resolve_class_expression_list(
        &self,
        ontology: &Ontology,
        head: &ProcessedObject,
        visiting: &mut HashSet<String>,
        budget: &mut ClassExpressionResolutionBudget,
        depth: usize,
    ) -> OwlResult<SmallVec<[Box<ClassExpression>; 4]>> {
        budget.enter(depth)?;
        let values = self.resolve_rdf_list(head)?;
        values
            .into_iter()
            .map(|value| {
                self.resolve_class_expression_term(ontology, &value, visiting, budget, depth + 1)?
                    .map(Box::new)
                    .ok_or_else(|| {
                        Self::parse_error(
                            "Malformed OWL boolean expression list: non-class operand",
                        )
                    })
            })
            .collect()
    }

    #[cfg(feature = "rio-xml")]
    fn materialize_pending_class_assertions(&mut self, ontology: &mut Ontology) -> OwlResult<()> {
        let pending = std::mem::take(&mut self.pending_class_assertions);
        for assertion in pending {
            let mut visiting = HashSet::new();
            let mut budget = ClassExpressionResolutionBudget::new();
            let Some(class_expr) = self.resolve_restriction_expression(
                ontology,
                &assertion.restriction_node,
                &mut visiting,
                &mut budget,
                0,
            )?
            else {
                return Err(Self::parse_error(format!(
                    "Unsupported RDF/XML class assertion expression at blank node {}",
                    assertion.restriction_node,
                )));
            };

            if !ontology.contains_named_individual_iri(&assertion.individual) {
                ontology.add_named_individual(<NamedIndividual as Entity>::from_shared_iri(
                    Arc::new(assertion.individual.clone()),
                ))?;
            }
            ontology.add_class_assertion(ClassAssertionAxiom::new(
                Arc::new(assertion.individual),
                class_expr,
            ))?;
        }
        Ok(())
    }

    #[cfg(feature = "rio-xml")]
    fn materialize_pending_class_edges(&mut self, ontology: &mut Ontology) -> OwlResult<()> {
        let pending = std::mem::take(&mut self.pending_class_edges);
        for edge in pending {
            if edge.equivalent {
                if let (Some(left), Some(right)) = (edge.left.as_iri(), edge.right.as_iri()) {
                    ontology.add_equivalent_classes_axiom(EquivalentClassesAxiom::new(vec![
                        Arc::new(left.clone()),
                        Arc::new(right.clone()),
                    ]))?;
                    continue;
                }
            }

            let mut visiting = HashSet::new();
            let mut budget = ClassExpressionResolutionBudget::new();
            let Some(left) = self.resolve_class_expression_term(
                ontology,
                &edge.left,
                &mut visiting,
                &mut budget,
                0,
            )?
            else {
                return Err(Self::parse_error(format!(
                    "Unsupported RDF/XML class expression in deferred class edge: left={} right={}",
                    Self::describe_processed_object(&edge.left),
                    Self::describe_processed_object(&edge.right)
                )));
            };
            let Some(right) = self.resolve_class_expression_term(
                ontology,
                &edge.right,
                &mut visiting,
                &mut budget,
                0,
            )?
            else {
                return Err(Self::parse_error(format!(
                    "Unsupported RDF/XML class expression in deferred class edge: left={} right={}",
                    Self::describe_processed_object(&edge.left),
                    Self::describe_processed_object(&edge.right)
                )));
            };
            ontology.add_subclass_axiom(SubClassOfAxiom::new(left.clone(), right.clone()))?;
            if edge.equivalent {
                ontology.add_subclass_axiom(SubClassOfAxiom::new(right, left))?;
            }
        }
        Ok(())
    }

    /// Whether a range object's oneOf enumeration is literal-valued —
    /// the data-range shape — for properties with no declaration to
    /// decide by (#150).
    #[cfg(feature = "rio-xml")]
    fn range_object_is_data_shaped(&self, object: &ProcessedObject) -> bool {
        let ProcessedObject::BlankNode(node_id) = object else {
            return false;
        };
        let Some(restriction) = self
            .pending_restrictions
            .get(&Self::restriction_node_key(node_id))
        else {
            return false;
        };
        let Some(head) = &restriction.one_of else {
            return false;
        };
        match self.resolve_rdf_list(head) {
            Ok(values) => {
                !values.is_empty()
                    && values
                        .iter()
                        .all(|value| matches!(value, ProcessedObject::Literal(_)))
            }
            Err(_) => false,
        }
    }

    /// Whether a range object's oneOf enumeration is individual-valued
    /// — the object-enumeration shape — for undeclared properties
    /// (#150).
    #[cfg(feature = "rio-xml")]
    fn range_object_is_object_shaped(&self, object: &ProcessedObject) -> bool {
        let ProcessedObject::BlankNode(node_id) = object else {
            return false;
        };
        let Some(restriction) = self
            .pending_restrictions
            .get(&Self::restriction_node_key(node_id))
        else {
            return false;
        };
        let Some(head) = &restriction.one_of else {
            return false;
        };
        match self.resolve_rdf_list(head) {
            Ok(values) => {
                !values.is_empty()
                    && values
                        .iter()
                        .all(|value| matches!(value, ProcessedObject::Iri(_)))
            }
            Err(_) => false,
        }
    }

    /// Resolve deferred rdfs:range edges whose object is a class or
    /// data expression. Data-flavored objects become enumerated-range
    /// pools (or plain datatype ranges — a facet-restricted datatype
    /// lowers to its base datatype, under-approximating the facets as
    /// recorded incompleteness); class expressions lower to the
    /// inverse-existential GCI `exists p- . T subclassof C`, the range
    /// reading that fires on spawned fillers downstream (#150).
    /// Malformed content surfaces as parse errors like every other
    /// expression path; shapes the resolvers do not cover keep
    /// today's drop.
    #[cfg(feature = "rio-xml")]
    fn materialize_pending_range_edges(&mut self, ontology: &mut Ontology) -> OwlResult<()> {
        let thing_iri = IRI::new(crate::parser::rdf_xml_common::NS_OWL_THING)
            .map_err(|_| Self::parse_error("invalid owl:Thing IRI"))?;
        let pending = std::mem::take(&mut self.pending_range_edges);
        for (property, object) in pending {
            let is_data = match Self::declared_data_property(ontology, &property) {
                Some(is_data) => is_data,
                // Undeclared properties decide by enumeration shape;
                // any other shape keeps today's drop rather than
                // lowering datatypes as classes.
                None => {
                    if self.range_object_is_data_shaped(&object) {
                        true
                    } else if self.range_object_is_object_shaped(&object) {
                        false
                    } else {
                        continue;
                    }
                }
            };
            if is_data {
                match self.resolve_data_range(&object)? {
                    Some(DataRange::DataOneOf(values)) => {
                        ontology.add_axiom(Axiom::DataPropertyEnumeratedRange(Box::new(
                            DataPropertyEnumeratedRangeAxiom::new(Arc::new(property), values),
                        )))?;
                    }
                    Some(DataRange::Datatype(datatype)) => {
                        ontology.add_axiom(Axiom::DataPropertyRange(Box::new(
                            DataPropertyRangeAxiom::new(Arc::new(property), Arc::new(datatype)),
                        )))?;
                    }
                    Some(DataRange::DatatypeRestriction(datatype, _facets)) => {
                        ontology.add_axiom(Axiom::DataPropertyRange(Box::new(
                            DataPropertyRangeAxiom::new(Arc::new(property), Arc::new(datatype)),
                        )))?;
                    }
                    // Compound data ranges have no axiom form yet;
                    // recorded incompleteness, dropped as today.
                    Some(
                        DataRange::DataIntersectionOf(_)
                        | DataRange::DataUnionOf(_)
                        | DataRange::DataComplementOf(_),
                    ) => {}
                    None => {}
                }
                continue;
            }
            let mut visiting = HashSet::new();
            let mut budget = ClassExpressionResolutionBudget::new();
            let Some(expression) = self.resolve_class_expression_term(
                ontology,
                &object,
                &mut visiting,
                &mut budget,
                0,
            )?
            else {
                continue;
            };
            let inverse = ObjectPropertyExpression::ObjectInverseOf(Box::new(
                ObjectPropertyExpression::ObjectProperty(Box::new(ObjectProperty::new(property))),
            ));
            let thing = ClassExpression::Class(<Class as Entity>::from_shared_iri(Arc::new(
                thing_iri.clone(),
            )));
            ontology.add_subclass_axiom(SubClassOfAxiom::new(
                ClassExpression::ObjectSomeValuesFrom(Box::new(inverse), Box::new(thing)),
                expression,
            ))?;
        }
        Ok(())
    }

    #[cfg(feature = "rio-xml")]
    fn materialize_pending_negative_assertions(
        &mut self,
        ontology: &mut Ontology,
    ) -> OwlResult<()> {
        let pending = std::mem::take(&mut self.pending_negative_assertions);
        for assertion in pending.into_values() {
            let (Some(source), Some(property), Some(target)) = (
                assertion.source_individual,
                assertion.assertion_property,
                assertion.target,
            ) else {
                continue;
            };

            match target {
                ProcessedObject::Iri(object) => {
                    ontology.add_axiom(Axiom::NegativeObjectPropertyAssertion(Box::new(
                        NegativeObjectPropertyAssertionAxiom::new(
                            Arc::new(source),
                            Arc::new(property),
                            Arc::new(object),
                        ),
                    )))?;
                }
                ProcessedObject::Literal(value) => {
                    ontology.add_axiom(Axiom::NegativeDataPropertyAssertion(Box::new(
                        NegativeDataPropertyAssertionAxiom::new(
                            Arc::new(source),
                            Arc::new(property),
                            value,
                        ),
                    )))?;
                }
                ProcessedObject::BlankNode(_) => {}
            }
        }
        Ok(())
    }

    /// How the ontology declares one property: Some(true) data,
    /// Some(false) object, None undeclared — the shared classifier for
    /// pairwise and members-collection property disjointness.
    #[cfg(feature = "rio-xml")]
    fn declared_property_kind(ontology: &Ontology, property: &IRI) -> Option<bool> {
        if ontology
            .data_properties()
            .iter()
            .any(|declared| declared.iri().as_ref() == property)
        {
            Some(true)
        } else if ontology
            .object_properties()
            .iter()
            .any(|declared| declared.iri().as_ref() == property)
        {
            Some(false)
        } else {
            None
        }
    }

    #[cfg(feature = "rio-xml")]
    fn materialize_pending_disjoint_property_pairs(
        &mut self,
        ontology: &mut Ontology,
    ) -> OwlResult<()> {
        let pending = std::mem::take(&mut self.pending_disjoint_property_pairs);
        for (left, right) in pending {
            let left_kind = Self::declared_property_kind(ontology, &left);
            let right_kind = Self::declared_property_kind(ontology, &right);

            if left_kind == Some(true) && right_kind == Some(true) {
                ontology.add_axiom(Axiom::DisjointDataProperties(Box::new(
                    DisjointDataPropertiesAxiom::new(vec![Arc::new(left), Arc::new(right)]),
                )))?;
            } else if left_kind == Some(false) && right_kind == Some(false) {
                ontology.add_axiom(Axiom::DisjointObjectProperties(Box::new(
                    DisjointObjectPropertiesAxiom::new(vec![Arc::new(left), Arc::new(right)]),
                )))?;
            }
        }
        Ok(())
    }

    #[cfg(feature = "rio-xml")]
    fn materialize_pending_different_individual_pairs(
        &mut self,
        ontology: &mut Ontology,
    ) -> OwlResult<()> {
        let pending = std::mem::take(&mut self.pending_different_individual_pairs);
        for (left, right) in pending {
            ontology.add_axiom(Axiom::DifferentIndividuals(Box::new(
                DifferentIndividualsAxiom::new(vec![Arc::new(left), Arc::new(right)]),
            )))?;
        }
        Ok(())
    }

    /// owl:AllDisjointClasses / owl:AllDisjointProperties /
    /// owl:AllDifferent marker nodes lower here: the typed kind joins its
    /// members list head, and the resolved members become one axiom — a
    /// DisjointClasses axiom over the members, a DisjointDataProperties
    /// or DisjointObjectProperties axiom classified by the members'
    /// declarations, or one DifferentIndividuals axiom (#130). Fewer
    /// than two members, non-IRI members, mixed or undeclared property
    /// kinds, and unresolvable lists keep the previous
    /// silently-ignored behavior (#109's narrow-gate lesson).
    #[cfg(feature = "rio-xml")]
    fn materialize_pending_all_disjoint(&mut self, ontology: &mut Ontology) -> OwlResult<()> {
        let kinds = std::mem::take(&mut self.pending_all_disjoint_kinds);
        let heads = std::mem::take(&mut self.pending_all_disjoint_heads);
        for (key, kind) in kinds {
            let Some(head) = heads.get(&key) else {
                continue;
            };
            let Ok(values) = self.resolve_rdf_list(head) else {
                continue;
            };
            let members: Option<Vec<_>> = values
                .iter()
                .map(|value| match value {
                    ProcessedObject::Iri(iri) => Some(Arc::new(iri.clone())),
                    _ => None,
                })
                .collect();
            let Some(members) = members else {
                continue;
            };
            if members.len() < 2 {
                continue;
            }
            match kind {
                AllDisjointKind::Classes => {
                    ontology.add_disjoint_classes_axiom(DisjointClassesAxiom::new(members))?;
                }
                AllDisjointKind::Properties => {
                    let kinds: Option<Vec<bool>> =
                        members.iter().map(|property| {
                            Self::declared_property_kind(ontology, property)
                        }).collect();
                    let Some(kinds) = kinds else {
                        continue;
                    };
                    if kinds.iter().all(|is_data| *is_data) {
                        ontology.add_axiom(Axiom::DisjointDataProperties(Box::new(
                            DisjointDataPropertiesAxiom::new(members),
                        )))?;
                    } else if kinds.iter().all(|is_data| !*is_data) {
                        ontology.add_axiom(Axiom::DisjointObjectProperties(Box::new(
                            DisjointObjectPropertiesAxiom::new(members),
                        )))?;
                    }
                }
                AllDisjointKind::Individuals => {
                    ontology.add_axiom(Axiom::DifferentIndividuals(Box::new(
                        DifferentIndividualsAxiom::new(members),
                    )))?;
                }
            }
        }
        Ok(())
    }

    #[cfg(feature = "rio-xml")]
    fn materialize_pending_has_keys(&mut self, ontology: &mut Ontology) -> OwlResult<()> {
        let pending = std::mem::take(&mut self.pending_has_key_pairs);
        for (class, head) in pending {
            let properties = self
                .resolve_rdf_list(&head)?
                .into_iter()
                .map(|value| match value {
                    ProcessedObject::Iri(property) => Some(Arc::new(property)),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| Self::parse_error("Malformed owl:hasKey list: non-IRI item"))?;
            ontology.add_axiom(Axiom::HasKey(Box::new(HasKeyAxiom::new(
                Arc::new(class),
                properties,
            ))))?;
        }
        Ok(())
    }

    /// Description-form class definitions: a named class carrying a bare
    /// owl:intersectionOf triple (the oiled/W3C description-logic
    /// serialization style) defines the class, so lower it to the same
    /// bidirectional SubClassOf pair the equivalentClass path produces.
    /// Only intersections: lowering bare oneOf/unionOf descriptions drags
    /// premises into fragments the evaluation pipeline answers
    /// "unsupported" for, regressing cases that pass today. Entries that
    /// fail to resolve keep the previous silently-ignored behavior.
    #[cfg(feature = "rio-xml")]
    fn materialize_pending_named_class_descriptions(
        &mut self,
        ontology: &mut Ontology,
    ) -> OwlResult<()> {
        // oneOf enumerations flush first: their subject lookup reads
        // pending_named_class_descriptions before the intersection
        // path takes it.
        let enumerations = std::mem::take(&mut self.pending_named_class_enumerations);
        for (key, heads) in enumerations {
            if heads.len() < 2 {
                // Single-list frames keep today's axiom-stream
                // behavior (nothing lowers); the flush targets the
                // multiple-enumeration lattice idiom (W3C
                // description-logic-501/502) whose N-equalities
                // reading no other path represents. The frame itself
                // lands in the ontology's side channel, resolved and
                // role-agnostic, so the entailment lane can lower it
                // premise-side where the parser cannot tell premises
                // from conclusions (#141, #155).
                let Some(subject) = self.pending_named_class_descriptions.get(&key) else {
                    continue;
                };
                for head in heads {
                    let mut budget = ClassExpressionResolutionBudget::new();
                    let Ok(ClassExpression::ObjectOneOf(individuals)) =
                        self.resolve_object_enumeration(&head, &mut budget, 0)
                    else {
                        // Entries that fail to resolve keep the
                        // previous silently-ignored behavior (#109).
                        continue;
                    };
                    let members: Option<Vec<_>> = individuals
                        .iter()
                        .map(|individual| match individual {
                            Individual::Named(named) => Some(named.iri().clone()),
                            _ => None,
                        })
                        .collect();
                    let Some(members) = members else {
                        continue;
                    };
                    ontology
                        .record_object_enumeration_frame(Arc::new(subject.clone()), members);
                }
                continue;
            }
            let Some(subject) = self.pending_named_class_descriptions.get(&key) else {
                continue;
            };
            for head in heads {
                let mut budget = ClassExpressionResolutionBudget::new();
                let Ok(expression) = self.resolve_object_enumeration(&head, &mut budget, 0)
                else {
                    // Entries that fail to resolve keep the previous
                    // silently-ignored behavior (#109).
                    continue;
                };
                let class = <Class as Entity>::from_shared_iri(Arc::new(subject.clone()));
                let named = ClassExpression::Class(class);
                ontology.add_subclass_axiom(SubClassOfAxiom::new(
                    named.clone(),
                    expression.clone(),
                ))?;
                ontology.add_subclass_axiom(SubClassOfAxiom::new(expression, named))?;
            }
        }
        // disjointUnionOf frames lower next (before the descriptions
        // take below consumes the subject map): a named class equals
        // the union of its named members, pairwise disjoint (#120,
        // W3C New-Feature-DisjointUnion-001). Frames with fewer than
        // two members or any unnamed member keep the previous
        // silently-ignored behavior (#109's narrow-gate lesson).
        let disjoint_unions = std::mem::take(&mut self.pending_named_class_disjoint_unions);
        for (key, heads) in disjoint_unions {
            let Some(subject) = self.pending_named_class_descriptions.get(&key) else {
                continue;
            };
            for head in heads {
            let mut visiting = HashSet::new();
            let mut budget = ClassExpressionResolutionBudget::new();
            let Ok(operands) =
                self.resolve_class_expression_list(ontology, &head, &mut visiting, &mut budget, 0)
            else {
                continue;
            };
            if operands.len() < 2 {
                continue;
            }
            let members: Option<Vec<_>> = operands
                .iter()
                .map(|operand| match operand.as_ref() {
                    ClassExpression::Class(class) => Some(class.iri().clone()),
                    _ => None,
                })
                .collect();
            let Some(members) = members else {
                continue;
            };
            let class = <Class as Entity>::from_shared_iri(Arc::new(subject.clone()));
            let named = ClassExpression::Class(class);
            let union = ClassExpression::ObjectUnionOf(operands);
            ontology.add_subclass_axiom(SubClassOfAxiom::new(named.clone(), union.clone()))?;
            ontology.add_subclass_axiom(SubClassOfAxiom::new(union, named))?;
            ontology.add_disjoint_classes_axiom(DisjointClassesAxiom::new(members))?;
            }
        }
        let pending = std::mem::take(&mut self.pending_named_class_descriptions);
        for (key, subject) in pending {
            let has_intersection = self
                .pending_restrictions
                .get(&key)
                .is_some_and(|restriction| restriction.intersection_of.is_some());
            if !has_intersection {
                continue;
            }
            let mut visiting = HashSet::new();
            let mut budget = ClassExpressionResolutionBudget::new();
            let Ok(Some(expression)) = self.resolve_restriction_expression(
                ontology,
                &key,
                &mut visiting,
                &mut budget,
                0,
            ) else {
                continue;
            };
            let class = <Class as Entity>::from_shared_iri(Arc::new(subject));
            let named = ClassExpression::Class(class);
            ontology.add_subclass_axiom(SubClassOfAxiom::new(named.clone(), expression.clone()))?;
            ontology.add_subclass_axiom(SubClassOfAxiom::new(expression, named))?;
        }
        Ok(())
    }

    #[cfg(feature = "rio-xml")]
    fn finalize_pending_functional_properties(&mut self, ontology: &mut Ontology) -> OwlResult<()> {
        self.materialize_pending_class_edges(ontology)?;
        self.materialize_pending_class_assertions(ontology)?;
        self.materialize_pending_negative_assertions(ontology)?;
        self.materialize_pending_disjoint_property_pairs(ontology)?;
        self.materialize_pending_different_individual_pairs(ontology)?;
        self.materialize_pending_all_disjoint(ontology)?;
        self.materialize_pending_property_chains(ontology)?;
        self.materialize_pending_equivalent_property_pairs(ontology)?;
        self.materialize_pending_same_individual_pairs(ontology)?;
        self.materialize_pending_has_keys(ontology)?;
        self.materialize_pending_named_class_descriptions(ontology)?;
        self.materialize_pending_range_edges(ontology)?;
        self.pending_restrictions.clear();
        self.pending_rdf_lists.clear();
        self.pending_facet_restrictions.clear();
        self.pending_expression_conflicts.clear();
        let pending = std::mem::take(&mut self.pending_functional_properties);
        for subject in pending {
            self.add_functional_property_axiom(ontology, &subject, false)?;
        }
        Ok(())
    }

    #[cfg(feature = "rio-xml")]
    fn add_functional_property_axiom(
        &mut self,
        ontology: &mut Ontology,
        subject: &IRI,
        is_data_property: bool,
    ) -> OwlResult<()> {
        if is_data_property {
            let axiom = FunctionalDataPropertyAxiom::new(Arc::new(subject.clone()));
            ontology.add_axiom(Axiom::FunctionalDataProperty(Box::new(axiom)))?;
        } else {
            self.handle_object_property_characteristic(
                ontology,
                subject,
                ObjectPropertyCharacteristicKind::Functional,
            )?;
        }
        Ok(())
    }

    /// Handle an object property characteristic represented as an `rdf:type` triple.
    #[cfg(feature = "rio-xml")]
    fn handle_object_property_characteristic(
        &mut self,
        ontology: &mut Ontology,
        subject: &IRI,
        characteristic: ObjectPropertyCharacteristicKind,
    ) -> OwlResult<()> {
        let property = <ObjectProperty as Entity>::from_shared_iri(Arc::new(subject.clone()));
        ontology.add_object_property(property)?;

        let property = Arc::new(subject.clone());
        let axiom = match characteristic {
            ObjectPropertyCharacteristicKind::Functional => {
                Axiom::FunctionalProperty(Box::new(FunctionalPropertyAxiom::new(property)))
            }
            ObjectPropertyCharacteristicKind::InverseFunctional => {
                Axiom::InverseFunctionalProperty(Box::new(InverseFunctionalPropertyAxiom::new(
                    property,
                )))
            }
            ObjectPropertyCharacteristicKind::Reflexive => {
                Axiom::ReflexiveProperty(Box::new(ReflexivePropertyAxiom::new(property)))
            }
            ObjectPropertyCharacteristicKind::Irreflexive => {
                Axiom::IrreflexiveProperty(Box::new(IrreflexivePropertyAxiom::new(property)))
            }
            ObjectPropertyCharacteristicKind::Symmetric => {
                Axiom::SymmetricProperty(Box::new(SymmetricPropertyAxiom::new(property)))
            }
            ObjectPropertyCharacteristicKind::Asymmetric => {
                Axiom::AsymmetricProperty(Box::new(AsymmetricPropertyAxiom::new(property)))
            }
            ObjectPropertyCharacteristicKind::Transitive => {
                Axiom::TransitiveProperty(Box::new(TransitivePropertyAxiom::new(property)))
            }
        };
        ontology.add_axiom(axiom)?;
        Ok(())
    }

    /// Handle subclass relationships
    #[cfg(feature = "rio-xml")]
    fn handle_subclass_of(
        &mut self,
        ontology: &mut Ontology,
        subject: &Arc<IRI>,
        object: &ProcessedObject,
    ) -> OwlResult<()> {
        let Some(object_iri) = object.as_iri() else {
            self.pending_class_edges.push(PendingClassEdge {
                left: Self::class_term_from_subject(subject),
                right: object.clone(),
                equivalent: false,
            });
            return Ok(());
        };
        if subject.as_str().starts_with("_:") {
            self.pending_class_edges.push(PendingClassEdge {
                left: Self::class_term_from_subject(subject),
                right: object.clone(),
                equivalent: false,
            });
            return Ok(());
        }
        let subclass = <Class as Entity>::from_shared_iri(subject.clone());
        let superclass = <Class as Entity>::from_shared_iri(Arc::new(object_iri.clone()));
        let axiom = SubClassOfAxiom::new(
            ClassExpression::Class(subclass),
            ClassExpression::Class(superclass),
        );
        ontology.add_subclass_axiom(axiom)?;
        Ok(())
    }

    /// Handle inverse-of declarations (P inverseOf Q)
    #[cfg(feature = "rio-xml")]
    fn handle_inverse_of(
        &mut self,
        ontology: &mut Ontology,
        subject: &Arc<IRI>,
        object_iri: &IRI,
    ) -> OwlResult<()> {
        let axiom = InverseObjectPropertiesAxiom::new(
            subject.clone(),
            Arc::new(object_iri.clone()),
        );
        ontology.add_axiom(crate::logic::axioms::Axiom::InverseObjectProperties(Box::new(
            axiom,
        )))?;
        Ok(())
    }

    /// Handle sub-property declarations (P ⊑ Q)
    #[cfg(feature = "rio-xml")]
    fn handle_subproperty_of(
        &mut self,
        ontology: &mut Ontology,
        subject: &Arc<IRI>,
        object_iri: &IRI,
    ) -> OwlResult<()> {
        let axiom = SubObjectPropertyOfAxiom::new(
            subject.clone(),
            Arc::new(object_iri.clone()),
        );
        ontology.add_axiom(crate::logic::axioms::Axiom::SubObjectProperty(Box::new(axiom)))?;
        Ok(())
    }

    /// Handle domain declarations
    #[cfg(feature = "rio-xml")]
    fn handle_domain(
        &mut self,
        ontology: &mut Ontology,
        subject: &Arc<IRI>,
        object_iri: &IRI,
    ) -> OwlResult<()> {
        // This is simplified - in practice, you'd need to determine the property type
        let axiom =
            ObjectPropertyDomainAxiom::new(subject.clone(), Arc::new(object_iri.clone()));
        // Add as generic axiom for now
        ontology.add_axiom(crate::logic::axioms::Axiom::ObjectPropertyDomain(Box::new(
            axiom,
        )))?;
        Ok(())
    }

    /// Handle range declarations
    #[cfg(feature = "rio-xml")]
    fn handle_range(
        &mut self,
        ontology: &mut Ontology,
        subject: &Arc<IRI>,
        object_iri: &IRI,
    ) -> OwlResult<()> {
        // For object property range
        let axiom =
            ObjectPropertyRangeAxiom::new(subject.clone(), Arc::new(object_iri.clone()));
        // Add as generic axiom for now
        ontology.add_axiom(crate::logic::axioms::Axiom::ObjectPropertyRange(Box::new(
            axiom,
        )))?;
        Ok(())
    }

    /// Handle OWL-specific properties
    #[cfg(feature = "rio-xml")]
    fn handle_owl_property(
        &mut self,
        ontology: &mut Ontology,
        subject: &Arc<IRI>,
        predicate_iri_str: &str,
        predicate_tag: PredicateTag,
        object: &ProcessedObject,
    ) -> OwlResult<()> {
        match predicate_tag {
            PredicateTag::OwlDisjointWith => {
                if let Some(object_iri) = object.as_iri() {
                    let axiom = DisjointClassesAxiom::new(vec![
                        subject.clone(),
                        Arc::new(object_iri.clone()),
                    ]);
                    ontology.add_disjoint_classes_axiom(axiom)?;
                }
            }
            PredicateTag::OwlEquivalentClass => {
                if let Some(object_iri) = object.as_iri() {
                    if !subject.as_str().starts_with("_:") {
                        let axiom = EquivalentClassesAxiom::new(vec![
                            subject.clone(),
                            Arc::new(object_iri.clone()),
                        ]);
                        ontology.add_equivalent_classes_axiom(axiom)?;
                    } else {
                        self.pending_class_edges.push(PendingClassEdge {
                            left: Self::class_term_from_subject(subject),
                            right: object.clone(),
                            equivalent: true,
                        });
                    }
                } else {
                    self.pending_class_edges.push(PendingClassEdge {
                        left: Self::class_term_from_subject(subject),
                        right: object.clone(),
                        equivalent: true,
                    });
                }
            }
            PredicateTag::OwlOther => {
                self.record_restriction_property(subject, predicate_iri_str, object);
            }
            PredicateTag::RdfType
            | PredicateTag::RdfsSubClassOf
            | PredicateTag::RdfsSubPropertyOf
            | PredicateTag::OwlInverseOf
            | PredicateTag::RdfsDomain
            | PredicateTag::RdfsRange
            | PredicateTag::Other => {}
        }

        Ok(())
    }

    /// Handle generic property assertions
    #[cfg(feature = "rio-xml")]
    fn handle_property_assertion(
        &mut self,
        ontology: &mut Ontology,
        subject: &Arc<IRI>,
        predicate: &Arc<IRI>,
        object: &ProcessedObject,
    ) -> OwlResult<()> {
        // Property assertions imply the subject is an individual.
        if !ontology.contains_named_individual_iri(subject) {
            ontology.add_named_individual(<NamedIndividual as Entity>::from_shared_iri(
                subject.clone(),
            ))?;
        }

        match object {
            ProcessedObject::Iri(object_iri) => {
                // Object property with named individual
                if !ontology.contains_named_individual_iri(object_iri) {
                    ontology.add_named_individual(<NamedIndividual as Entity>::from_shared_iri(
                        Arc::new(object_iri.clone()),
                    ))?;
                }

                let assertion = PropertyAssertionAxiom::new(
                    subject.clone(),
                    predicate.clone(),
                    Arc::new(object_iri.clone()),
                );
                ontology.add_property_assertion(assertion)?;
            }
            ProcessedObject::BlankNode(node_id) => {
                // Object property with anonymous individual
                let anon_individual = AnonymousIndividual::new(
                    self.cached_blank_node_label(node_id).as_ref().to_string(),
                );
                ontology.add_anonymous_individual(anon_individual.clone())?;

                let assertion = PropertyAssertionAxiom::new_with_anonymous(
                    subject.clone(),
                    predicate.clone(),
                    anon_individual,
                );
                ontology.add_property_assertion(assertion)?;
            }
            ProcessedObject::Literal(literal) => {
                // Data property with literal value
                let assertion = DataPropertyAssertionAxiom::new(
                    subject.clone(),
                    predicate.clone(),
                    literal.clone(),
                );
                ontology.add_data_property_assertion(assertion)?;
            }
        }

        Ok(())
    }
}

/// Processed object representation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessedObject {
    Iri(IRI),
    BlankNode(String),
    Literal(Literal),
}

impl ProcessedObject {
    pub fn as_iri(&self) -> Option<&IRI> {
        match self {
            ProcessedObject::Iri(iri) => Some(iri),
            _ => None,
        }
    }
}

// Fallback implementations when rio-xml feature is not enabled
#[cfg(not(feature = "rio-xml"))]
impl RdfXmlStreamingParser {
    pub fn parse_content(&mut self, _content: &str) -> OwlResult<Ontology> {
        Err(crate::core::error::OwlError::ParseError(
            "Streaming parsing requires 'rio-xml' feature".to_string(),
        ))
    }

    pub fn parse_file(&self, _path: &Path) -> OwlResult<Ontology> {
        Err(crate::core::error::OwlError::ParseError(
            "Streaming parsing requires 'rio-xml' feature".to_string(),
        ))
    }

    pub fn parse_stream(&mut self, _reader: impl std::io::BufRead) -> OwlResult<Ontology> {
        Err(crate::core::error::OwlError::ParseError(
            "Streaming parsing requires 'rio-xml' feature".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "rio-xml")]
    #[test]
    fn test_rdf_xml_streaming_basic_parsing() {
        let config = ParserConfig::default();
        let mut parser = RdfXmlStreamingParser::new(config);

        // Simple RDF/XML content
        let rdf_xml_content = r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:owl="http://www.w3.org/2002/07/owl#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#">

    <owl:Class rdf:about="http://example.org/Person">
        <rdfs:label>Person</rdfs:label>
    </owl:Class>

</rdf:RDF>"#;

        let result = parser.parse_content(rdf_xml_content);
        assert!(
            result.is_ok(),
            "Failed to parse basic RDF/XML content: {:?}",
            result.err()
        );

        if let Ok(ontology) = result {
            let classes = ontology.classes();
            assert!(
                !classes.is_empty(),
                "No classes were parsed from the content"
            );
        }
    }

    #[cfg(feature = "rio-xml")]
    #[test]
    fn test_cardinality_accepts_integral_decimal_literal() {
        // W3C description-logic-601/608: the corpus types cardinality
        // literals as xsd:decimal with integral lexical forms. The digits
        // validation below already rejects non-integral lexicals, so
        // accepting the decimal datatype here is a coercion, not a
        // widening.
        let config = ParserConfig::default();
        let mut parser = RdfXmlStreamingParser::new(config);

        let rdf_xml_content = r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="http://example.org/C">
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty rdf:resource="http://example.org/p"/>
        <owl:cardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#decimal">0</owl:cardinality>
      </owl:Restriction>
    </owl:equivalentClass>
  </owl:Class>
</rdf:RDF>"#;

        let result = parser.parse_content(rdf_xml_content);
        assert!(
            result.is_ok(),
            "integral xsd:decimal cardinality must parse: {:?}",
            result.err()
        );

        let rdf_xml_content = r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="http://example.org/C">
    <owl:equivalentClass>
      <owl:Restriction>
        <owl:onProperty rdf:resource="http://example.org/p"/>
        <owl:minCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#decimal">1.5</owl:minCardinality>
      </owl:Restriction>
    </owl:equivalentClass>
  </owl:Class>
</rdf:RDF>"#;

        let result = parser.parse_content(rdf_xml_content);
        assert!(
            result.is_err(),
            "non-integral xsd:decimal cardinality must fail closed"
        );
    }

    #[cfg(feature = "rio-xml")]
    #[test]
    fn test_single_list_oneof_frame_records_to_channel() {
        // W3C WebOnt-SymmetricProperty-002 (0323) premise shape: the
        // named class A nested as an inverse-functional property's
        // range carries ONE owl:oneOf list. #109's narrow gate keeps
        // the frame out of the axiom stream (no SubClassOf pair), but
        // the frame's class and resolved members land in the side
        // channel for kernel-side consumption where premise and
        // conclusion roles are visible (#141, #155).
        let config = ParserConfig::default();
        let mut parser = RdfXmlStreamingParser::new(config);

        let rdf_xml_content = r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:owl="http://www.w3.org/2002/07/owl#"
    xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#">
  <owl:InverseFunctionalProperty rdf:about="http://example.org/p">
    <rdfs:range>
      <owl:Class rdf:about="http://example.org/A">
        <owl:oneOf rdf:parseType="Collection">
          <owl:Thing rdf:about="http://example.org/a"/>
          <owl:Thing rdf:about="http://example.org/b"/>
        </owl:oneOf>
      </owl:Class>
    </rdfs:range>
  </owl:InverseFunctionalProperty>
</rdf:RDF>"#;

        let ontology = parser
            .parse_content(rdf_xml_content)
            .expect("single-list oneOf frame must parse");
        let frames = ontology.object_enumeration_frames();
        assert_eq!(frames.len(), 1, "exactly one frame must be recorded");
        assert_eq!(frames[0].class.as_str(), "http://example.org/A");
        let members: Vec<&str> = frames[0]
            .members
            .iter()
            .map(|member| member.as_str())
            .collect();
        assert_eq!(members, vec!["http://example.org/a", "http://example.org/b"]);
        let enumerated = IRI::new_optimized("http://example.org/A")
            .expect("test IRI is valid");
        let lowered = ontology
            .subclass_axioms()
            .iter()
            .filter(|axiom| axiom.involves_class(&enumerated))
            .count();
        assert_eq!(
            lowered, 0,
            "the single-list frame must NOT lower to axioms (#109 gate)"
        );
    }

    #[cfg(feature = "rio-xml")]
    #[test]
    fn test_deferred_class_edge_accepts_generator_expression_shapes() {
        let config = ParserConfig::default();
        let mut parser = RdfXmlStreamingParser::new(config);

        // OWL2Bench generator shapes: named classes pointing at blank-node
        // class expressions through equivalentClass/subClassOf deferred edges.
        let rdf_xml_content = r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:owl="http://www.w3.org/2002/07/owl#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#"
         xmlns:xsd="http://www.w3.org/2001/XMLSchema#">

    <owl:Class rdf:about="http://example.org/T20CricketFan">
        <owl:equivalentClass>
            <owl:Restriction>
                <owl:onProperty rdf:resource="http://example.org/isCrazyAbout"/>
                <owl:hasValue rdf:resource="http://example.org/T20Cricket"/>
            </owl:Restriction>
        </owl:equivalentClass>
    </owl:Class>

    <owl:Class rdf:about="http://example.org/SelfAwarePerson">
        <owl:equivalentClass>
            <owl:Class>
                <owl:intersectionOf rdf:parseType="Collection">
                    <rdf:Description rdf:about="http://example.org/Person"/>
                    <owl:Restriction>
                        <owl:onProperty rdf:resource="http://example.org/knows"/>
                        <owl:hasSelf rdf:datatype="http://www.w3.org/2001/XMLSchema#boolean">true</owl:hasSelf>
                    </owl:Restriction>
                </owl:intersectionOf>
            </owl:Class>
        </owl:equivalentClass>
    </owl:Class>

    <owl:Class rdf:about="http://example.org/UGStudent">
        <owl:equivalentClass>
            <owl:Class>
                <owl:intersectionOf rdf:parseType="Collection">
                    <rdf:Description rdf:about="http://example.org/Student"/>
                    <owl:Restriction>
                        <owl:onProperty rdf:resource="http://example.org/enrollFor"/>
                        <owl:qualifiedCardinality rdf:datatype="http://www.w3.org/2001/XMLSchema#nonNegativeInteger">1</owl:qualifiedCardinality>
                        <owl:onClass rdf:resource="http://example.org/UGProgram"/>
                    </owl:Restriction>
                </owl:intersectionOf>
            </owl:Class>
        </owl:equivalentClass>
    </owl:Class>

    <owl:Class rdf:about="http://example.org/College">
        <rdfs:subClassOf>
            <owl:Restriction>
                <owl:onProperty rdf:resource="http://example.org/isPartOf"/>
                <owl:someValuesFrom rdf:resource="http://example.org/University"/>
            </owl:Restriction>
        </rdfs:subClassOf>
    </owl:Class>

</rdf:RDF>"#;

        let result = parser.parse_content(rdf_xml_content);
        assert!(
            result.is_ok(),
            "Failed to parse deferred blank-node class edges: {:?}",
            result.err()
        );
        let ontology = result.unwrap();

        let has_value_axioms = ontology
            .subclass_axioms()
            .iter()
            .filter(|axiom| {
                matches!(
                    axiom.super_class(),
                    ClassExpression::ObjectHasValue(_, _)
                ) || matches!(axiom.sub_class(), ClassExpression::ObjectHasValue(_, _))
            })
            .count();
        assert_eq!(
            has_value_axioms, 2,
            "owl:hasValue restriction must model as ObjectHasValue on both subclass directions"
        );

        let intersection_has_operand = |axiom: &crate::logic::axioms::SubClassOfAxiom,
                                        predicate: &dyn Fn(&ClassExpression) -> bool| {
            [axiom.sub_class(), axiom.super_class()].iter().any(|side| {
                side.collect_subexpressions().iter().any(|expr| {
                    if let ClassExpression::ObjectIntersectionOf(operands) = expr {
                        operands.iter().any(|operand| predicate(operand))
                    } else {
                        false
                    }
                })
            })
        };
        let has_self_axioms = ontology
            .subclass_axioms()
            .iter()
            .filter(|axiom| {
                intersection_has_operand(axiom, &|operand| {
                    matches!(operand, ClassExpression::ObjectHasSelf(_))
                })
            })
            .count();
        assert_eq!(
            has_self_axioms, 2,
            "owl:hasSelf restriction must appear as an ObjectHasSelf intersection operand on both subclass directions"
        );

        let qualified_axioms = ontology
            .subclass_axioms()
            .iter()
            .filter(|axiom| {
                intersection_has_operand(axiom, &|operand| {
                    matches!(operand, ClassExpression::ObjectExactCardinality(..))
                })
            })
            .count();
        assert_eq!(
            qualified_axioms, 2,
            "owl:qualifiedCardinality restriction must appear as an ObjectExactCardinality intersection operand on both directions"
        );

        let some_values_axioms = ontology
            .subclass_axioms()
            .iter()
            .filter(|axiom| {
                matches!(
                    axiom.super_class(),
                    ClassExpression::ObjectSomeValuesFrom(..)
                )
            })
            .count();
        assert_eq!(
            some_values_axioms, 1,
            "blank-node rdfs:subClassOf edge must model the restriction as the superclass"
        );
    }

    #[cfg(feature = "rio-xml")]
    #[test]
    fn test_has_value_mixed_restriction_family_is_rejected() {
        let config = ParserConfig::default();
        let mut parser = RdfXmlStreamingParser::new(config);

        let rdf_xml_content = r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">

    <owl:Class rdf:about="http://example.org/Mixed">
        <owl:equivalentClass>
            <owl:Restriction>
                <owl:onProperty rdf:resource="http://example.org/p"/>
                <owl:hasValue rdf:resource="http://example.org/v"/>
                <owl:someValuesFrom rdf:resource="http://example.org/C"/>
            </owl:Restriction>
        </owl:equivalentClass>
    </owl:Class>

</rdf:RDF>"#;

        let result = parser.parse_content(rdf_xml_content);
        assert!(
            result.is_err(),
            "owl:hasValue mixed with another restriction family must fail closed, got {:?}",
            result.ok()
        );
    }

    #[cfg(feature = "rio-xml")]
    #[test]
    fn test_subject_to_iri_with_named_node() {
        let config = ParserConfig::default();
        let mut parser = RdfXmlStreamingParser::new(config);

        use rio_api::model::NamedNode;
        let named_node = NamedNode {
            iri: "http://example.org/test",
        };
        let subject = Subject::NamedNode(named_node);

        let result = parser.subject_to_iri(&subject);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_str(), "http://example.org/test");
    }

    #[cfg(feature = "rio-xml")]
    #[test]
    fn test_subject_to_iri_with_blank_node() {
        let config = ParserConfig::default();
        let mut parser = RdfXmlStreamingParser::new(config);

        use rio_api::model::BlankNode;
        let blank_node = BlankNode { id: "test123" };
        let subject = Subject::BlankNode(blank_node);

        let result = parser.subject_to_iri(&subject);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_str(), "_:test123");
    }

    #[cfg(feature = "rio-xml")]
    #[test]
    fn test_process_object_with_named_node() {
        let config = ParserConfig::default();
        let mut parser = RdfXmlStreamingParser::new(config);

        use rio_api::model::NamedNode;
        let named_node = NamedNode {
            iri: "http://example.org/object",
        };
        let term = Term::NamedNode(named_node);

        let result = parser.process_object(&term);
        assert!(result.is_ok());

        if let ProcessedObject::Iri(iri) = result.unwrap() {
            assert_eq!(iri.as_str(), "http://example.org/object");
        } else {
            panic!("Expected Iri object");
        }
    }

    #[cfg(feature = "rio-xml")]
    #[test]
    fn test_process_object_with_blank_node() {
        let config = ParserConfig::default();
        let mut parser = RdfXmlStreamingParser::new(config);

        use rio_api::model::BlankNode;
        let blank_node = BlankNode { id: "blank456" };
        let term = Term::BlankNode(blank_node);

        let result = parser.process_object(&term);
        assert!(result.is_ok());

        if let ProcessedObject::BlankNode(id) = result.unwrap() {
            assert_eq!(id, "blank456");
        } else {
            panic!("Expected BlankNode object");
        }
    }

    #[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
    #[test]
    fn test_experimental_pipeline_parity_basic_counts() {
        let config = ParserConfig::default();
        let mut baseline = RdfXmlStreamingParser::new(config.clone());
        let mut experimental = RdfXmlStreamingParser::new(config);

        let rdf_xml_content = r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:owl="http://www.w3.org/2002/07/owl#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#">
  <owl:Class rdf:about="http://example.org/A"/>
  <owl:Class rdf:about="http://example.org/B"/>
  <rdf:Description rdf:about="http://example.org/A">
    <rdfs:subClassOf rdf:resource="http://example.org/B"/>
  </rdf:Description>
</rdf:RDF>"#;

        let baseline_ontology = baseline
            .parse_content(rdf_xml_content)
            .expect("baseline parser should succeed");
        let experimental_ontology = experimental
            .parse_content_experimental(rdf_xml_content)
            .expect("experimental parser should succeed");

        assert_eq!(
            baseline_ontology.classes().len(),
            experimental_ontology.classes().len(),
            "class count mismatch between baseline and experimental parser"
        );
        assert_eq!(
            baseline_ontology.subclass_axioms().len(),
            experimental_ontology.subclass_axioms().len(),
            "subclass axiom count mismatch between baseline and experimental parser"
        );
    }

    #[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
    #[test]
    fn test_experimental_strict_mode_rejects_skipped() {
        let strict_err = RdfXmlStreamingParser::ensure_experimental_strict_mode(true, 1);
        assert!(
            strict_err.is_err(),
            "strict mode should fail when skipped > 0"
        );

        let strict_ok = RdfXmlStreamingParser::ensure_experimental_strict_mode(true, 0);
        assert!(
            strict_ok.is_ok(),
            "strict mode should pass when skipped == 0"
        );

        let non_strict_ok = RdfXmlStreamingParser::ensure_experimental_strict_mode(false, 99);
        assert!(
            non_strict_ok.is_ok(),
            "non-strict mode should allow skipped triples"
        );
    }

    #[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
    #[test]
    fn test_experimental_raw_term_rejects_rdf_star_terms() {
        use rio_api::model::{NamedNode, Triple};

        let inner_subject = Subject::NamedNode(NamedNode {
            iri: "http://example.org/s",
        });
        let inner_predicate = NamedNode {
            iri: "http://example.org/p",
        };
        let inner_object = Term::NamedNode(NamedNode {
            iri: "http://example.org/o",
        });
        let nested = Triple {
            subject: inner_subject,
            predicate: inner_predicate,
            object: inner_object,
        };

        let s = Subject::Triple(&nested);
        assert!(
            RdfXmlStreamingParser::raw_term_from_subject(s).is_none(),
            "RDF-star triple subject must be rejected by experimental converter"
        );

        let o = Term::Triple(&nested);
        assert!(
            RdfXmlStreamingParser::raw_term_from_object(o).is_none(),
            "RDF-star triple object must be rejected by experimental converter"
        );
    }

    #[cfg(feature = "rio-xml")]
    #[test]
    fn test_property_chain_middle_inverse_member_rewrites_in_place() {
        // #165: the auxiliary-role rewrite is position-independent. The
        // closure only composes two-step chains, so the middle position is
        // asserted at the axiom level: the chain records [x, aux, z] with
        // the aux role declared inverse to y.
        let config = ParserConfig::default();
        let mut parser = RdfXmlStreamingParser::new(config);

        let rdf_xml_content = r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:ObjectProperty rdf:about="http://example.org/x"/>
  <owl:ObjectProperty rdf:about="http://example.org/y"/>
  <owl:ObjectProperty rdf:about="http://example.org/z"/>
  <owl:ObjectProperty rdf:about="http://example.org/t">
    <owl:propertyChainAxiom rdf:parseType="Collection">
      <rdf:Description rdf:about="http://example.org/x"/>
      <rdf:Description>
        <owl:inverseOf rdf:resource="http://example.org/y"/>
      </rdf:Description>
      <rdf:Description rdf:about="http://example.org/z"/>
    </owl:propertyChainAxiom>
  </owl:ObjectProperty>
</rdf:RDF>"#;

        let ontology = parser
            .parse_content(rdf_xml_content)
            .expect("middle inverse chain member must parse");
        let chains = ontology.sub_property_chain_axioms();
        assert_eq!(chains.len(), 1, "exactly one chain axiom");
        assert_eq!(
            chains[0].super_property().as_str(),
            "http://example.org/t"
        );
        let members: Vec<&str> = chains[0]
            .properties()
            .iter()
            .map(|property| property.as_str())
            .collect();
        assert_eq!(members.len(), 3);
        assert_eq!(members[0], "http://example.org/x");
        assert!(
            members[1].starts_with("urn:tableauxx:inverse-role:"),
            "middle member must be the auxiliary role, got {}",
            members[1]
        );
        assert_eq!(members[2], "http://example.org/z");

        let inverse_axioms = ontology.inverse_object_properties_axioms();
        let auxiliary_pairs: Vec<_> = inverse_axioms
            .iter()
            .filter(|axiom| {
                axiom.property1().as_str() == members[1]
                    && axiom.property2().as_str() == "http://example.org/y"
            })
            .collect();
        assert_eq!(
            auxiliary_pairs.len(),
            1,
            "the auxiliary role must be declared inverse to y exactly once"
        );
    }

    #[cfg(feature = "rio-xml")]
    #[test]
    fn test_structural_interner_dedups_and_round_trips() {
        // Pins #170's Rc-backed interner: identical lexicals share one id,
        // distinct lexicals get distinct ids, and resolve() returns the
        // exact spelling back.
        let mut interner = StructuralInterner::with_capacity(4);
        let first = interner.intern("http://example.org/same");
        let repeat = interner.intern("http://example.org/same");
        let other = interner.intern("http://example.org/other");
        assert_eq!(first, repeat, "identical lexicals must share one term id");
        assert_ne!(first, other, "distinct lexicals must get distinct ids");
        assert_eq!(
            interner.resolve(first),
            Some("http://example.org/same"),
            "resolve must round-trip the exact spelling"
        );
        assert_eq!(interner.resolve(other), Some("http://example.org/other"));
        assert_eq!(interner.resolve(other + 1), None, "unused ids must not resolve");
    }

    #[cfg(feature = "rio-xml")]
    #[test]
    fn test_structural_subject_cursor_parity_on_repeated_subjects() {
        // #170's cursor reuses the previous subject's term id on consecutive
        // repeats. The fixture interleaves: a block with three consecutive
        // annotations (cursor hits), a second subject (cursor swap), a return
        // to the first subject (cursor miss, interner hit), and an anonymous
        // block (blank-node subject). The structural parse must produce the
        // same assertions as the streaming parse of the same document.
        let rdf_xml_content = r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:rdfs="http://www.w3.org/2000/01/rdf-schema#">
  <rdf:Description rdf:about="http://example.org/first">
    <rdfs:label>one</rdfs:label>
    <rdfs:comment>two</rdfs:comment>
    <rdfs:label>three</rdfs:label>
  </rdf:Description>
  <rdf:Description rdf:about="http://example.org/second">
    <rdfs:label>other</rdfs:label>
  </rdf:Description>
  <rdf:Description rdf:about="http://example.org/first">
    <rdfs:comment>back-again</rdfs:comment>
  </rdf:Description>
  <rdf:Description>
    <rdfs:label>anonymous</rdfs:label>
  </rdf:Description>
</rdf:RDF>"#;

        let assertion_fingerprint = |ontology: &Ontology| {
            let mut rows: Vec<String> = ontology
                .data_property_assertions()
                .into_iter()
                .map(|assertion| {
                    format!(
                        "{} {} {:?}",
                        assertion.subject().as_str(),
                        assertion.property().as_str(),
                        assertion.value()
                    )
                })
                .collect();
            rows.sort();
            rows
        };

        let mut streaming = RdfXmlStreamingParser::new(ParserConfig::default());
        let streaming_ontology = streaming
            .parse_content(rdf_xml_content)
            .expect("streaming parse of the fixture must succeed");
        let mut structural = RdfXmlStreamingParser::new(ParserConfig::default());
        let structural_ontology = structural
            .parse_content_structural(rdf_xml_content)
            .expect("structural parse of the fixture must succeed");

        let streaming_rows = assertion_fingerprint(&streaming_ontology);
        let structural_rows = assertion_fingerprint(&structural_ontology);
        assert_eq!(
            streaming_rows.len(),
            6,
            "fixture must produce six labeled/comment assertions"
        );
        assert_eq!(
            streaming_rows, structural_rows,
            "structural subject cursor must not change assertion identity"
        );
        assert_eq!(
            streaming_ontology.named_individuals().len(),
            structural_ontology.named_individuals().len(),
            "named-individual registration must stay in parity"
        );
    }
}
