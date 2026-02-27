//! GS1 EPCIS Ontology Implementation
//!
//! This module provides a comprehensive implementation of the GS1 EPCIS 2.0 standard
//! for supply chain traceability and event management using OWL2 reasoning.

use crate::*;
use std::collections::HashMap;
use std::time::SystemTime;

/// EPCIS Event Types according to GS1 EPCIS 2.0 standard
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EPCISEventType {
    /// ObjectEvent: Tracks individual EPCs
    ObjectEvent,
    /// AggregationEvent: Groups EPCs into containers
    AggregationEvent,
    /// TransactionEvent: Tracks ownership changes
    TransactionEvent,
    /// TransformationEvent: Tracks product transformations
    TransformationEvent,
}

/// EPCIS Action Types
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EPCISAction {
    /// ADD: Add EPC to the system
    Add,
    /// OBSERVE: Observe EPC without changing state
    Observe,
    /// DELETE: Remove EPC from system
    Delete,
}

/// Business Step identifiers
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EPCISBusinessStep {
    /// Manufacturing processes
    Manufacturing,
    Assembling,
    Commissioning,
    /// Distribution processes
    Receiving,
    Shipping,
    Loading,
    Unloading,
    /// Retail processes
    Picking,
    Packing,
    Selling,
    /// Quality and safety
    Inspecting,
    Testing,
    Certifying,
    /// Other
    Custom(String),
}

/// Disposition states
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EPCISDisposition {
    /// Product states
    InProgress,
    Complete,
    /// Inventory states
    InStock,
    OutOfStock,
    Reserved,
    /// Quality states
    Passed,
    Failed,
    UnderInspection,
    /// Regulatory states
    Quarantined,
    Recalled,
    Destroyed,
    /// Other
    Custom(String),
}

/// Supply Chain Participant
#[derive(Debug, Clone)]
pub struct SupplyChainParticipant {
    pub id: String,
    pub name: String,
    pub role: ParticipantRole,
    pub location: Option<BusinessLocation>,
    pub contact_info: HashMap<String, String>,
}

/// Participant Roles in Supply Chain
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ParticipantRole {
    Manufacturer,
    Distributor,
    Retailer,
    LogisticsProvider,
    Regulator,
    Consumer,
    ServiceProvider,
    Custom(String),
}

/// Business Location
#[derive(Debug, Clone)]
pub struct BusinessLocation {
    pub id: String,
    pub name: String,
    pub address: Address,
    pub coordinates: Option<(f64, f64)>,
    pub capabilities: Vec<LocationCapability>,
}

/// Address information
#[derive(Debug, Clone)]
pub struct Address {
    pub street: String,
    pub city: String,
    pub state: String,
    pub postal_code: String,
    pub country: String,
}

/// Location capabilities
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LocationCapability {
    Manufacturing,
    Warehousing,
    Distribution,
    Retail,
    QualityTesting,
    Certification,
    Customs,
}

/// Read Point (where event occurred)
#[derive(Debug, Clone)]
pub struct ReadPoint {
    pub id: String,
    pub name: String,
    pub location: BusinessLocation,
    pub reader_type: ReaderType,
}

/// Reader types
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReaderType {
    RFID,
    Barcode,
    Manual,
    IoT,
    API,
}

/// EPCIS Event Core Structure
#[derive(Debug, Clone)]
pub struct EPCISEvent {
    pub event_id: String,
    pub event_type: EPCISEventType,
    pub event_time: SystemTime,
    pub record_time: SystemTime,
    pub action: EPCISAction,
    pub biz_step: Option<EPCISBusinessStep>,
    pub disposition: Option<EPCISDisposition>,
    pub read_point: Option<ReadPoint>,
    pub business_location: Option<BusinessLocation>,
    pub epc_list: Vec<String>,               // EPC URNs
    pub quantity_list: HashMap<String, u32>, // EPC -> quantity
    pub child_epcs: Option<Vec<String>>,     // For aggregation events
    pub parent_id: Option<String>,           // For aggregation events
    pub business_transaction_list: Vec<BusinessTransaction>,
    pub source_list: Vec<SourceDestination>,
    pub destination_list: Vec<SourceDestination>,
    pub extension: HashMap<String, String>, // Custom extensions
}

/// Business Transaction Reference
#[derive(Debug, Clone)]
pub struct BusinessTransaction {
    pub transaction_type: String,
    pub transaction_id: String,
}

/// Source/Destination for transactions
#[derive(Debug, Clone)]
pub struct SourceDestination {
    pub source_type: String,
    pub source_id: String,
    pub destination_type: String,
    pub destination_id: String,
}

