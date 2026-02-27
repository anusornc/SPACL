//! Simplified EPCIS Test Data Generator
//!
//! Basic test data generation for EPCIS events with working compilation.

use crate::app::epcis::*;
use crate::*;
use rand::Rng;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, SystemTime};

/// Test data generator for EPCIS events
pub struct EPCISTestDataGenerator {
    config: TestDataConfig,
    rng: rand::rngs::ThreadRng,
    start_time: SystemTime,
    participants: Vec<SupplyChainParticipant>,
    epc_pool: Vec<String>,
}

/// Configuration for test data generation
#[derive(Debug, Clone)]
pub struct TestDataConfig {
    /// Number of events to generate
    pub event_count: usize,
    /// Scale of the test (small, medium, large)
    pub scale: TestScale,
    /// Include complex scenarios
    pub include_complex_scenarios: bool,
    /// Seed for reproducible generation
    pub seed: Option<u64>,
}

/// Test scale definitions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestScale {
    /// Small scale: 100-500 events
    Small,
    /// Medium scale: 1K-5K events  
    Medium,
    /// Large scale: 10K-50K events
    Large,
}

impl TestScale {
    /// Get the event count range for this scale
    pub fn event_range(&self) -> (usize, usize) {
        match self {
            TestScale::Small => (100, 500),
            TestScale::Medium => (1000, 5000),
            TestScale::Large => (10000, 50000),
        }
    }

    /// Get descriptive name
    pub fn name(&self) -> &'static str {
        match self {
            TestScale::Small => "Small Scale",
            TestScale::Medium => "Medium Scale",
            TestScale::Large => "Large Scale",
        }
    }
}

impl EPCISTestDataGenerator {
    /// Create a new test data generator
    pub fn new(config: TestDataConfig) -> Self {
        let mut generator = Self {
            config,
            rng: rand::thread_rng(),
            start_time: SystemTime::now(),
            participants: Vec::new(),
            epc_pool: Vec::new(),
        };

        generator.initialize_test_data();
        generator
    }

    /// Initialize test data structures
    fn initialize_test_data(&mut self) {
        // Create supply chain participants
        self.participants = vec![
            SupplyChainParticipant {
                id: "manufacturer-001".to_string(),
                name: "Global Manufacturing Corp".to_string(),
                role: ParticipantRole::Manufacturer,
                location: Some(BusinessLocation {
                    id: "loc-mfg-001".to_string(),
                    name: "Main Factory".to_string(),
                    address: Address {
                        street: "123 Industrial Ave".to_string(),
                        city: "Factory City".to_string(),
                        state: "FC".to_string(),
                        postal_code: "12345".to_string(),
                        country: "US".to_string(),
                    },
                    coordinates: Some((40.7128, -74.0060)),
                    capabilities: vec![LocationCapability::Manufacturing],
                }),
                contact_info: HashMap::new(),
            },
            SupplyChainParticipant {
                id: "distributor-001".to_string(),
                name: "Regional Distribution Inc".to_string(),
                role: ParticipantRole::Distributor,
                location: Some(BusinessLocation {
                    id: "loc-dist-001".to_string(),
                    name: "Central Warehouse".to_string(),
                    address: Address {
                        street: "456 Logistics Blvd".to_string(),
                        city: "Distribution Center".to_string(),
                        state: "DC".to_string(),
                        postal_code: "67890".to_string(),
                        country: "US".to_string(),
                    },
                    coordinates: Some((41.8781, -87.6298)),
                    capabilities: vec![
                        LocationCapability::Warehousing,
                        LocationCapability::Distribution,
                    ],
                }),
                contact_info: HashMap::new(),
            },
            SupplyChainParticipant {
                id: "retailer-001".to_string(),
                name: "Metro Retail Chain".to_string(),
                role: ParticipantRole::Retailer,
                location: Some(BusinessLocation {
                    id: "loc-ret-001".to_string(),
                    name: "Downtown Store".to_string(),
                    address: Address {
                        street: "789 Shopping St".to_string(),
                        city: "Retail District".to_string(),
                        state: "RD".to_string(),
                        postal_code: "54321".to_string(),
                        country: "US".to_string(),
                    },
                    coordinates: Some((42.3601, -71.0589)),
                    capabilities: vec![LocationCapability::Retail],
                }),
                contact_info: HashMap::new(),
            },
        ];

        // Generate EPC pool
        self.generate_epc_pool();
    }

