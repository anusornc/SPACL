//! Streaming RDF/XML parser using rio-xml library

use crate::logic::axioms::class_expressions::ClassExpression;
use crate::logic::axioms::*;
use crate::core::entities::*;
use crate::core::error::OwlResult;
use crate::core::iri::IRI;
use crate::core::ontology::Ontology;
use crate::parser::rdf_xml_common::{ERR_RIO_XML_PARSE, NS_OWL};
use crate::parser::{ParserArenaBuilder, ParserArenaTrait, ParserConfig};
use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

#[cfg(feature = "rio-xml")]
use rio_api::model::{Subject, Term, Triple};
#[cfg(feature = "rio-xml")]
use rio_api::parser::TriplesParser as _;
#[cfg(feature = "rio-xml")]
use rio_xml::RdfXmlParser as RioRdfXmlParser;
#[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
use crossbeam::channel::bounded;
#[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
use crossbeam::thread;
#[cfg(feature = "rio-xml")]
use rustc_hash::FxHashMap;
#[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
use std::borrow::Cow;
#[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
use std::cmp;
#[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
#[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
use dashmap::DashMap;
#[cfg(all(feature = "rio-xml", feature = "experimental-xml-parser"))]
use parking_lot::RwLock;

#[cfg(feature = "rio-xml")]
const RDF_TYPE_IRI: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
#[cfg(feature = "rio-xml")]
const RDFS_SUBCLASS_OF_IRI: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
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
const OWL_NAMED_INDIVIDUAL_IRI: &str = "http://www.w3.org/2002/07/owl#NamedIndividual";
#[cfg(feature = "rio-xml")]
const OWL_DISJOINT_WITH_IRI: &str = "http://www.w3.org/2002/07/owl#disjointWith";
#[cfg(feature = "rio-xml")]
const OWL_EQUIVALENT_CLASS_IRI: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
#[cfg(feature = "rio-xml")]
const RDF_XML_BUF_CAPACITY: usize = 1024 * 1024;
#[cfg(feature = "rio-xml")]
const STRUCTURAL_AUTO_THRESHOLD_BYTES_DEFAULT: u64 = 4 * 1024 * 1024;

#[cfg(feature = "rio-xml")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PredicateTag {
    RdfType,
    RdfsSubClassOf,
    RdfsDomain,
    RdfsRange,
    OwlDisjointWith,
    OwlEquivalentClass,
    OwlOther,
    Other,
}