/// EPCIS Data Generator Configuration
#[derive(Debug, Clone)]
pub struct EPCISDataConfig {
    pub scale: DataScale,
    pub participant_count: usize,
    pub event_patterns: Vec<EventPattern>,
    pub time_span: std::time::Duration,
    pub include_extensions: bool,
}

/// Data Scale Categories
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataScale {
    Small,  // 100-500 events, 3-5 participants
    Medium, // 1K-5K events, 10-15 participants
    Large,  // 10K-50K events, 50+ participants
}

/// Event Pattern for realistic data generation
#[derive(Debug, Clone)]
pub struct EventPattern {
    pub event_type: EPCISEventType,
    pub frequency: f64, // Events per time unit
    pub participants: Vec<ParticipantRole>,
    pub business_steps: Vec<EPCISBusinessStep>,
    pub dispositions: Vec<EPCISDisposition>,
    pub weight: f64, // Probability weight
}

/// Traceability Scenario
#[derive(Debug, Clone)]
pub struct TraceabilityScenario {
    pub name: String,
    pub description: String,
    pub product_journey: Vec<JourneyStep>,
    pub participants: Vec<SupplyChainParticipant>,
    pub expected_trace: Vec<TraceEvent>,
}

/// Journey Step in product lifecycle
#[derive(Debug, Clone)]
pub struct JourneyStep {
    pub step_id: String,
    pub participant_id: String,
    pub location_id: String,
    pub event_type: EPCISEventType,
    pub business_step: EPCISBusinessStep,
    pub disposition: EPCISDisposition,
    pub expected_time_offset: std::time::Duration,
}

/// Trace Event for validation
#[derive(Debug, Clone, PartialEq)]
pub struct TraceEvent {
    pub event_id: String,
    pub epc: String,
    pub timestamp: SystemTime,
    pub location: String,
    pub participant: String,
    pub event_type: EPCISEventType,
}

/// Test Result
#[derive(Debug, Clone, PartialEq)]
pub enum TestResult {
    Success,
    Failure(String),
    PartialSuccess(Vec<String>),
}

impl EPCISEvent {
    /// Create a new EPCIS event
    pub fn new(event_id: String, event_type: EPCISEventType) -> Self {
        let now = SystemTime::now();
        Self {
            event_id,
            event_type,
            event_time: now,
            record_time: now,
            action: EPCISAction::Add,
            biz_step: None,
            disposition: None,
            read_point: None,
            business_location: None,
            epc_list: Vec::new(),
            quantity_list: HashMap::new(),
            child_epcs: None,
            parent_id: None,
            business_transaction_list: Vec::new(),
            source_list: Vec::new(),
            destination_list: Vec::new(),
            extension: HashMap::new(),
        }
    }

    /// Add EPC to event
    pub fn add_epc(&mut self, epc: String) {
        self.epc_list.push(epc);
    }

    /// Add EPC with quantity
    pub fn add_quantity(&mut self, epc: String, quantity: u32) {
        self.quantity_list.insert(epc, quantity);
    }

    /// Set business step
    pub fn with_business_step(mut self, step: EPCISBusinessStep) -> Self {
        self.biz_step = Some(step);
        self
    }

    /// Set disposition
    pub fn with_disposition(mut self, disposition: EPCISDisposition) -> Self {
        self.disposition = Some(disposition);
        self
    }

    /// Set read point
    pub fn with_read_point(mut self, read_point: ReadPoint) -> Self {
        self.read_point = Some(read_point);
        self
    }

    /// Set business location
    pub fn with_business_location(mut self, location: BusinessLocation) -> Self {
        self.business_location = Some(location);
        self
    }

    /// Convert to OWL2 ontology representation
    pub fn to_owl2(&self) -> OwlResult<(Ontology, Vec<String>)> {
        let mut ontology = Ontology::new();
        ontology.set_iri("http://example.org/epcis/test-ontology");

        let mut individual_iris = Vec::new();

        // Create EPCIS classes if they don't exist
        let event_class = Class::new("http://example.org/epcis/EPCISEvent");
        ontology.add_class(event_class.clone())?;

        // Create event individual
        let event_iri = format!("http://example.org/epcis/events/{}", self.event_id);
        let event_individual = NamedIndividual::new(event_iri.clone());
        ontology.add_named_individual(event_individual.clone())?;
        individual_iris.push(event_iri.clone());

        // Add class assertion for event
        let event_assertion = ClassAssertionAxiom::new(
            event_individual.iri().clone(),
            ClassExpression::from(event_class),
        );
        ontology.add_class_assertion(event_assertion)?;

        // Add event properties
        self.add_event_properties(&mut ontology, &event_iri)?;

        // Add participants, locations, etc.
        self.add_participants(&mut ontology)?;
        self.add_locations(&mut ontology)?;
        self.add_epcs(&mut ontology, &mut individual_iris)?;

        Ok((ontology, individual_iris))
    }

