//! Memory profiling utilities
//!
//! Provides tools for tracking memory usage during ontology loading
//! and reasoning operations.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Global memory tracker
pub static GLOBAL_ALLOCATOR: MemoryTracker<System> = MemoryTracker::new(System);

/// Memory-tracking allocator wrapper
pub struct MemoryTracker<A: GlobalAlloc> {
    allocator: A,
    allocated_bytes: AtomicUsize,
    deallocated_bytes: AtomicUsize,
}

impl<A: GlobalAlloc> MemoryTracker<A> {
    /// Create new memory tracker
    pub const fn new(allocator: A) -> Self {
        Self {
            allocator,
            allocated_bytes: AtomicUsize::new(0),
            deallocated_bytes: AtomicUsize::new(0),
        }
    }
    
    /// Get total allocated bytes
    pub fn allocated_bytes(&self) -> usize {
        self.allocated_bytes.load(Ordering::Relaxed)
    }
    
    /// Get total deallocated bytes
    pub fn deallocated_bytes(&self) -> usize {
        self.deallocated_bytes.load(Ordering::Relaxed)
    }
    
    /// Get current memory usage (allocated - deallocated)
    pub fn current_usage(&self) -> usize {
        self.allocated_bytes() - self.deallocated_bytes()
    }
    
    /// Reset counters
    pub fn reset(&self) {
        self.allocated_bytes.store(0, Ordering::Relaxed);
        self.deallocated_bytes.store(0, Ordering::Relaxed);
    }
}

unsafe impl<A: GlobalAlloc> GlobalAlloc for MemoryTracker<A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.allocated_bytes.fetch_add(layout.size(), Ordering::Relaxed);
        self.allocator.alloc(layout)
    }
    
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.deallocated_bytes.fetch_add(layout.size(), Ordering::Relaxed);
        self.allocator.dealloc(ptr, layout)
    }
    
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        self.deallocated_bytes.fetch_add(layout.size(), Ordering::Relaxed);
        self.allocated_bytes.fetch_add(new_size, Ordering::Relaxed);
        self.allocator.realloc(ptr, layout, new_size)
    }
}

/// Memory profiler for tracking usage during operations
#[derive(Debug, Clone)]
pub struct MemoryProfiler {
    start_usage: usize,
    checkpoints: Vec<(String, usize)>,
}

impl MemoryProfiler {
    /// Create new profiler and record starting memory
    pub fn new() -> Self {
        Self {
            start_usage: GLOBAL_ALLOCATOR.current_usage(),
            checkpoints: Vec::new(),
        }
    }
    
    /// Record a checkpoint with a label
    pub fn checkpoint(&mut self, label: &str) {
        let usage = GLOBAL_ALLOCATOR.current_usage();
        self.checkpoints.push((label.to_string(), usage));
    }
    
    /// Get memory delta since start
    pub fn delta(&self) -> isize {
        let current = GLOBAL_ALLOCATOR.current_usage();
        current as isize - self.start_usage as isize
    }
    
    /// Generate report
    pub fn report(&self) -> String {
        let mut report = format!("Memory Profile Report:\n");
        report.push_str(&format!("  Start: {} bytes\n", self.start_usage));
        
        let mut last = self.start_usage;
        for (label, usage) in &self.checkpoints {
            let delta = *usage as isize - last as isize;
            report.push_str(&format!("  {}: {} bytes (Δ {} bytes)\n", 
                label, usage, delta));
            last = *usage;
        }
        
        let total_delta = self.delta();
        report.push_str(&format!("  Total: {} bytes (Δ {} bytes)\n",
            GLOBAL_ALLOCATOR.current_usage(), total_delta));
        
        report
    }
}

impl Default for MemoryProfiler {
    fn default() -> Self {
        Self::new()
    }
}

/// Get current resident memory (RSS) in bytes
#[cfg(target_os = "linux")]
pub fn get_resident_memory() -> usize {
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    
    let file = match File::open("/proc/self/status") {
        Ok(f) => f,
        Err(_) => return 0,
    };
    
    let reader = BufReader::new(file);
    for line in reader.lines() {
        if let Ok(line) = line {
            if line.starts_with("VmRSS:") {
                // Parse "VmRSS:    12345 kB"
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(kb) = parts[1].parse::<usize>() {
                        return kb * 1024; // Convert KB to bytes
                    }
                }
            }
        }
    }
    
    0
}

#[cfg(not(target_os = "linux"))]
pub fn get_resident_memory() -> usize {
    // Fallback for non-Linux platforms
    GLOBAL_ALLOCATOR.current_usage()
}

/// Memory-efficient IRI cache configuration for large ontologies
pub fn configure_iri_cache_for_large_ontology(expected_classes: usize) {
    use crate::core::iri::set_global_iri_cache_limit;
    
    // For large ontologies, we need a bigger cache
    // Cache size = 2x expected classes to handle properties + individuals
    let cache_size = (expected_classes * 2).max(10_000).min(1_000_000);
    
    set_global_iri_cache_limit(cache_size);
    
    println!("Configured IRI cache for {} classes (cache size: {})", 
        expected_classes, cache_size);
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_memory_profiler() {
        let mut profiler = MemoryProfiler::new();
        
        // Allocate some memory
        let _vec: Vec<u8> = vec![0; 1024];
        profiler.checkpoint("after allocation");
        
        // Check that we recorded something
        assert!(!profiler.checkpoints.is_empty());
    }
}
