//! Global Memory Protection System
//!
//! This module provides comprehensive memory protection mechanisms to prevent out-of-memory conditions
//! and ensure graceful degradation under memory pressure. It implements circuit breakers, automatic cleanup,
//! and memory-aware allocation strategies for the OWL2 reasoner.

use crate::util::cache_manager;
use crate::core::iri::clear_global_iri_cache;
use crate::util::memory::*;
use parking_lot::Mutex;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};

/// Global memory protection configuration
#[derive(Debug, Clone)]
pub struct MemoryProtectionConfig {
    /// Global memory limit in bytes
    pub global_memory_limit: usize,
    /// Warning threshold (percentage of global limit)
    pub warning_threshold_percent: f64,
    /// Critical threshold (percentage of global limit)
    pub critical_threshold_percent: f64,
    /// Emergency threshold (percentage of global limit)
    pub emergency_threshold_percent: f64,
    /// Enable automatic cleanup
    pub enable_auto_cleanup: bool,
    /// Cleanup check interval
    pub cleanup_check_interval: Duration,
    /// Graceful degradation enabled
    pub enable_graceful_degradation: bool,
}

impl Default for MemoryProtectionConfig {
    fn default() -> Self {
        Self {
            global_memory_limit: 2 * 1024 * 1024 * 1024, // 2GB
            warning_threshold_percent: 0.7,              // 70%
            critical_threshold_percent: 0.85,            // 85%
            emergency_threshold_percent: 0.95,           // 95%
            enable_auto_cleanup: true,
            cleanup_check_interval: Duration::from_secs(30),
            enable_graceful_degradation: true,
        }
    }
}

/// Memory protection state and actions
#[derive(Debug, Clone)]
pub enum MemoryProtectionState {
    Normal,
    Warning,
    Critical,
    Emergency,
}

/// Circuit breaker state for preventing memory exhaustion
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    /// Current state
    state: Arc<Mutex<CircuitBreakerState>>,
    /// Last trip time
    last_trip_time: Arc<Mutex<Instant>>,
    /// Trip threshold duration
    trip_duration: Duration,
    /// Failure threshold
    failure_threshold: usize,
    /// Failure count
    failure_count: Arc<AtomicUsize>,
    /// Is circuit currently open
    is_open: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
