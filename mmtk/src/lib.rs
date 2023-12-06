//! This crate interfaces with the MMTk framework to provide memory management capabilities.

#[macro_use]
extern crate lazy_static;

use crate::{
    active_plan::ArtActivePlan,
    collection::{
        ArtCollection,
        GcThreadKind,
    },
    object_model::ArtObjectModel,
    reference_glue::ArtReferenceGlue,
    scanning::ArtScanning,
};
use mmtk::{
    MMTK,
    MMTKBuilder,
    Mutator,
    util::{
        Address,
        ObjectReference,
        conversions::{
            chunk_align_down,
            chunk_align_up,
        },
        heap::vm_layout::{
            BYTES_IN_CHUNK,
            VMLayout,
        },
        opaque_pointer::*,
    },
    vm::{
        edge_shape::{
            Edge,
            MemorySlice,
        },
        VMBinding,
    },
};
use std::{
    ops::Range,
    ptr::null_mut,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    }
};

/// log_2 of how many bytes in an integer
const LOG_BYTES_IN_INT:  u8 = 2;
/// log_2 of how many bytes in an edge
const LOG_BYTES_IN_EDGE: u8 = 2;

/// Module which implements `ActivePlan` trait
pub mod active_plan;
/// Module which implements the user-facing API
pub mod api;
/// Module which implements `Collection` trait
pub mod collection;
/// Module which implements `ObjectModel` trait
pub mod object_model;
/// Module which implements `ReferenceGlue` trait
pub mod reference_glue;
/// Module which implements `Scanning` trait
pub mod scanning;

/// VMBinding to ART
#[derive(Default)]
pub struct Art;

#[repr(C)]
/// What weak reference phase are we currently executing?
pub enum RefProcessingPhase {
    /// Forward soft references if we are not going to clear them
    Phase1 = 0,
    /// Clear soft and weak references with unmarked referents. Enqueue
    /// finalizer references and perform transitive closure
    Phase2 = 1,
    /// Clear soft and weak references where the references are only reachable
    /// by finalizers. Clear all phantom references with unmarked referents. And
    /// finally sweep system weaks.
    Phase3 = 2,
}

/// Has MMTk been initialized?
pub static IS_MMTK_INITIALIZED: AtomicBool = AtomicBool::new(false);

lazy_static! {
    /// MMTk builder instance
    pub static ref BUILDER: Mutex<MMTKBuilder> = Mutex::new(MMTKBuilder::new());
    /// The actual MMTk instance
    pub static ref SINGLETON: MMTK<Art> = {
        let mut builder = BUILDER.lock().unwrap();
        assert!(!IS_MMTK_INITIALIZED.load(Ordering::Relaxed));
        set_vm_layout(&mut builder);
        let ret = mmtk::memory_manager::mmtk_init(&builder);
        IS_MMTK_INITIALIZED.store(true, Ordering::SeqCst);
        *ret
    };
}

fn compress(o: ObjectReference) -> u32 {
    if o.is_null() {
        0u32
    } else {
        o.to_raw_address().as_usize() as u32
    }
}

fn decompress(v: u32) -> ObjectReference {
    if v == 0 {
        ObjectReference::NULL
    } else {
        // We mask the lowest-order bits in case, due to concurrency, a slot
        // (which happens to be an object) is enqueued while someone is trying
        // to forward the same object. This could result in `slot.load()`
        // returning the forwarding bits, hence messing up the transitive closure
        unsafe {
            ObjectReference::from_raw_address(
                Address::from_usize((v as usize) & !0b11)
            )
        }
    }
}

fn set_vm_layout(builder: &mut MMTKBuilder) {
    let max_heap_size = builder.options.gc_trigger.max_heap_size();
    // Android doesn't really allow setting heap sizes > 800 MB on the
    // command-line (so actually > 1600 MB as they double the command-line
    // argument for the actual heap size)
    assert!(
        max_heap_size <= (2usize << 30),
        "Heap size is larger than 2 GB!"
    );

    let start = 0x1000_0000;
    let end = start + max_heap_size;

    let heap_start = chunk_align_down(unsafe { Address::from_usize(start) });
    // XXX(kunals): We have to add some slack to the heap end here as otherwise
    // MMTk will fail to allocate pages for the collection reserve.
    // See discussion here:
    // https://mmtk.zulipchat.com/#narrow/stream/262673-mmtk-core/topic/Immix.20defragmentation.20GC.20when.20space.20is.20full
    // for more information
    let heap_end = chunk_align_up(unsafe { Address::from_usize(end) } + 5 * BYTES_IN_CHUNK);
    let constants = VMLayout {
        log_address_space: 32,
        heap_start,
        heap_end,
        log_space_extent: 31,
        force_use_contiguous_spaces: false,
    };

    builder.set_vm_layout(constants);
}

