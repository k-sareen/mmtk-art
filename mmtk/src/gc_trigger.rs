//! Implements the ART GC trigger.

use crate::{unlikely, Art};
use mmtk::{
    util::{
        constants::LOG_BYTES_IN_PAGE,
        heap::{GCTriggerPolicy, SpaceStats},
    },
    Plan, MMTK,
};
use std::sync::atomic::{AtomicUsize, Ordering};

/// ART GC trigger initialization parameters.
#[derive(Debug, Default)]
pub(crate) struct ArtTriggerInit {
    /// Minimum free heap size (in bytes)
    pub min_free: usize,
    /// The ideal maximum free heap size (in bytes)
    pub max_free: usize,
    /// Initial heap size (in bytes)
    pub initial_size: usize,
    /// Max heap size (in bytes)
    pub capacity: usize,
    /// Size the heap is limited to (in bytes). growth limit <= capacity
    pub growth_limit: usize,
    /// Target ideal heap utilization
    pub target_utilization: f64,
    /// Heap growth multiplier when the application is in the foreground
    pub foreground_heap_growth_multiplier: f64,
}

/// The ART GC trigger.
pub struct ArtTrigger {
    /// Minimum free heap size (in pages)
    pub min_free: usize,
    /// The ideal maximum free heap size (in pages)
    pub max_free: usize,
    /// Max heap size (in pages)
    pub capacity: AtomicUsize,
    /// Size the heap is limited to (in pages). growth limit <= capacity
    pub growth_limit: AtomicUsize,
    /// Target ideal heap utilization
    pub target_utilization: f64,
    /// Target size for the heap (in pages). A GC is triggered when the heap size exceeds this value
    pub target_footprint: AtomicUsize,
    /// Number of pages that are pending allocation
    pub pending_allocation: AtomicUsize,
    /// Heap size before the GC started (in pages)
    pub reserved_pages_before_gc: AtomicUsize,
    /// Request a concurrent GC if heap size (in pages) exceeds this value
    pub concurrent_start_pages: AtomicUsize,
    /// Heap growth multiplier when the application is in the foreground
    pub foreground_heap_growth_multiplier: f64,
    /// Cached target footprint value when the application was in the foreground.
    /// We set target footprint to this value when process moves from background to foreground
    pub min_foreground_target_footprint: AtomicUsize,
    /// Cached concurrent start pages value when the application was in the foreground.
    /// We set the concurrent start pages to this value when process moves from background to foreground
    pub min_foreground_concurrent_start_pages: AtomicUsize,
}

impl ArtTrigger {
    /// Create a new ART GC trigger.
    pub fn new(
        min_free: usize,
        max_free: usize,
        initial_size: usize,
        capacity: usize,
        growth_limit: usize,
        target_utilization: f64,
        foreground_heap_growth_multiplier: f64,
    ) -> Self {
        Self {
            min_free: min_free >> LOG_BYTES_IN_PAGE,
            max_free: max_free >> LOG_BYTES_IN_PAGE,
            capacity: AtomicUsize::new(capacity >> LOG_BYTES_IN_PAGE),
            growth_limit: AtomicUsize::new(growth_limit >> LOG_BYTES_IN_PAGE),
            target_utilization,
            target_footprint: AtomicUsize::new(initial_size >> LOG_BYTES_IN_PAGE),
            pending_allocation: AtomicUsize::new(0),
            reserved_pages_before_gc: AtomicUsize::new(0),
            concurrent_start_pages: AtomicUsize::new(usize::MAX),
            foreground_heap_growth_multiplier,
            min_foreground_target_footprint: AtomicUsize::new(0),
            min_foreground_concurrent_start_pages: AtomicUsize::new(0),
        }
    }

    fn get_heap_growth_multiplier(&self, jank_perceptible: bool) -> f64 {
        if !jank_perceptible {
            1.0_f64
        } else {
            self.foreground_heap_growth_multiplier
        }
    }

