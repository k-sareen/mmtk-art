use crate::{
    Art,
    MutatorClosure,
    UPCALLS,
};
use mmtk::{
    Mutator,
    MutatorContext,
    util::opaque_pointer::*,
    vm::{Collection, GCThreadContext},
};

/// Type of GC worker
#[repr(C)]
pub enum GcThreadKind {
    /// Simple GC Worker thread
    Worker = 0,
}

/// Implements collection trait
pub struct ArtCollection;

impl Collection<Art> for ArtCollection {
    fn stop_all_mutators<F>(tls: VMWorkerThread, _mutator_visitor: F)
    where
        F: FnMut(&'static mut Mutator<Art>),
    {
        // SAFETY: Assumes upcalls is valid
        unsafe { ((*UPCALLS).suspend_mutators)(tls) }

        // Flush remembered sets
        // XXX(kunals): Relevant MMTk issue to keep track of for flushing
        // mutator state: https://github.com/mmtk/mmtk-core/issues/1047
        // SAFETY: Assumes upcalls is valid
        unsafe {
            ((*UPCALLS).for_all_mutators)(MutatorClosure::from_rust_closure(&mut |mutator| {
                mutator.flush();
            }));
        }
    }

    fn resume_mutators(tls: VMWorkerThread) {
        // SAFETY: Assumes upcalls is valid
        unsafe { ((*UPCALLS).resume_mutators)(tls) }
    }

    fn block_for_gc(tls: VMMutatorThread) {
        // SAFETY: Assumes upcalls is valid
        unsafe { ((*UPCALLS).block_for_gc)(tls) }
    }

    fn spawn_gc_thread(tls: VMThread, ctx: GCThreadContext<Art>) {
        let (ctx_ptr, kind) = match ctx {
            GCThreadContext::Worker(w) => (
                Box::into_raw(w) as *mut libc::c_void,
                GcThreadKind::Worker,
            ),
        };
        // SAFETY: Assumes upcalls is valid
        unsafe {
            ((*UPCALLS).spawn_gc_thread)(tls, kind, ctx_ptr);
        }
    }
}