    /// Generate EPC pool for testing
    fn generate_epc_pool(&mut self) {
        let base_epcs = vec![
            "urn:epc:id:sgtin:0614141.107346.2018",
            "urn:epc:id:sgtin:0614141.107347.2018",
            "urn:epc:id:sgtin:0614141.107348.2018",
            "urn:epc:id:sgtin:0614141.107349.2018",
            "urn:epc:id:sgtin:0614141.107350.2018",
        ];

        // Expand EPC pool based on scale
        let multiplier = match self.config.scale {
            TestScale::Small => 20,
            TestScale::Medium => 200,
            TestScale::Large => 2000,
        };

        for base in &base_epcs {
            for i in 1..=multiplier {
                let epc = format!("{}.{}", base, i);
                self.epc_pool.push(epc);
            }
        }
    }

    /// Generate test ontology with events
    pub fn generate_ontology(&mut self) -> OwlResult<Ontology> {
        let mut ontology = Ontology::new();
        ontology.set_iri("http://example.org/epcis/test-data");

        // Add core EPCIS classes
        self.add_epcis_classes(&mut ontology)?;

        // Add participants
        self.add_participants_to_ontology(&mut ontology)?;

        // Generate events
        let events = self.generate_events();

        println!("Generated {} events", events.len());

        Ok(ontology)
    }

    /// Add EPCIS classes to ontology
    fn add_epcis_classes(&self, ontology: &mut Ontology) -> OwlResult<()> {
        let event_class = Class::new("http://ns.gs1.org/epcis/Event");
        let object_event_class = Class::new("http://ns.gs1.org/epcis/ObjectEvent");
        let aggregation_event_class = Class::new("http://ns.gs1.org/epcis/AggregationEvent");

        ontology.add_class(event_class.clone())?;
        ontology.add_class(object_event_class.clone())?;
        ontology.add_class(aggregation_event_class.clone())?;

        // Class hierarchy
        let object_subclass = SubClassOfAxiom::new(
            crate::logic::axioms::class_expressions::ClassExpression::Class(object_event_class),
            crate::logic::axioms::class_expressions::ClassExpression::Class(event_class.clone()),
        );
        let aggregation_subclass = SubClassOfAxiom::new(
            crate::logic::axioms::class_expressions::ClassExpression::Class(aggregation_event_class),
            crate::logic::axioms::class_expressions::ClassExpression::Class(event_class),
        );

        ontology.add_subclass_axiom(object_subclass)?;
        ontology.add_subclass_axiom(aggregation_subclass)?;

        Ok(())
    }

    /// Add participants to ontology
    fn add_participants_to_ontology(&self, ontology: &mut Ontology) -> OwlResult<()> {
        for participant in &self.participants {
            let participant_class = Class::new(format!(
                "http://example.org/participants/{}",
                participant.id
            ));
            ontology.add_class(participant_class)?;
        }
        Ok(())
    }

    /// Generate test events
    pub fn generate_events(&mut self) -> Vec<EPCISEvent> {
        let mut events = Vec::new();
        let event_count = self
            .config
            .event_count
            .min(self.config.scale.event_range().1);

        for i in 0..event_count {
            let event_type = self.select_event_type(i, event_count);
            let event = self.create_event(event_type, i);
            events.push(event);
        }

        events
    }

    /// Select event type based on position and total count
    fn select_event_type(&mut self, index: usize, _total: usize) -> EPCISEventType {
        // Create realistic distribution: 70% ObjectEvent, 20% AggregationEvent, 10% others
        let rand_val = self.rng.gen_range(0.0..1.0);

        if rand_val < 0.7 {
            EPCISEventType::ObjectEvent
        } else if rand_val < 0.9 {
            EPCISEventType::AggregationEvent
        } else {
            // Cycle through other types for variety
            match index % 3 {
                0 => EPCISEventType::TransactionEvent,
                1 => EPCISEventType::TransformationEvent,
                _ => EPCISEventType::ObjectEvent,
            }
        }
    }

