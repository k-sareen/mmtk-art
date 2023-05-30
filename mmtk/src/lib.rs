//! This crate interfaces with the MMTk framework to provide memory management capabilities.
#[macro_use]
extern crate lazy_static;

use {
    std::{
        ops::Range,
        sync::{
            Mutex,
            atomic::{AtomicBool, Ordering},
        }
    },
    mmtk::{
        AllocationSemantics,
        MMTK,
        MMTKBuilder,
        Mutator,
        MutatorContext,
        Plan,
        util::{
            Address,
            ObjectReference,
            conversions,
            copy::*,
            opaque_pointer::*,
        },
        vm::*,
    },
};

/// VMBinding to ART
#[derive(Default)]
pub struct Art;

/// Has MMTk been initialized?
pub static IS_MMTK_INITIALIZED: AtomicBool = AtomicBool::new(false);

lazy_static! {
    /// MMTk builder instance
    pub static ref BUILDER: Mutex<MMTKBuilder> = Mutex::new(MMTKBuilder::new());
    /// The actual MMTk instance
    pub static ref SINGLETON: MMTK<Art> = {
        let builder = BUILDER.lock().unwrap();
        assert!(!IS_MMTK_INITIALIZED.load(Ordering::Relaxed));
        let ret = mmtk::memory_manager::mmtk_init(&builder);
        IS_MMTK_INITIALIZED.store(true, Ordering::SeqCst);
        *ret
    };
}

/// The type of edge in ART
type ArtEdge = Address;
/// Implements active GC plan trait
pub struct ArtActivePlan;
/// Implements collection trait
pub struct ArtCollection;
/// Implements object model trait
pub struct ArtObjectModel;
/// Implements reference glue trait
pub struct ArtReferenceGlue;
/// Implements scanning trait
pub struct ArtScanning;

impl VMBinding for Art {
    type VMActivePlan = ArtActivePlan;
    type VMCollection = ArtCollection;
    type VMObjectModel = ArtObjectModel;
    type VMReferenceGlue = ArtReferenceGlue;
    type VMScanning = ArtScanning;

    type VMEdge = ArtEdge;
    type VMMemorySlice = Range<Address>;

    const MIN_ALIGNMENT: usize = 8;
    const MAX_ALIGNMENT: usize = 8;
    const ALLOC_END_ALIGNMENT: usize = 8;
    const USE_ALLOCATION_OFFSET: bool = false;
}

impl ActivePlan<Art> for ArtActivePlan {
	fn global() -> &'static dyn Plan<VM=Art> {
        SINGLETON.get_plan()
    }

    fn number_of_mutators() -> usize {
        unimplemented!()
    }

    fn is_mutator(_tls: VMThread) -> bool {
        true
    }

    fn mutator(_tls: VMMutatorThread) -> &'static mut Mutator<Art> {
        unimplemented!()
    }

    fn reset_mutator_iterator() {
        unimplemented!()
    }

    fn get_next_mutator() -> Option<&'static mut Mutator<Art>> {
        unimplemented!()
    }
}

impl Collection<Art> for ArtCollection {
	fn stop_all_mutators<F>(_tls: VMWorkerThread, _mutator_visitor: F)
    where
        F: FnMut(&'static mut Mutator<Art>),
    {
        unimplemented!()
    }

    fn resume_mutators(_tls: VMWorkerThread) {
        unimplemented!()
    }

    fn block_for_gc(_tls: VMMutatorThread) {
		unimplemented!()
    }

    fn spawn_gc_thread(_tls: VMThread, _ctx: GCThreadContext<Art>) {}

    fn prepare_mutator<T: MutatorContext<Art>>(
        _tls_w: VMWorkerThread,
        _tls_m: VMMutatorThread,
        _mutator: &T,
    ) {
        unimplemented!()
    }
}

impl ObjectModel<Art> for ArtObjectModel {
	const GLOBAL_LOG_BIT_SPEC: VMGlobalLogBitSpec = VMGlobalLogBitSpec::in_header(0);
    const LOCAL_FORWARDING_POINTER_SPEC: VMLocalForwardingPointerSpec = VMLocalForwardingPointerSpec::in_header(0);
    const LOCAL_FORWARDING_BITS_SPEC: VMLocalForwardingBitsSpec = VMLocalForwardingBitsSpec::in_header(0);
    const LOCAL_MARK_BIT_SPEC: VMLocalMarkBitSpec = VMLocalMarkBitSpec::in_header(0);
    const LOCAL_LOS_MARK_NURSERY_SPEC: VMLocalLOSMarkNurserySpec = VMLocalLOSMarkNurserySpec::in_header(0);