    fn add_event_properties(&self, ontology: &mut Ontology, event_iri: &str) -> OwlResult<()> {
        // Add event type property
        let event_type_prop = ObjectProperty::new("http://example.org/epcis/hasEventType");
        ontology.add_object_property(event_type_prop.clone())?;

        let event_type_class =
            Class::new(format!("http://example.org/epcis/{:?}", self.event_type));
        ontology.add_class(event_type_class.clone())?;

        let event_type_individual = NamedIndividual::new(format!(
            "http://example.org/epcis/types/{:?}",
            self.event_type
        ));
        ontology.add_named_individual(event_type_individual.clone())?;

        let type_assertion = PropertyAssertionAxiom::new(
            IRI::new_optimized(event_iri)?,
            event_type_prop.iri().clone(),
            event_type_individual.iri().clone(),
        );
        ontology.add_property_assertion(type_assertion)?;

        // Add event time property
        let time_prop = ObjectProperty::new("http://example.org/epcis/eventTime");
        ontology.add_object_property(time_prop.clone())?;

        Ok(())
    }

    fn add_participants(&self, ontology: &mut Ontology) -> OwlResult<()> {
        let participant_class = Class::new("http://example.org/epcis/Participant");
        ontology.add_class(participant_class)?;
        Ok(())
    }

    fn add_locations(&self, ontology: &mut Ontology) -> OwlResult<()> {
        let location_class = Class::new("http://example.org/epcis/BusinessLocation");
        ontology.add_class(location_class)?;
        Ok(())
    }

    fn add_epcs(
        &self,
        ontology: &mut Ontology,
        individual_iris: &mut Vec<String>,
    ) -> OwlResult<()> {
        let epc_class = Class::new("http://example.org/epcis/EPC");
        ontology.add_class(epc_class.clone())?;

        for epc in &self.epc_list {
            let epc_individual =
                NamedIndividual::new(format!("http://example.org/epcis/epcs/{}", epc));
            ontology.add_named_individual(epc_individual.clone())?;
            individual_iris.push(epc_individual.iri().as_str().to_string());

            let epc_assertion = ClassAssertionAxiom::new(
                epc_individual.iri().clone(),
                ClassExpression::from(epc_class.clone()),
            );
            ontology.add_class_assertion(epc_assertion)?;
        }

        Ok(())
    }
}

impl SupplyChainParticipant {
    /// Create a new participant
    pub fn new(id: String, name: String, role: ParticipantRole) -> Self {
        Self {
            id,
            name,
            role,
            location: None,
            contact_info: HashMap::new(),
        }
    }

    /// Set location
    pub fn with_location(mut self, location: BusinessLocation) -> Self {
        self.location = Some(location);
        self
    }

    /// Add contact info
    pub fn add_contact(mut self, key: String, value: String) -> Self {
        self.contact_info.insert(key, value);
        self
    }
}

impl BusinessLocation {
    /// Create a new business location
    pub fn new(id: String, name: String, address: Address) -> Self {
        Self {
            id,
            name,
            address,
            coordinates: None,
            capabilities: Vec::new(),
        }
    }

    /// Set coordinates
    pub fn with_coordinates(mut self, lat: f64, lon: f64) -> Self {
        self.coordinates = Some((lat, lon));
        self
    }

    /// Add capability
    pub fn add_capability(mut self, capability: LocationCapability) -> Self {
        self.capabilities.push(capability);
        self
    }
}

impl Address {
    /// Create new address
    pub fn new(
        street: String,
        city: String,
        state: String,
        postal_code: String,
        country: String,
    ) -> Self {
        Self {
            street,
            city,
            state,
            postal_code,
            country,
        }
    }
}

impl ReadPoint {
    /// Create new read point
    pub fn new(
        id: String,
        name: String,
        location: BusinessLocation,
        reader_type: ReaderType,
    ) -> Self {
        Self {
            id,
            name,
            location,
            reader_type,
        }
    }
}

impl EPCISDataConfig {
    /// Create small scale configuration
    pub fn small_scale() -> Self {
        Self {
            scale: DataScale::Small,
            participant_count: 5,
            event_patterns: vec![EventPattern {
                event_type: EPCISEventType::ObjectEvent,
                frequency: 10.0,
                participants: vec![ParticipantRole::Manufacturer, ParticipantRole::Distributor],
                business_steps: vec![
                    EPCISBusinessStep::Manufacturing,
                    EPCISBusinessStep::Receiving,
                ],
                dispositions: vec![EPCISDisposition::InProgress, EPCISDisposition::InStock],
                weight: 1.0,
            }],
            time_span: std::time::Duration::from_secs(86400), // 1 day
            include_extensions: false,
        }
    }