/// The type of edge in ART. ART edges only operate on 32-bits
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(transparent)]
pub struct ArtEdge(pub Address);

impl Edge for ArtEdge {
    fn load(&self) -> ObjectReference {
        decompress(unsafe { self.0.load::<u32>() })
    }

    fn store(&self, object: ObjectReference) {
        unsafe { self.0.store::<u32>(compress(object)) }
    }
}

/// A range of `ArtEdge`s, usually used to represent arrays
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ArtMemorySlice {
    range: Range<ArtEdge>,
}

impl From<Range<Address>> for ArtMemorySlice {
    fn from(value: Range<Address>) -> Self {
        Self {
            range: Range {
                start: ArtEdge(value.start),
                end: ArtEdge(value.end),
            },
        }
    }
}

/// An iterator through an ArtMemorySlice
pub struct ArtEdgeIterator {
    cursor: Address,
    limit: Address,
}

impl Iterator for ArtEdgeIterator {
    type Item = ArtEdge;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor >= self.limit {
            None
        } else {
            let edge = self.cursor;
            self.cursor += (1 << LOG_BYTES_IN_EDGE) as usize;
            Some(ArtEdge(edge))
        }
    }
}

impl MemorySlice for ArtMemorySlice {
    type Edge = ArtEdge;
    type EdgeIterator = ArtEdgeIterator;

    fn iter_edges(&self) -> Self::EdgeIterator {
        ArtEdgeIterator {
            cursor: self.range.start.0,
            limit: self.range.end.0,
        }
    }

    fn object(&self) -> Option<ObjectReference> {
        None
    }

    fn start(&self) -> Address {
        self.range.start.0
    }

    fn bytes(&self) -> usize {
        self.range.end.0 - self.range.start.0
    }

    fn copy(src: &Self, tgt: &Self) {
        debug_assert_eq!(src.bytes(), tgt.bytes());
        // Raw memory copy
        debug_assert_eq!(
            src.bytes() & ((1 << LOG_BYTES_IN_INT) - 1),
            0,
            "bytes are not a multiple of 32-bit integers"
        );
        unsafe {
            let words = tgt.bytes() >> LOG_BYTES_IN_INT;
            let src = src.start().to_ptr::<u32>();
            let tgt = tgt.start().to_mut_ptr::<u32>();
            std::ptr::copy(src, tgt, words)
        }
    }
}

impl VMBinding for Art {
    type VMActivePlan = ArtActivePlan;
    type VMCollection = ArtCollection;
    type VMObjectModel = ArtObjectModel;
    type VMReferenceGlue = ArtReferenceGlue;
    type VMScanning = ArtScanning;

    type VMEdge = ArtEdge;
    type VMMemorySlice = ArtMemorySlice;

    const MIN_ALIGNMENT: usize = 8;
    const MAX_ALIGNMENT: usize = 8;
    const ALLOC_END_ALIGNMENT: usize = 8;
    const USE_ALLOCATION_OFFSET: bool = false;
}