	const UNIFIED_OBJECT_REFERENCE_ADDRESS: bool = true;
    const OBJECT_REF_OFFSET_LOWER_BOUND: isize = 0;

    fn copy(
        _from: ObjectReference,
        _semantics: CopySemantics,
        _copy_context: &mut GCWorkerCopyContext<Art>,
    ) -> ObjectReference {
        unimplemented!()
    }

    fn copy_to(_from: ObjectReference, _to: ObjectReference, _region: Address) -> Address {
        unimplemented!()
    }

    fn get_current_size(_object: ObjectReference) -> usize {
        unimplemented!()
    }

    fn get_size_when_copied(_object: ObjectReference) -> usize {
        0
    }

    fn get_align_when_copied(_object: ObjectReference) -> usize {
        0
    }

    fn get_align_offset_when_copied(_object: ObjectReference) -> isize {
        0
    }

    fn get_reference_when_copied_to(_from: ObjectReference, _to: Address) -> ObjectReference {
        unimplemented!()
    }

    fn get_type_descriptor(_reference: ObjectReference) -> &'static [i8] {
        unimplemented!()
    }

    fn ref_to_object_start(object: ObjectReference) -> Address {
        object.to_raw_address()
    }

    fn ref_to_header(object: ObjectReference) -> Address {
        object.to_raw_address()
    }

    fn ref_to_address(object: ObjectReference) -> Address {
        Self::ref_to_object_start(object)
    }

    fn address_to_ref(addr: Address) -> ObjectReference {
        ObjectReference::from_raw_address(addr)
    }

    fn dump_object(_object: ObjectReference) {
        unimplemented!()
    }
}

impl ReferenceGlue<Art> for ArtReferenceGlue {
    type FinalizableType = ObjectReference;

    fn set_referent(_reference: ObjectReference, _referent: ObjectReference) {
        unimplemented!()
    }

    fn get_referent(_object: ObjectReference) -> ObjectReference {
        unimplemented!()
    }

    fn enqueue_references(_references: &[ObjectReference], _tls: VMWorkerThread) {
        unimplemented!()
    }
}

impl Scanning<Art> for ArtScanning {
    fn scan_thread_roots(_tls: VMWorkerThread, _factory: impl RootsWorkFactory<ArtEdge>) {
        unimplemented!()
    }

    fn scan_thread_root(
        _tls: VMWorkerThread,
        _mutator: &'static mut Mutator<Art>,
        _factory: impl RootsWorkFactory<ArtEdge>,
    ) {
        unimplemented!()
    }

    fn scan_vm_specific_roots(_tls: VMWorkerThread, _factory: impl RootsWorkFactory<ArtEdge>) {
        unimplemented!()
    }

    fn scan_object<EV: EdgeVisitor<ArtEdge>>(
        _tls: VMWorkerThread,
        _object: ObjectReference,
        _edge_visitor: &mut EV,
    ) {
        unimplemented!()
    }

    fn notify_initial_thread_scan_complete(_partial_scan: bool, _tls: VMWorkerThread) {
        unimplemented!()
    }
    
    fn supports_return_barrier() -> bool {
        unimplemented!()
    }

    fn prepare_for_roots_re_scanning() {
        unimplemented!()
    }
}

/// Initialize MMTk instance
#[no_mangle]
pub extern "C" fn mmtk_init() {
    // Make sure that we haven't initialized MMTk (by accident) yet
    assert!(!crate::IS_MMTK_INITIALIZED.load(Ordering::SeqCst));
    // Make sure we initialize MMTk here
    lazy_static::initialize(&SINGLETON)
}

/// Bind a mutator thread in MMTk
#[no_mangle]
pub extern "C" fn mmtk_bind_mutator(tls: VMMutatorThread) -> *mut Mutator<Art> {
    Box::into_raw(mmtk::memory_manager::bind_mutator(&SINGLETON, tls))
}

/// Allocate an object of `size` using MMTk
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn mmtk_alloc(
    mutator: *mut Mutator<Art>,
    size: usize,
    align: usize,
    offset: isize,
    allocator: AllocationSemantics,
) -> Address {
    // We need to do this as a workaround to a bug in mmtk-core: https://github.com/mmtk/mmtk-core/issues/730
    let aligned_size = conversions::raw_align_up(size, align);
    mmtk::memory_manager::alloc::<Art>(
        unsafe { &mut *mutator },
        aligned_size,
        align,
        offset,
        allocator
    )
}
