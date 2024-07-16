use crate::{
    Art,
    MutatorClosure,
    UPCALLS,
};
use mmtk::{
    Mutator,
    util::{opaque_pointer::*, ObjectReference},
    plan::ObjectQueue,
    scheduler::GCWorker,
    vm::ActivePlan,
};
use std::{
    collections::VecDeque,
    marker::PhantomData,
};

/// Implements active GC plan trait
pub struct ArtActivePlan;

impl ActivePlan<Art> for ArtActivePlan {
    fn number_of_mutators() -> usize {
        // SAFETY: Assumes upcalls is valid
        unsafe { ((*UPCALLS).number_of_mutators)() }
    }

    fn is_mutator(tls: VMThread) -> bool {
        // SAFETY: Assumes upcalls is valid
        unsafe { ((*UPCALLS).is_mutator)(tls) }
    }

    fn mutator(tls: VMMutatorThread) -> &'static mut Mutator<Art> {
        // SAFETY: Assumes upcalls is valid
        unsafe {
            let m = ((*UPCALLS).get_mmtk_mutator)(tls);
            &mut *m
        }
    }

    fn mutators<'a>() -> Box<dyn Iterator<Item = &'a mut Mutator<Art>> + 'a> {
        Box::new(ArtMutatorIterator::new())
    }

    fn vm_trace_object<Q: ObjectQueue>(
        queue: &mut Q,
        object: ObjectReference,
        _worker: &mut GCWorker<Art>,
    ) -> ObjectReference {
        queue.enqueue(object);
        object
    }
}

/// An iterator that iterates through all active mutator threads
struct ArtMutatorIterator<'a> {
    mutators: VecDeque<&'a mut Mutator<Art>>,
    phantom_data: PhantomData<&'a ()>,
}

impl<'a> ArtMutatorIterator<'a> {
    fn new() -> Self {
        let mut mutators = VecDeque::new();
        // SAFETY: Assumes upcalls is valid
        unsafe {
            ((*UPCALLS).for_all_mutators)(MutatorClosure::from_rust_closure(&mut |mutator| {
                mutators.push_back(mutator);
            }));
        }
        Self {
            mutators,
            phantom_data: PhantomData,
        }
    }
}

impl<'a> Iterator for ArtMutatorIterator<'a> {
    type Item = &'a mut Mutator<Art>;

    fn next(&mut self) -> Option<Self::Item> {
        self.mutators.pop_front()
    }
}