#[repr(C)]
/// Upcalls from MMTk to ART
pub struct ArtUpcalls {
    /// Get the size of the given object
    pub size_of: extern "C" fn(object: ObjectReference) -> usize,
    /// Scan object for references
    pub scan_object: extern "C" fn(
        object: ObjectReference,
        closure: ScanObjectClosure,
    ),
    /// Block mutator thread for GC
    pub block_for_gc: extern "C" fn(tls: VMMutatorThread),
    /// Spawn GC thread with type `kind`
    pub spawn_gc_thread: extern "C" fn(
        tls: VMThread,
        kind: GcThreadKind,
        ctx: *mut libc::c_void
    ),
    /// Stop all mutator threads
    pub stop_all_mutators: extern "C" fn(),
    /// Resume all mutator threads
    pub resume_mutators: extern "C" fn(tls: VMWorkerThread),
    /// Return the number of mutators
    pub number_of_mutators: extern "C" fn() -> usize,
    /// Return if given thread `tls` is a mutator thread
    pub is_mutator: extern "C" fn(tls: VMThread) -> bool,
    /// Return the `Mutator` instance associated with thread `tls`
    pub get_mmtk_mutator: extern "C" fn(tls: VMMutatorThread) -> *mut Mutator<Art>,
    /// Evaluate given closure for each mutator thread
    pub for_all_mutators: extern "C" fn(closure: MutatorClosure),
    /// Scan all VM roots and report nodes to MMTk
    pub scan_all_roots: extern "C" fn(closure: NodesClosure),
    /// Process weak references
    pub process_references: extern "C" fn(
        tls: VMWorkerThread,
        closure: TraceObjectClosure,
        phase: RefProcessingPhase,
        clear_soft_references: bool,
    ),
    /// Sweep system weaks in ART
    pub sweep_system_weaks: extern "C" fn(),
}

/// Global static instance of upcalls
pub static mut UPCALLS: *const ArtUpcalls = null_mut();

/// A closure iterating on mutators. The C++ code should pass `data` back as the
/// last argument.
#[repr(C)]
pub struct MutatorClosure {
    /// The closure to invoke
    pub func: extern "C" fn(mutator: *mut Mutator<Art>, data: *mut libc::c_void),
    /// The Rust context associated with the closure
    pub data: *mut libc::c_void,
}

impl MutatorClosure {
    fn from_rust_closure<F>(callback: &mut F) -> Self
    where
        F: FnMut(&'static mut Mutator<Art>),
    {
        Self {
            func: Self::call_rust_closure::<F>,
            data: callback as *mut F as *mut libc::c_void,
        }
    }

    extern "C" fn call_rust_closure<F>(
        mutator: *mut Mutator<Art>,
        callback_ptr: *mut libc::c_void,
    ) where
        F: FnMut(&'static mut Mutator<Art>),
    {
        let callback: &mut F = unsafe { &mut *(callback_ptr as *mut F) };
        callback(unsafe { &mut *mutator });
    }
}

/// A representation of a Rust buffer
#[repr(C)]
pub struct RustBuffer {
    /// The start address of the buffer
    pub ptr: *mut Address,
    /// The capacity of the buffer
    pub capacity: usize,
}

/// A closure for reporting root nodes. The C++ code should pass `data` back as the last argument.
#[repr(C)]
pub struct NodesClosure {
    /// The closure to invoke
    pub func: extern "C" fn(
        buf: *mut Address,
        size: usize,
        cap: usize,
        data: *mut libc::c_void,
    ) -> RustBuffer,
    /// The Rust context associated with the closure
    pub data: *const libc::c_void,
}

/// A closure for scanning an object for references. The C++ code should pass
/// `data` back as the last argument.
#[repr(C)]
pub struct ScanObjectClosure {
    /// The closure to invoke
    pub func: extern "C" fn(
        edge: ArtEdge,
        data: *mut libc::c_void,
    ),
    /// The Rust context associated with the closure
    pub data: *const libc::c_void,
}

/// A closure for tracing an object. The C++ code should pass `data` back as the
/// last argument.
#[repr(C)]
pub struct TraceObjectClosure {
    /// The closure to invoke
    pub func: extern "C" fn(
        object: ObjectReference,
        data: *mut libc::c_void,
    ) -> ObjectReference,
    /// The Rust context associated with the closure
    pub data: *const libc::c_void,
}

impl TraceObjectClosure {
    fn from_rust_closure<F>(callback: &mut F) -> Self
    where
        F: FnMut(ObjectReference) -> ObjectReference,
    {
        Self {
            func: Self::call_rust_closure::<F>,
            data: callback as *mut F as *mut libc::c_void,
        }
    }

    extern "C" fn call_rust_closure<F>(
        object: ObjectReference,
        callback_ptr: *mut libc::c_void,
    ) -> ObjectReference
    where
        F: FnMut(ObjectReference) -> ObjectReference,
    {
        let callback: &mut F = unsafe { &mut *(callback_ptr as *mut F) };
        callback(object)
    }
}