    /// Create medium scale configuration
    pub fn medium_scale() -> Self {
        Self {
            scale: DataScale::Medium,
            participant_count: 15,
            event_patterns: vec![
                EventPattern {
                    event_type: EPCISEventType::ObjectEvent,
                    frequency: 50.0,
                    participants: vec![
                        ParticipantRole::Manufacturer,
                        ParticipantRole::Distributor,
                        ParticipantRole::Retailer,
                    ],
                    business_steps: vec![
                        EPCISBusinessStep::Manufacturing,
                        EPCISBusinessStep::Shipping,
                        EPCISBusinessStep::Receiving,
                        EPCISBusinessStep::Selling,
                    ],
                    dispositions: vec![
                        EPCISDisposition::InProgress,
                        EPCISDisposition::InStock,
                        EPCISDisposition::OutOfStock,
                    ],
                    weight: 1.0,
                },
                EventPattern {
                    event_type: EPCISEventType::AggregationEvent,
                    frequency: 10.0,
                    participants: vec![ParticipantRole::Distributor],
                    business_steps: vec![EPCISBusinessStep::Packing],
                    dispositions: vec![EPCISDisposition::Complete],
                    weight: 0.5,
                },
            ],
            time_span: std::time::Duration::from_secs(604800), // 1 week
            include_extensions: true,
        }
    }

    /// Create large scale configuration
    pub fn large_scale() -> Self {
        Self {
            scale: DataScale::Large,
            participant_count: 50,
            event_patterns: vec![
                EventPattern {
                    event_type: EPCISEventType::ObjectEvent,
                    frequency: 200.0,
                    participants: vec![
                        ParticipantRole::Manufacturer,
                        ParticipantRole::Distributor,
                        ParticipantRole::Retailer,
                        ParticipantRole::LogisticsProvider,
                        ParticipantRole::Regulator,
                    ],
                    business_steps: vec![
                        EPCISBusinessStep::Manufacturing,
                        EPCISBusinessStep::Shipping,
                        EPCISBusinessStep::Receiving,
                        EPCISBusinessStep::Inspecting,
                        EPCISBusinessStep::Certifying,
                        EPCISBusinessStep::Selling,
                    ],
                    dispositions: vec![
                        EPCISDisposition::InProgress,
                        EPCISDisposition::InStock,
                        EPCISDisposition::OutOfStock,
                        EPCISDisposition::Passed,
                        EPCISDisposition::UnderInspection,
                        EPCISDisposition::Quarantined,
                    ],
                    weight: 1.0,
                },
                EventPattern {
                    event_type: EPCISEventType::TransactionEvent,
                    frequency: 50.0,
                    participants: vec![ParticipantRole::Distributor, ParticipantRole::Retailer],
                    business_steps: vec![EPCISBusinessStep::Receiving],
                    dispositions: vec![EPCISDisposition::InStock],
                    weight: 0.7,
                },
                EventPattern {
                    event_type: EPCISEventType::AggregationEvent,
                    frequency: 30.0,
                    participants: vec![ParticipantRole::Distributor],
                    business_steps: vec![EPCISBusinessStep::Packing],
                    dispositions: vec![EPCISDisposition::Complete],
                    weight: 0.5,
                },
            ],
            time_span: std::time::Duration::from_secs(2592000), // 30 days
            include_extensions: true,
        }
    }
}

impl std::fmt::Display for EPCISEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EPCISEventType::ObjectEvent => write!(f, "ObjectEvent"),
            EPCISEventType::AggregationEvent => write!(f, "AggregationEvent"),
            EPCISEventType::TransactionEvent => write!(f, "TransactionEvent"),
            EPCISEventType::TransformationEvent => write!(f, "TransformationEvent"),
        }
    }
}

impl std::fmt::Display for EPCISAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EPCISAction::Add => write!(f, "ADD"),
            EPCISAction::Observe => write!(f, "OBSERVE"),
            EPCISAction::Delete => write!(f, "DELETE"),
        }
    }
}

impl std::fmt::Display for DataScale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataScale::Small => write!(f, "Small (100-500 events)"),
            DataScale::Medium => write!(f, "Medium (1K-5K events)"),
            DataScale::Large => write!(f, "Large (10K-50K events)"),
        }
    }
}