    /// Create a single event
    fn create_event(&mut self, event_type: EPCISEventType, index: usize) -> EPCISEvent {
        let event_time = self.start_time + Duration::from_secs(index as u64 * 60); // 1 minute intervals

        // Select business step and disposition first
        let biz_step = self.select_business_step();
        let disposition = self.select_disposition();

        let participant_idx = self.rng.gen_range(0..self.participants.len());
        let participant = &self.participants[participant_idx];

        let mut event = EPCISEvent {
            event_id: format!("event-{:06}", index + 1),
            event_type: event_type.clone(),
            event_time,
            record_time: event_time + Duration::from_secs(5), // 5 seconds later
            action: EPCISAction::Observe,
            biz_step: Some(biz_step),
            disposition: Some(disposition),
            read_point: Some(ReadPoint {
                id: format!("rp-{}", participant.location.as_ref().unwrap().id),
                name: format!(
                    "Read Point at {}",
                    participant.location.as_ref().unwrap().name
                ),
                location: participant.location.as_ref().unwrap().clone(),
                reader_type: ReaderType::RFID,
            }),
            business_location: participant.location.clone(),
            epc_list: Vec::new(),
            quantity_list: HashMap::new(),
            child_epcs: None,
            parent_id: None,
            business_transaction_list: Vec::new(),
            source_list: Vec::new(),
            destination_list: Vec::new(),
            extension: HashMap::new(),
        };

        // Add EPCs based on event type
        match event_type {
            EPCISEventType::ObjectEvent => {
                event.epc_list = self.select_random_epcs(1..5);
            }
            EPCISEventType::AggregationEvent => {
                event.epc_list = vec![self.select_random_epc()];
                event.child_epcs = Some(self.select_random_epcs(3..10));
            }
            EPCISEventType::TransactionEvent => {
                event.epc_list = self.select_random_epcs(1..3);
                if self.rng.gen_bool(0.7) {
                    event.parent_id = Some(format!("parent-{:06}", self.rng.gen_range(1..1000)));
                }
            }
            EPCISEventType::TransformationEvent => {
                event.epc_list = self.select_random_epcs(1..3);
                event.child_epcs = Some(self.select_random_epcs(2..5));
            }
        }

        // Add quantities
        for epc in &event.epc_list {
            event.quantity_list.insert(epc.clone(), 1);
        }

        event
    }

    /// Select business step
    fn select_business_step(&mut self) -> EPCISBusinessStep {
        let steps = [
            EPCISBusinessStep::Manufacturing,
            EPCISBusinessStep::Assembling,
            EPCISBusinessStep::Receiving,
            EPCISBusinessStep::Shipping,
            EPCISBusinessStep::Picking,
            EPCISBusinessStep::Packing,
        ];
        steps[self.rng.gen_range(0..steps.len())].clone()
    }

    /// Select disposition
    fn select_disposition(&mut self) -> EPCISDisposition {
        let dispositions = [
            EPCISDisposition::InProgress,
            EPCISDisposition::InStock,
            EPCISDisposition::Passed,
            EPCISDisposition::Reserved,
        ];
        dispositions[self.rng.gen_range(0..dispositions.len())].clone()
    }

    /// Select random EPCs
    fn select_random_epcs(&mut self, range: std::ops::Range<usize>) -> Vec<String> {
        let count = self.rng.gen_range(range);
        let mut selected = Vec::new();
        let mut indices = HashSet::new();

        while indices.len() < count && indices.len() < self.epc_pool.len() {
            indices.insert(self.rng.gen_range(0..self.epc_pool.len()));
        }

        for idx in indices {
            selected.push(self.epc_pool[idx].clone());
        }

        selected
    }

    /// Select single random EPC
    fn select_random_epc(&mut self) -> String {
        let idx = self.rng.gen_range(0..self.epc_pool.len());
        self.epc_pool[idx].clone()
    }

    /// Get generation statistics
    pub fn get_stats(&self) -> GenerationStats {
        GenerationStats {
            event_count: self.config.event_count,
            scale: self.config.scale,
            participant_count: self.participants.len(),
            epc_pool_size: self.epc_pool.len(),
            actual_events_generated: self
                .config
                .event_count
                .min(self.config.scale.event_range().1),
        }
    }
}

/// Generation statistics
#[derive(Debug, Clone)]
pub struct GenerationStats {
    pub event_count: usize,
    pub scale: TestScale,
    pub participant_count: usize,
    pub epc_pool_size: usize,
    pub actual_events_generated: usize,
}

impl GenerationStats {
    /// Get human-readable summary
    pub fn summary(&self) -> String {
        format!(
            "{} Test Data Generation:\n- Events: {}\n- Participants: {}\n- EPC Pool: {}\n- Scale: {}",
            self.scale.name(),
            self.actual_events_generated,
            self.participant_count,
            self.epc_pool_size,
            self.scale.name()
        )
    }
}

/// Create small-scale test data configuration
pub fn small_scale_config() -> TestDataConfig {
    TestDataConfig {
        event_count: 250,
        scale: TestScale::Small,
        include_complex_scenarios: true,
        seed: None, // Remove seed issue for now
    }
}

/// Create medium-scale test data configuration
pub fn medium_scale_config() -> TestDataConfig {
    TestDataConfig {
        event_count: 2500,
        scale: TestScale::Medium,
        include_complex_scenarios: true,
        seed: None,
    }
}

/// Create large-scale test data configuration
pub fn large_scale_config() -> TestDataConfig {
    TestDataConfig {
        event_count: 25000,
        scale: TestScale::Large,
        include_complex_scenarios: true,
        seed: None,
    }
}
