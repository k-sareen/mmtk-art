use crate::{Art, UPCALLS};
use mmtk::{
    util::{Address, ObjectReference, copy::*},
    vm::*,
};

/// Implements object model trait
pub struct ArtObjectModel;

impl ObjectModel<Art> for ArtObjectModel {
    const GLOBAL_LOG_BIT_SPEC: VMGlobalLogBitSpec = VMGlobalLogBitSpec::side_first();
    const LOCAL_FORWARDING_POINTER_SPEC: VMLocalForwardingPointerSpec = VMLocalForwardingPointerSpec::in_header(32);
    const LOCAL_FORWARDING_BITS_SPEC: VMLocalForwardingBitsSpec = VMLocalForwardingBitsSpec::in_header(0);
    const LOCAL_LOS_MARK_NURSERY_SPEC: VMLocalLOSMarkNurserySpec = VMLocalLOSMarkNurserySpec::side_first();
    const LOCAL_MARK_BIT_SPEC: VMLocalMarkBitSpec = VMLocalMarkBitSpec::side_after(Self::LOCAL_LOS_MARK_NURSERY_SPEC.as_spec());
    const LOCAL_PINNING_BIT_SPEC: VMLocalPinningBitSpec = VMLocalPinningBitSpec::side_after(Self::LOCAL_MARK_BIT_SPEC.as_spec());

    const UNIFIED_OBJECT_REFERENCE_ADDRESS: bool = true;
    const OBJECT_REF_OFFSET_LOWER_BOUND: isize = 0;
    const IN_OBJECT_ADDRESS_OFFSET: isize = 0;

    fn copy(
        from: ObjectReference,
        semantics: CopySemantics,
        copy_context: &mut GCWorkerCopyContext<Art>,
    ) -> ObjectReference {
        let bytes = Self::get_size_when_copied(from);
        let dst = copy_context.alloc_copy(from, bytes, Art::MIN_ALIGNMENT, 0, semantics);
        let src = from.to_raw_address();
        // SAFETY: Assumes src and dst are valid
        unsafe { std::ptr::copy_nonoverlapping::<u8>(src.to_ptr(), dst.to_mut_ptr(), bytes) }
        // SAFETY: dst is valid from above
        let to_obj = unsafe { ObjectReference::from_raw_address_unchecked(dst) };
        copy_context.post_copy(to_obj, bytes, semantics);
        to_obj
    }

    fn copy_to(_from: ObjectReference, _to: ObjectReference, _region: Address) -> Address {
        unimplemented!()
    }

    fn get_current_size(object: ObjectReference) -> usize {
        use mmtk::util::conversions;
        // SAFETY: Assumes upcalls is valid
        conversions::raw_align_up(crate::abi::get_object_size(object), Art::MIN_ALIGNMENT)
    }

    fn get_size_when_copied(object: ObjectReference) -> usize {
        Self::get_current_size(object)
    }

    fn get_align_when_copied(_object: ObjectReference) -> usize {
        Art::MIN_ALIGNMENT
    }

    fn get_align_offset_when_copied(_object: ObjectReference) -> usize {
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

    fn dump_object(_object: ObjectReference) {
        unimplemented!()
    }

    fn is_object_sane(object: ObjectReference) -> bool {
        // SAFETY: Assumes upcalls is valid
        unsafe {
            ((*UPCALLS).is_valid_object)(object)
        }
    }
}