#[cfg(feature = "rio-xml")]
#[derive(Clone, Debug, PartialEq, Eq)]
enum SubjectCacheKind {
    Named,
    Blank,
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
    forward: FxHashMap<String, StructuralTermId>,
    reverse: Vec<String>,
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
        let owned = value.to_string();
        self.reverse.push(owned.clone());
        self.forward.insert(owned, id);
        id
    }

    fn resolve(&self, id: StructuralTermId) -> Option<&str> {
        self.reverse.get(id as usize).map(String::as_str)
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
        self.reverse.read().get(id as usize).and_then(|entry| entry.clone())
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
            RDFS_DOMAIN_IRI => PredicateTag::RdfsDomain,
            RDFS_RANGE_IRI => PredicateTag::RdfsRange,
            OWL_DISJOINT_WITH_IRI => PredicateTag::OwlDisjointWith,
            OWL_EQUIVALENT_CLASS_IRI => PredicateTag::OwlEquivalentClass,
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
    fn read_len_prefixed_component<'a>(
        mut input: &'a str,
    ) -> OwlResult<(&'a str, &'a str)> {
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
        iri_cache: &mut [Option<IRI>],
    ) -> OwlResult<IRI> {
        if let Some(Some(existing)) = iri_cache.get(id as usize) {
            return Ok(existing.clone());
        }

        let iri = IRI::new_unchecked(Self::resolve_structural_term(interner, id)?)?;
        let slot = iri_cache.get_mut(id as usize).ok_or_else(|| {
            crate::core::error::OwlError::ParseError(format!(
                "Missing structural cache slot for term id: {}",
                id
            ))
        })?;
        *slot = Some(iri.clone());
        Ok(iri)
    }

    #[cfg(feature = "rio-xml")]
    fn resolve_structural_predicate_iri(
        &mut self,
        interner: &StructuralInterner,
        id: StructuralTermId,
        predicate_cache: &mut [Option<IRI>],
    ) -> OwlResult<IRI> {
        if let Some(Some(existing)) = predicate_cache.get(id as usize) {
            return Ok(existing.clone());
        }

        let iri = IRI::new_unchecked(Self::resolve_structural_term(interner, id)?)?;
        let slot = predicate_cache.get_mut(id as usize).ok_or_else(|| {
            crate::core::error::OwlError::ParseError(format!(
                "Missing structural predicate cache slot for term id: {}",
                id
            ))
        })?;
        *slot = Some(iri.clone());
        Ok(iri)
    }

    #[cfg(feature = "rio-xml")]
    fn resolve_structural_subject_iri(
        &mut self,
        interner: &StructuralInterner,
        subject: StructuralSubjectRecord,
        bnode_cache: &mut [Option<IRI>],
        iri_cache: &mut [Option<IRI>],
    ) -> OwlResult<IRI> {
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
                let iri = IRI::new_unchecked(label)?;
                let slot = bnode_cache.get_mut(id as usize).ok_or_else(|| {
                    crate::core::error::OwlError::ParseError(format!(
                        "Missing structural bnode cache slot for term id: {}",
                        id
                    ))
                })?;
                *slot = Some(iri.clone());
                Ok(iri)
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
                worker_handles.push(scope.spawn(move |_| -> Result<Vec<CompactTripleRecord>, String> {
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
                            stats_worker
                                .total_triples
                                .fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    Ok(local)
                }));
            }

            let producer_res = producer_handle
                .join()
                .map_err(|_| crate::core::error::OwlError::ParseError(
                    "Experimental producer thread panicked".to_string(),
                ))?;
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
            let object = Self::resolve_object(
                &interner,
                triple.o,
                &mut raw_term_cache,
                &mut object_cache,
            )?;

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

    /// Structural parser: phase A (ingest+intern), phase B (materialize axioms).
    #[cfg(feature = "rio-xml")]
    pub fn parse_stream_structural(&mut self, reader: impl std::io::BufRead) -> OwlResult<Ontology> {
        self.triple_counter = 0;
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
                Subject::NamedNode(node) => {
                    StructuralSubjectRecord::Iri(interner.intern(node.iri))
                }
                Subject::BlankNode(node) => {
                    StructuralSubjectRecord::BNode(interner.intern(node.id))
                }
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
        let term_capacity = interner.reverse.len();
        let mut bnode_subject_cache: Vec<Option<IRI>> = vec![None; term_capacity];
        let mut object_iri_cache: Vec<Option<IRI>> = vec![None; term_capacity];
        let mut predicate_cache: Vec<Option<IRI>> = vec![None; term_capacity];
        let mut bnode_object_cache: Vec<Option<String>> = vec![None; term_capacity];
        let breakdown_enabled = Self::structural_breakdown_enabled();
        let mut subject_ns: u128 = 0;
        let mut predicate_ns: u128 = 0;
        let mut object_ns: u128 = 0;
        let mut apply_ns: u128 = 0;
        let mut apply_rdf_type_ns: u128 = 0;
        let mut apply_subclass_ns: u128 = 0;
        let mut apply_domain_ns: u128 = 0;
        let mut apply_range_ns: u128 = 0;
        let mut apply_owl_family_ns: u128 = 0;
        let mut apply_other_ns: u128 = 0;
        let mut apply_rdf_type_count: u64 = 0;
        let mut apply_subclass_count: u64 = 0;
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
            let subject_iri = self.resolve_structural_subject_iri(
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
                StructuralObjectRecord::Iri(id) => ProcessedObject::Iri(
                    self.resolve_structural_object_iri(&interner, id, &mut object_iri_cache)?,
                ),
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
                subject_iri,
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
                "[phase] structural_apply_breakdown total_apply_ns={} rdf_type_ns={} rdf_type_count={} subclass_ns={} subclass_count={} domain_ns={} domain_count={} range_ns={} range_count={} owl_family_ns={} owl_family_count={} other_ns={} other_count={} rdf_type_pct={:.2} subclass_pct={:.2} domain_pct={:.2} range_pct={:.2} owl_family_pct={:.2} other_pct={:.2}",
                apply_ns,
                apply_rdf_type_ns,
                apply_rdf_type_count,
                apply_subclass_ns,
                apply_subclass_count,
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
                pct(apply_domain_ns, apply_ns),
                pct(apply_range_ns, apply_ns),
                pct(apply_owl_family_ns, apply_ns),
                pct(apply_other_ns, apply_ns),
            );
        }

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
            subject_iri,
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
        subject_iri: IRI,
        predicate_iri_str: &str,
        predicate_tag: PredicateTag,
        parsed_predicate: Option<IRI>,
        object: ProcessedObject,
    ) -> OwlResult<()> {
        match predicate_tag {
            PredicateTag::RdfType => {
                if let Some(object_iri) = object.as_iri() {
                    self.handle_type_assertion(ontology, &subject_iri, object_iri)?;
                }
            }
            PredicateTag::RdfsSubClassOf => {
                if let Some(object_iri) = object.as_iri() {
                    self.handle_subclass_of(ontology, &subject_iri, object_iri)?;
                }
            }
            PredicateTag::RdfsDomain => {
                if let Some(object_iri) = object.as_iri() {
                    self.handle_domain(ontology, &subject_iri, object_iri)?;
                }
            }
            PredicateTag::RdfsRange => {
                if let Some(object_iri) = object.as_iri() {
                    self.handle_range(ontology, &subject_iri, object_iri)?;
                }
            }
            PredicateTag::OwlDisjointWith
            | PredicateTag::OwlEquivalentClass
            | PredicateTag::OwlOther => {
                self.handle_owl_property(ontology, &subject_iri, predicate_tag, &object)?;
            }
            PredicateTag::Other => {
                let predicate_iri = match parsed_predicate {
                    Some(iri) => iri,
                    None => self.cached_predicate_iri(predicate_iri_str)?,
                };
                self.handle_property_assertion(ontology, &subject_iri, &predicate_iri, &object)?;
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
            subject_iri,
            &predicate_str,
            predicate_tag,
            Some(predicate_iri),
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
        subject: &IRI,
        object_iri: &IRI,
    ) -> OwlResult<()> {
        match object_iri.as_str() {
            OWL_CLASS_IRI => {
                if !ontology.contains_class_iri(subject) {
                    let class = <Class as Entity>::from_shared_iri(Arc::new(subject.clone()));
                    ontology.add_class(class)?;
                }
            }
            OWL_OBJECT_PROPERTY_IRI => {
                let property =
                    <ObjectProperty as Entity>::from_shared_iri(Arc::new(subject.clone()));
                ontology.add_object_property(property)?;
            }
            OWL_DATATYPE_PROPERTY_IRI => {
                let property = <DataProperty as Entity>::from_shared_iri(Arc::new(subject.clone()));
                ontology.add_data_property(property)?;
            }
            OWL_NAMED_INDIVIDUAL_IRI => {
                if !ontology.contains_named_individual_iri(subject) {
                    let individual =
                        <NamedIndividual as Entity>::from_shared_iri(Arc::new(subject.clone()));
                    ontology.add_named_individual(individual)?;
                }
            }
            _ => {
                // Generic type assertion
                if !ontology.contains_named_individual_iri(subject) {
                    ontology.add_named_individual(
                        <NamedIndividual as Entity>::from_shared_iri(Arc::new(subject.clone())),
                    )?;
                }
                let class = <Class as Entity>::from_shared_iri(Arc::new(object_iri.clone()));
                let assertion = ClassAssertionAxiom::new(
                    Arc::new(subject.clone()),
                    ClassExpression::Class(class),
                );
                ontology.add_class_assertion(assertion)?;
            }
        }
        Ok(())
    }

    /// Handle subclass relationships
    #[cfg(feature = "rio-xml")]
    fn handle_subclass_of(
        &mut self,
        ontology: &mut Ontology,
        subject: &IRI,
        object_iri: &IRI,
    ) -> OwlResult<()> {
        let subclass = <Class as Entity>::from_shared_iri(Arc::new(subject.clone()));
        let superclass = <Class as Entity>::from_shared_iri(Arc::new(object_iri.clone()));
        let axiom = SubClassOfAxiom::new(
            ClassExpression::Class(subclass),
            ClassExpression::Class(superclass),
        );
        ontology.add_subclass_axiom(axiom)?;
        Ok(())
    }

    /// Handle domain declarations
    #[cfg(feature = "rio-xml")]
    fn handle_domain(
        &mut self,
        ontology: &mut Ontology,
        subject: &IRI,
        object_iri: &IRI,
    ) -> OwlResult<()> {
        // This is simplified - in practice, you'd need to determine the property type
        let axiom = ObjectPropertyDomainAxiom::new(
            Arc::new(subject.clone()),
            Arc::new(object_iri.clone()),
        );
        // Add as generic axiom for now
        ontology.add_axiom(crate::logic::axioms::Axiom::ObjectPropertyDomain(Box::new(axiom)))?;
        Ok(())
    }

    /// Handle range declarations
    #[cfg(feature = "rio-xml")]
    fn handle_range(
        &mut self,
        ontology: &mut Ontology,
        subject: &IRI,
        object_iri: &IRI,
    ) -> OwlResult<()> {
        // For object property range
        let axiom = ObjectPropertyRangeAxiom::new(
            Arc::new(subject.clone()),
            Arc::new(object_iri.clone()),
        );
        // Add as generic axiom for now
        ontology.add_axiom(crate::logic::axioms::Axiom::ObjectPropertyRange(Box::new(axiom)))?;
        Ok(())
    }

    /// Handle OWL-specific properties
    #[cfg(feature = "rio-xml")]
    fn handle_owl_property(
        &mut self,
        ontology: &mut Ontology,
        subject: &IRI,
        predicate_tag: PredicateTag,
        object: &ProcessedObject,
    ) -> OwlResult<()> {
        match predicate_tag {
            PredicateTag::OwlDisjointWith => {
                if let Some(object_iri) = object.as_iri() {
                    let axiom = DisjointClassesAxiom::new(vec![
                        Arc::new(subject.clone()),
                        Arc::new(object_iri.clone()),
                    ]);
                    ontology.add_disjoint_classes_axiom(axiom)?;
                }
            }
            PredicateTag::OwlEquivalentClass => {
                if let Some(object_iri) = object.as_iri() {
                    let axiom = EquivalentClassesAxiom::new(vec![
                        Arc::new(subject.clone()),
                        Arc::new(object_iri.clone()),
                    ]);
                    ontology.add_equivalent_classes_axiom(axiom)?;
                }
            }
            PredicateTag::OwlOther
            | PredicateTag::RdfType
            | PredicateTag::RdfsSubClassOf
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
        subject: &IRI,
        predicate: &IRI,
        object: &ProcessedObject,
    ) -> OwlResult<()> {
        // Property assertions imply the subject is an individual.
        if !ontology.contains_named_individual_iri(subject) {
            ontology.add_named_individual(<NamedIndividual as Entity>::from_shared_iri(
                Arc::new(subject.clone()),
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
                    Arc::new(subject.clone()),
                    Arc::new(predicate.clone()),
                    Arc::new(object_iri.clone()),
                );
                ontology.add_property_assertion(assertion)?;
            }
            ProcessedObject::BlankNode(node_id) => {
                // Object property with anonymous individual
                let anon_individual =
                    AnonymousIndividual::new(self.cached_blank_node_label(node_id).as_ref().to_string());
                ontology.add_anonymous_individual(anon_individual.clone())?;

                let assertion = PropertyAssertionAxiom::new_with_anonymous(
                    Arc::new(subject.clone()),
                    Arc::new(predicate.clone()),
                    anon_individual,
                );
                ontology.add_property_assertion(assertion)?;
            }
            ProcessedObject::Literal(literal) => {
                // Data property with literal value
                let assertion = DataPropertyAssertionAxiom::new(
                    Arc::new(subject.clone()),
                    Arc::new(predicate.clone()),
                    literal.clone(),
                );
                ontology.add_data_property_assertion(assertion)?;
            }
        }

        Ok(())
    }
}

/// Processed object representation
#[derive(Debug, Clone)]
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
        assert!(strict_err.is_err(), "strict mode should fail when skipped > 0");

        let strict_ok = RdfXmlStreamingParser::ensure_experimental_strict_mode(true, 0);
        assert!(strict_ok.is_ok(), "strict mode should pass when skipped == 0");

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
}