    fn set_ideal_footprint(&self, mut target_footprint: usize) {
        if target_footprint > self.growth_limit.load(Ordering::Relaxed) {
            target_footprint = self.growth_limit.load(Ordering::Relaxed);
        }
        self.target_footprint
            .store(target_footprint, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    fn set_concurrent_start_pages(&self) {}

    /// Set the capacity to the growth limit, thereby decreasing the available heap space.
    fn clamp_growth_limit_inner(&self) {
        self.capacity
            .store(self.growth_limit.load(Ordering::Relaxed), Ordering::Relaxed);
    }

    /// Set the growth limit to the capacity, thereby increasing the available heap space.
    fn clear_growth_limit(&self) {
        let capacity = self.capacity.load(Ordering::Relaxed);
        let growth_limit = self.growth_limit.load(Ordering::Relaxed);
        if self.target_footprint.load(Ordering::Relaxed) == growth_limit && growth_limit < capacity
        {
            self.target_footprint.store(capacity, Ordering::Relaxed);
            self.set_concurrent_start_pages();
        }
        self.growth_limit.store(capacity, Ordering::Relaxed);
    }

    /// Increase heap size when the application transitions to a jank perceptible state.
    fn grow_heap_on_jank_perceptible_switch(&self, _mmtk: &'static MMTK<Art>) {
        let mut target_footprint = self.target_footprint.load(Ordering::Relaxed);
        if target_footprint < self.min_foreground_target_footprint.load(Ordering::Relaxed) {
            loop {
                match self.target_footprint.compare_exchange(
                    target_footprint,
                    self.min_foreground_target_footprint.load(Ordering::Relaxed),
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(current) => target_footprint = current,
                }
            }
        }

        // if mmtk.is_gc_concurrent() && self.concurrent_start_pages.load(Ordering::Relaxed)
        //     < self.min_foreground_concurrent_start_pages.load(Ordering::Relaxed)
        // {
        //     self.concurrent_start_pages.store(
        //         self.min_foreground_concurrent_start_pages.load(Ordering::Relaxed),
        //         Ordering::Relaxed,
        //     );
        // }
    }

    fn grow_for_utilization(&self, mmtk: &'static MMTK<Art>) {
        let mut grow_pages: usize;
        let mut target_size: usize;

        let reserved_pages = mmtk.get_plan().get_reserved_pages();
        crate::atrace_heap_size(reserved_pages);
        let multiplier = self.get_heap_growth_multiplier(mmtk.is_jank_perceptible());
        if crate::api::mmtk_is_nursery_collection() {
            let adjusted_max_free = (self.max_free as f64 * multiplier) as usize;
            if reserved_pages + adjusted_max_free < self.target_footprint.load(Ordering::Relaxed) {
                target_size = reserved_pages + adjusted_max_free;
                grow_pages = self.max_free;
            } else {
                target_size = std::cmp::max(
                    reserved_pages,
                    self.target_footprint.load(Ordering::Relaxed),
                );
                grow_pages = 0_usize;
            }
        } else {
            let delta = (reserved_pages as f64 * (1.0 / self.target_utilization - 1.0)) as u64;
            debug_assert!(delta as usize <= usize::MAX);
            grow_pages = std::cmp::min(delta as usize, self.max_free);
            grow_pages = std::cmp::max(grow_pages, self.min_free);
            target_size = reserved_pages + (grow_pages as f64 * multiplier) as usize;
        }

        if target_size - reserved_pages < self.pending_allocation.load(Ordering::Relaxed) {
            target_size += self.pending_allocation.load(Ordering::Relaxed);
        }

        self.set_ideal_footprint(target_size);
        let min_foreground_target_footprint = if multiplier <= 1.0 && grow_pages > 0 {
            std::cmp::min(
                reserved_pages
                    + (grow_pages as f64 * self.foreground_heap_growth_multiplier) as usize,
                self.growth_limit.load(Ordering::Relaxed),
            )
        } else {
            0_usize
        };
        self.min_foreground_target_footprint
            .store(min_foreground_target_footprint, Ordering::Relaxed);

        // if mmtk.is_gc_concurrent() {
        // }
    }
}

impl GCTriggerPolicy<Art> for ArtTrigger {
    fn on_pending_allocation(&self, pages: usize) {
        self.pending_allocation.fetch_add(pages, Ordering::Relaxed);
    }

    fn on_gc_start(&self, mmtk: &'static MMTK<Art>) {
        let reserved_pages = mmtk.get_plan().get_reserved_pages();
        self.reserved_pages_before_gc
            .store(reserved_pages, Ordering::Relaxed);
    }

    fn on_gc_end(&self, mmtk: &'static MMTK<Art>) {
        self.grow_for_utilization(mmtk);
        self.pending_allocation.store(0, Ordering::Relaxed);
        self.reserved_pages_before_gc.store(0, Ordering::Relaxed);
    }

    fn is_gc_required(
        &self,
        space_full: bool,
        space: Option<SpaceStats<Art>>,
        plan: &dyn Plan<VM = Art>,
    ) -> bool {
        let gc_required = loop {
            let old_target = self.target_footprint.load(Ordering::Relaxed);
            let reserved_pages = plan.get_reserved_pages();
            let pending_allocation = self.pending_allocation.load(Ordering::Relaxed);
            let new_target = reserved_pages + pending_allocation;
            if unlikely(new_target <= old_target) {
                break false;
            } else if unlikely(new_target > self.growth_limit.load(Ordering::Relaxed)) {
                break true;
            }

            // We are between target_footprint and growth_limit
            // if plan.is_concurrent() {
            //     return false;
            // }
            if self
                .target_footprint
                .compare_exchange_weak(old_target, new_target, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                self.pending_allocation
                    .fetch_sub(pending_allocation, Ordering::Relaxed);
                break false;
            }
        };

        gc_required || plan.collection_required(space_full, space)
    }

    fn is_heap_full(&self, plan: &dyn Plan<VM = Art>) -> bool {
        let reserved_pages = plan.get_reserved_pages();
        let target_footprint = self.target_footprint.load(Ordering::Relaxed);
        reserved_pages >= target_footprint
    }

    fn get_current_heap_size_in_pages(&self) -> usize {
        self.target_footprint.load(Ordering::Relaxed)
    }

    fn get_max_heap_size_in_pages(&self) -> usize {
        self.growth_limit.load(Ordering::Relaxed)
    }

    fn can_heap_size_grow(&self) -> bool {
        self.target_footprint.load(Ordering::Relaxed) < self.growth_limit.load(Ordering::Relaxed)
    }

    fn clamp_max_heap_size(&mut self, max_heap_size: usize) -> bool {
        let max_heap_pages = max_heap_size >> LOG_BYTES_IN_PAGE;
        self.capacity.store(max_heap_pages, Ordering::Relaxed);
        self.growth_limit.store(max_heap_pages, Ordering::Relaxed);
        self.set_ideal_footprint(max_heap_pages);
        self.set_concurrent_start_pages();
        true
    }

    fn grow_heap_size_for_event(&self, mmtk: &'static MMTK<Art>) {
        self.grow_heap_on_jank_perceptible_switch(mmtk)
    }

    fn clamp_growth_limit(&self) {
        self.clamp_growth_limit_inner();
    }

    fn clear_growth_limit(&self) {
        self.clear_growth_limit();
    }
}