struct CircuitBreakerState {
    is_open: bool,
    failure_count: usize,
    last_failure_time: Instant,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: usize, trip_duration: Duration) -> Self {
        Self {
            state: Arc::new(Mutex::new(CircuitBreakerState {
                is_open: false,
                failure_count: 0,
                last_failure_time: Instant::now(),
            })),
            last_trip_time: Arc::new(Mutex::new(Instant::now())),
            trip_duration,
            failure_threshold,
            failure_count: Arc::new(AtomicUsize::new(0)),
            is_open: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_open(&self) -> bool {
        self.is_open.load(Ordering::Relaxed)
    }

    pub fn check_and_maybe_open(&self) -> bool {
        if self.is_open.load(Ordering::Relaxed) {
            // Check if we should close the circuit breaker
            let mut last_trip = self.last_trip_time.lock();
            if last_trip.elapsed() >= self.trip_duration {
                // Close the circuit breaker
                let mut state = self.state.lock();
                state.is_open = false;
                state.failure_count = 0;
                self.is_open.store(false, Ordering::Relaxed);
                *last_trip = Instant::now();
                false
            } else {
                true // Still open
            }
        } else {
            true // Circuit is closed
        }
    }

    pub fn record_failure(&self) {
        let mut should_open = false;

        {
            let mut state = self.state.lock();
            state.failure_count += 1;
            state.last_failure_time = Instant::now();

            if state.failure_count >= self.failure_threshold {
                state.is_open = true;
                should_open = true;
                {
                    let mut last_trip = self.last_trip_time.lock();
                    *last_trip = Instant::now();
                }
            }
        }

        if should_open {
            self.failure_count.fetch_add(1, Ordering::Relaxed);
            self.is_open.store(true, Ordering::Relaxed);
        }
    }
}

#[derive(Debug, Default, Clone)]
struct EmergencyProtection;

impl EmergencyProtection {
    fn new() -> Self {
        Self
    }

    fn start_emergency_protection(&mut self) {}

    fn stop_emergency_protection(&mut self) {}

    fn can_emergency_allocate(&self, _requested_bytes: usize) -> bool {
        true
    }
}

/// Global memory protection system
pub struct MemoryProtection {
    config: Arc<Mutex<MemoryProtectionConfig>>,
    current_state: Arc<Mutex<MemoryProtectionState>>,
    protection_circuit_breaker: Arc<CircuitBreaker>,
    global_stats: Arc<Mutex<GlobalMemoryStats>>,
    protection_thread: Option<thread::JoinHandle<()>>,
    shutdown_flag: Arc<AtomicBool>,
    emergency_protection: Arc<Mutex<EmergencyProtection>>,
}

/// Global memory statistics
#[derive(Debug, Default, Clone)]
pub struct GlobalMemoryStats {
    pub total_memory_usage: usize,
    pub peak_memory_usage: usize,
    pub cache_size: usize,
    pub arena_count: usize,
    pub reasoning_graph_size: usize,
    pub cleanup_count: usize,
    pub circuit_breaker_trips: usize,
    pub last_cleanup_time: Option<Instant>,
}

impl Default for MemoryProtection {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryProtection {
    pub fn new() -> Self {
        let config = MemoryProtectionConfig::default();
        let protection_circuit_breaker = Arc::new(CircuitBreaker::new(
            5,                       // 5 failures before opening
            Duration::from_secs(60), // Open for 1 minute
        ));

        Self {
            config: Arc::new(Mutex::new(config)),
            current_state: Arc::new(Mutex::new(MemoryProtectionState::Normal)),
            protection_circuit_breaker,
            global_stats: Arc::new(Mutex::new(GlobalMemoryStats::default())),
            protection_thread: None,
            shutdown_flag: Arc::new(AtomicBool::new(false)),
            emergency_protection: Arc::new(Mutex::new(EmergencyProtection::new())),
        }
    }

    pub fn with_config(config: MemoryProtectionConfig) -> Self {
        let protection = Self::new();
        *protection.config.lock() = config;
        protection
    }

    /// Start the memory protection system
    pub fn start_protection(&mut self) {
        if self.protection_thread.is_some() {
            return; // Already running
        }

        // Start emergency protection first
        {
            let mut emergency = self.emergency_protection.lock();
            emergency.start_emergency_protection();
        }

        self.shutdown_flag.store(false, Ordering::Relaxed);

        let config = self.config.clone();
        let current_state = self.current_state.clone();
        let global_stats = self.global_stats.clone();
        let protection_circuit_breaker = self.protection_circuit_breaker.clone();
        let shutdown_flag = Arc::clone(&self.shutdown_flag);

        let handle = thread::spawn(move || {
            let mut consecutive_warnings = 0;
            let mut consecutive_critical = 0;

            while !shutdown_flag.load(Ordering::Relaxed) {
                let current_memory = get_memory_stats().total_usage;
                let config = config.lock();

                // Update global statistics
                {
                    let mut stats = global_stats.lock();
                    stats.total_memory_usage = current_memory;
                    stats.peak_memory_usage = stats.peak_memory_usage.max(current_memory);

                    // Update other stats from various components
                    if let Ok(cache_size) =
                        cache_manager::global_cache_manager().get_iri_cache_size()
                    {
                        stats.cache_size = cache_size;
                    }
                }

                // Calculate memory pressure percentage
                let memory_pressure = if config.global_memory_limit > 0 {
                    current_memory as f64 / config.global_memory_limit as f64
                } else {
                    0.0
                };

                // Determine protection state
                let new_state = if memory_pressure >= config.emergency_threshold_percent {
                    consecutive_warnings = 0;
                    consecutive_critical = 0;

                    // Trigger emergency protection
                    Self::trigger_emergency_protection(
                        &config,
                        &global_stats,
                        &protection_circuit_breaker,
                    );

                    MemoryProtectionState::Emergency
                } else if memory_pressure >= config.critical_threshold_percent {
                    consecutive_warnings = 0;
                    consecutive_critical += 1;

                    if consecutive_critical >= 3 {
                        // Trip circuit breaker after 3 critical warnings
                        protection_circuit_breaker.record_failure();
                    }

                    // Trigger critical protection
                    Self::trigger_critical_protection(&config, &global_stats);

                    MemoryProtectionState::Critical
                } else if memory_pressure >= config.warning_threshold_percent {
                    consecutive_warnings += 1;
                    consecutive_critical = 0;

                    if consecutive_warnings >= 5 {
                        // Trip circuit breaker after 5 warnings
                        protection_circuit_breaker.record_failure();
                    }

                    // Trigger warning protection
                    Self::trigger_warning_protection(&config, &global_stats);

                    MemoryProtectionState::Warning
                } else {
                    consecutive_warnings = 0;
                    consecutive_critical = 0;
                    MemoryProtectionState::Normal
                };

                // Update state
                {
                    let mut state = current_state.lock();
                    *state = new_state;
                }

                // Sleep until next check
                if shutdown_flag.load(Ordering::Relaxed) {
                    break;
                }
                thread::sleep(config.cleanup_check_interval);
            }
        });

        self.protection_thread = Some(handle);
    }

    /// Stop the memory protection system
    pub fn stop_protection(&mut self) {
        if let Some(handle) = self.protection_thread.take() {
            self.shutdown_flag.store(true, Ordering::Relaxed);

            // Give thread a moment to shutdown gracefully
            thread::sleep(Duration::from_millis(100));

            if !handle.is_finished() {
                handle.thread().unpark();
            }

            let _ = handle.join();
        }

        // Stop emergency protection
        {
            let mut emergency = self.emergency_protection.lock();
            emergency.stop_emergency_protection();
        }
    }

    /// Check if memory protection allows an operation
    pub fn can_allocate(&self, requested_bytes: usize) -> AllocationResult {
        let config = self.config.lock();
        let current_state = self.current_state.lock();

        // Check emergency protection first
        let emergency_protection = self.emergency_protection.lock();
        if !emergency_protection.can_emergency_allocate(requested_bytes) {
            return AllocationResult::Rejected(RejectionReason::EmergencyProtection);
        }
        drop(emergency_protection);

        // Check global limits first
        if config.global_memory_limit != usize::MAX {
            let current_memory = get_memory_stats().total_usage;
            if current_memory + requested_bytes > config.global_memory_limit {
                return AllocationResult::Rejected(RejectionReason::GlobalLimitExceeded);
            }
        }

        // Check circuit breakers
        if self.protection_circuit_breaker.is_open() {
            return AllocationResult::Rejected(RejectionReason::CircuitBreakerOpen);
        }

        // Check state-based restrictions
        match *current_state {
            MemoryProtectionState::Normal => AllocationResult::Allowed,
            MemoryProtectionState::Warning => {
                if requested_bytes > 1024 * 1024 {
                    // 1MB
                    AllocationResult::Rejected(RejectionReason::HighMemoryUsage)
                } else {
                    AllocationResult::Allowed
                }
            }
            MemoryProtectionState::Critical => {
                if requested_bytes > 512 * 1024 {
                    // 512KB
                    AllocationResult::Rejected(RejectionReason::HighMemoryUsage)
                } else {
                    AllocationResult::Allowed
                }
            }
            MemoryProtectionState::Emergency => {
                AllocationResult::Rejected(RejectionReason::EmergencyProtection)
            }
        }
    }

    /// Get current memory protection state
    pub fn get_state(&self) -> MemoryProtectionState {
        self.current_state.lock().clone()
    }

    /// Get global memory statistics
    pub fn get_global_stats(&self) -> GlobalMemoryStats {
        self.global_stats.lock().clone()
    }

    /// Manually trigger cleanup
    pub fn trigger_cleanup(&self) -> CleanupResult {
        let config = self.config.lock();

        if !config.enable_auto_cleanup {
            return CleanupResult::Disabled;
        }

        let mut cleanup_count = 0;
        let mut cleanup_success = true;

        // Clear caches
        if let Err(e) = clear_global_iri_cache() {
            cleanup_success = false;
            eprintln!("Failed to clear IRI cache: {}", e);
        } else {
            cleanup_count += 1;
        }

        if let Err(e) = crate::core::entities::clear_global_entity_cache() {
            cleanup_success = false;
            eprintln!("Failed to clear entity cache: {}", e);
        } else {
            cleanup_count += 1;
        }

        // Update statistics
        {
            let mut stats = self.global_stats.lock();
            stats.cleanup_count += cleanup_count;
            stats.last_cleanup_time = Some(Instant::now());
        }

        if cleanup_success {
            CleanupResult::Success(cleanup_count)
        } else {
            CleanupResult::PartialSuccess(cleanup_count)
        }
    }

    /// Trigger emergency protection measures
    fn trigger_emergency_protection(
        config: &MemoryProtectionConfig,
        stats: &Arc<Mutex<GlobalMemoryStats>>,
        circuit_breaker: &Arc<CircuitBreaker>,
    ) {
        println!("🚨 EMERGENCY MEMORY PROTECTION TRIGGERED");

        circuit_breaker.record_failure();

        if config.enable_graceful_degradation {
            // Emergency cleanup
            let _ = force_memory_cleanup();
            thread::sleep(Duration::from_millis(100));
        }

        // Update stats
        {
            let mut s = stats.lock();
            s.circuit_breaker_trips += 1;
        }

        // Log emergency state
        let current_memory = get_memory_stats().total_usage;
        let pressure = current_memory as f64 / config.global_memory_limit as f64;
        println!(
            "  Emergency protection: {:.1}% memory usage, {} bytes",
            pressure * 100.0,
            current_memory
        );
    }

    /// Trigger critical protection measures
    fn trigger_critical_protection(
        config: &MemoryProtectionConfig,
        stats: &Arc<Mutex<GlobalMemoryStats>>,
    ) {
        println!("⚠️  CRITICAL MEMORY PROTECTION TRIGGERED");

        if config.enable_auto_cleanup {
            // Aggressive cleanup
            let _ = force_memory_cleanup();
            thread::sleep(Duration::from_millis(50));
        }

        // Update stats
        {
            let mut s = stats.lock();
            s.cleanup_count += 1;
            s.last_cleanup_time = Some(Instant::now());
        }

        let current_memory = get_memory_stats().total_usage;
        let pressure = current_memory as f64 / config.global_memory_limit as f64;
        println!(
            "  Critical protection: {:.1}% memory usage, {} bytes",
            pressure * 100.0,
            current_memory
        );
    }

    /// Trigger warning protection measures
    fn trigger_warning_protection(
        config: &MemoryProtectionConfig,
        stats: &Arc<Mutex<GlobalMemoryStats>>,
    ) {
        println!("⚠️  WARNING: High memory usage detected");

        if config.enable_auto_cleanup {
            // Preventative cleanup
            let _ = force_memory_cleanup();
        }

        // Update stats
        {
            let mut s = stats.lock();
            s.cleanup_count += 1;
            s.last_cleanup_time = Some(Instant::now());
        }

        let current_memory = get_memory_stats().total_usage;
        let pressure = current_memory as f64 / config.global_memory_limit as f64;
        println!(
            "  Warning protection: {:.1}% memory usage, {} bytes",
            pressure * 100.0,
            current_memory
        );
    }
}

/// Result of allocation request
#[derive(Debug, Clone)]
pub enum AllocationResult {
    Allowed,
    Rejected(RejectionReason),
}

/// Reasons for allocation rejection
#[derive(Debug, Clone)]
pub enum RejectionReason {
    GlobalLimitExceeded,
    CircuitBreakerOpen,
    HighMemoryUsage,
    EmergencyProtection,
}

/// Result of cleanup operation
#[derive(Debug, Clone)]
pub enum CleanupResult {
    Success(usize),
    PartialSuccess(usize),
    Disabled,
    Failed(String),
}

/// Global memory protection instance
static GLOBAL_MEMORY_PROTECTION: std::sync::LazyLock<Mutex<MemoryProtection>> =
    std::sync::LazyLock::new(|| Mutex::new(MemoryProtection::new()));

/// Get global memory protection instance
pub fn get_memory_protection() -> parking_lot::MutexGuard<'static, MemoryProtection> {
    GLOBAL_MEMORY_PROTECTION.lock()
}

/// Initialize global memory protection with custom config
pub fn init_memory_protection(config: MemoryProtectionConfig) {
    let mut protection = GLOBAL_MEMORY_PROTECTION.lock();
    *protection = MemoryProtection::with_config(config);
}

/// Check if allocation is allowed under current memory conditions
pub fn can_allocate(requested_bytes: usize) -> AllocationResult {
    GLOBAL_MEMORY_PROTECTION
        .lock()
        .can_allocate(requested_bytes)
}

/// Get current memory protection state
pub fn get_memory_protection_state() -> MemoryProtectionState {
    GLOBAL_MEMORY_PROTECTION.lock().get_state()
}

/// Trigger manual cleanup
pub fn trigger_memory_cleanup() -> CleanupResult {
    GLOBAL_MEMORY_PROTECTION.lock().trigger_cleanup()
}

/// Get global memory statistics
pub fn get_global_memory_stats() -> GlobalMemoryStats {
    GLOBAL_MEMORY_PROTECTION.lock().get_global_stats()
}

/// Start memory protection monitoring
pub fn start_memory_protection() {
    let mut protection = GLOBAL_MEMORY_PROTECTION.lock();
    protection.start_protection();
}

/// Stop memory protection monitoring
pub fn stop_memory_protection() {
    let mut protection = GLOBAL_MEMORY_PROTECTION.lock();
    protection.stop_protection();
}
