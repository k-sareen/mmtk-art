use crate::{ArtObjectModel, ArtSlot};
use memoffset::offset_of;
use mmtk::{
    util::{
        conversions::{raw_align_up, raw_is_aligned},
        Address, ObjectReference, OpaquePointer,
    },
    vm::*,
};
use std::{
    marker::PhantomData,
    num::NonZeroU32,
    ops::Deref,
    sync::atomic::{AtomicI32, Ordering},
};

/// Size of object reference (in bytes)
const PRIM_OBJECT_REFERENCE_SIZE: u32 = 4;

/// Size of object header (in bytes)
pub const OBJECT_HEADER_SIZE: u32 = 8;

/// Size of an ArtHeapReference (in bytes)
pub const ART_HEAP_REFERENCE_SIZE: usize = std::mem::size_of::<ArtHeapReference<Object>>();

/// Get the slot of a field in an object.
pub fn get_field_slot<T>(o: &T, offset: i32) -> ArtSlot {
    ArtSlot(get_field_address(o, offset))
}

/// Get the address of a field in an object.
fn get_field_address<T>(o: &T, offset: i32) -> Address {
    Address::from_ref(o) + offset as isize
}

/// References between objects within the managed heap.
#[repr(C)]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct ArtHeapReference<T> {
    reference: Option<NonZeroU32>,
    phantom: PhantomData<T>,
}

impl<T> Deref for ArtHeapReference<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        debug_assert_ne!(self.reference, None);
        debug_assert!(ArtObjectModel::is_object_sane(self.into()));
        // SAFETY: self.reference is valid from above.
        unsafe { &*(self.reference.unwrap().get() as *const T) }
    }
}

impl<T> From<ObjectReference> for &ArtHeapReference<T> {
    fn from(o: ObjectReference) -> Self {
        let ref_usize = o.to_raw_address().as_usize();
        debug_assert!(ref_usize <= u32::MAX as usize);
        // SAFETY: o is a valid ObjectReference and within the u32 range.
        unsafe { std::mem::transmute(ref_usize) }
    }
}

impl<T> From<&ArtHeapReference<T>> for ObjectReference {
    fn from(o: &ArtHeapReference<T>) -> Self {
        debug_assert_ne!(o.reference, None);
        // SAFETY: o is a valid ObjectReference and within the u32 range.
        unsafe { std::mem::transmute(o.reference.unwrap().get() as usize) }
    }
}

/// Rust mirror of java.lang.Object
#[repr(C)]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct Object {
    /// The Class representing the type of the object.
    pub klass: ArtHeapReference<Class>,
    /// Monitor and hash code information.
    pub monitor: u32,
}

impl From<ObjectReference> for &Object {
    fn from(o: ObjectReference) -> Self {
        debug_assert!(ArtObjectModel::is_object_sane(o));
        // SAFETY: o is a valid ObjectReference.
        unsafe { std::mem::transmute(o) }
    }
}

impl From<&Object> for ObjectReference {
    fn from(o: &Object) -> Self {
        // SAFETY: o is a valid Rust reference.
        unsafe { std::mem::transmute(o) }
    }
}

impl Object {
    /// Get the offset of the klass slot of an object.
    pub const fn class_offset() -> usize {
        offset_of!(Object, klass)
    }

    /// Return the address of the klass slot of an object.
    pub fn klass_slot(&self) -> ArtSlot {
        get_field_slot(self, Object::class_offset() as i32)
    }

    /// Return the Class of an object.
    pub fn get_class(&self) -> ArtHeapReference<Class> {
        // println!(
        //     "get_class {:x} {:x} {:x}",
        //     // SAFETY: self is a valid Object reference.
        //     unsafe { std::mem::transmute::<&Object, usize>(self) },
        //     self.klass.reference.unwrap().get(),
        //     // SAFETY: self is a valid Object reference.
        //     unsafe { self.klass_slot().0.load::<u32>() } & 0b11
        // );
        // debug_assert!(
        //     // SAFETY: self is a valid Object reference.
        //     unsafe { self.klass_slot().0.load::<u32>() } & 0b11 != 0b11,
        //     "Broken object {:x} {:x}",
        //     // SAFETY: self is a valid Object reference.
        //     unsafe { std::mem::transmute::<&Object, usize>(self) },
        //     self.klass.reference.unwrap().get()
        // );
        debug_assert!(ArtObjectModel::is_object_sane(self.into()));
        let klass_slot = self.klass_slot();
        // XXX(kunals): Need to mask the lowest two bits to remove the
        // forwarding bits in case we want to read the size of an object, for
        // example, when we are doing a GC
        klass_slot.load_class_without_forwarding_bits()
    }
}

const fn component_size_shift_width(component_size: u32) -> usize {
    if component_size == 1 {
        0
    } else if component_size == 2 {
        1
    } else if component_size == 4 {
        2
    } else if component_size == 8 {
        3
    } else {
        0
    }
}

/// Primitive types
#[repr(C)]
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Debug)]
pub enum ArtPrimitive {
    /// Not a primitive type
    Not = 0,
    /// Boolean
    Boolean,
    /// Byte
    Byte,
    /// Char
    Char,
    /// Short
    Short,
    /// Int
    Int,
    /// Long
    Long,
    /// Float
    Float,
    /// Double
    Double,
    /// Void
    Void,
}

impl ArtPrimitive {
    /// Convert a u32 to an ArtPrimitive
    pub const fn from_u32(value: u32) -> ArtPrimitive {
        match value {
            0 => ArtPrimitive::Not,
            1 => ArtPrimitive::Boolean,
            2 => ArtPrimitive::Byte,
            3 => ArtPrimitive::Char,
            4 => ArtPrimitive::Short,
            5 => ArtPrimitive::Int,
            6 => ArtPrimitive::Long,
            7 => ArtPrimitive::Float,
            8 => ArtPrimitive::Double,
            9 => ArtPrimitive::Void,
            _ => panic!("Invalid ArtPrimitive value"),
        }
    }

    /// Get the component size shift of a primitive type
    pub const fn component_size_shift(r#type: ArtPrimitive) -> usize {
        match r#type {
            ArtPrimitive::Void | ArtPrimitive::Boolean | ArtPrimitive::Byte => 0,
            ArtPrimitive::Char | ArtPrimitive::Short => 1,
            ArtPrimitive::Int | ArtPrimitive::Float => 2,
            ArtPrimitive::Long | ArtPrimitive::Double => 3,
            ArtPrimitive::Not => component_size_shift_width(PRIM_OBJECT_REFERENCE_SIZE),
        }
    }
}

/// Class status
#[repr(u8)]
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Debug)]
pub enum ClassStatus {
    /// Zero-initialized Class object starts in this state.
    NotReady = 0,
    /// Retired, should not be used. Use the newly cloned one instead.
    Retired = 1,
    /// Has errors but the class has been resolved.
    ErrorResolved = 2,
    /// Has errors but the class has not been resolved.
    ErrorUnresolved = 3,
    /// Loaded, DEX idx in super_class_type_idx and interfaces_type_idx.
    Idx = 4,
    /// DEX idx values resolved.
    Loaded = 5,
    /// Just cloned from temporary class object.
    Resolving = 6,
    /// Part of linking.
    Resolved = 7,
    /// In the process of being verified.
    Verifying = 8,
    /// Compile time verification failed, retry at runtime.
    RetryVerificationAtRuntime = 9,
    /// Compile time verification only failed for access checks.
    VerifiedNeedsAccessChecks = 10,
    /// Logically part of linking; done pre-init.
    Verified = 11,
    /// Superclass validation part of init done.
    SuperclassValidated = 12,
    /// Class init in progress.
    Initializing = 13,
    /// Ready to go.
    Initialized = 14,
    /// Initialized and visible to all threads.
    VisiblyInitialized = 15,
}

impl ClassStatus {
    /// Convert a u32 to a ClassStatus
    pub const fn from_u32(value: u32) -> ClassStatus {
        match value {
            0 => ClassStatus::NotReady,
            1 => ClassStatus::Retired,
            2 => ClassStatus::ErrorResolved,
            3 => ClassStatus::ErrorUnresolved,
            4 => ClassStatus::Idx,
            5 => ClassStatus::Loaded,
            6 => ClassStatus::Resolving,
            7 => ClassStatus::Resolved,
            8 => ClassStatus::Verifying,
            9 => ClassStatus::RetryVerificationAtRuntime,
            10 => ClassStatus::VerifiedNeedsAccessChecks,
            11 => ClassStatus::Verified,
            12 => ClassStatus::SuperclassValidated,
            13 => ClassStatus::Initializing,
            14 => ClassStatus::Initialized,
            15 => ClassStatus::VisiblyInitialized,
            _ => panic!("Invalid ClassStatus value"),
        }
    }
}

/// Rust mirror of java.lang.Class
#[repr(C)]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct Class {
    /// Object header
    pub header: Object,
    /// Defining class loader, or null for the "bootstrap" system loader.
    pub class_loader: ArtHeapReference<ClassLoader>,
    /// For array classes, the component class object for instanceof/checkcast
    /// (for String[][][], this will be String[][]). null for non-array classes.
    pub component_type: ArtHeapReference<Class>,
    /// DexCache of resolved constant pool entries (will be null for classes generated by the
    /// runtime such as arrays and primitive classes).
    pub dex_cache: ArtHeapReference<DexCache>,
    /// Extraneous class data that is not always needed.
    pub ext_data: ArtHeapReference<Object>, // ClassExt
    /// The interface table (iftable)
    pub iftable: ArtHeapReference<Object>, // IfTable
    /// Descriptor for the class such as "java.lang.Class" or "[C". Lazily initialized by ComputeName
    pub name: ArtHeapReference<Object>, // String
    /// The superclass, or null if this is java.lang.Object or a primitive type.
    pub super_class: ArtHeapReference<Class>,
    /// Virtual method table (vtable), for use by "invoke-virtual".
    pub vtable: ArtHeapReference<Object>, // PointerArray
    /// Instance fields. These describe the layout of the contents of an Object.
    pub ifields: u64,
    /// Pointer to an ArtMethod length-prefixed array.
    pub methods: u64,
    /// Static fields length-prefixed array.
    pub sfields: u64,
    /// Access flags; low 16 bits are defined by VM spec.
    pub access_flags: u32,
    /// Class flags to help speed up visiting object references.
    pub class_flags: u32,
    /// Total size of the Class instance; used when allocating storage on GC heap.
    pub class_size: u32,
    /// TID used to check for recursive <clinit> invocation.
    pub clinit_thread_id: u32,
    /// ClassDef index in dex file, -1 if no class definition such as an array.
    pub dex_class_def_idx: i32,
    /// Type index in dex file.
    pub dex_type_idx: i32,
    /// Number of instance fields that are object refs.
    pub num_reference_instance_fields: u32,
    /// Number of static fields that are object refs.
    pub num_reference_static_fields: u32,
    /// Total object size.
    pub object_size: u32,
    /// Aligned object size for allocation fast-path.
    pub object_size_alloc_fast_path: u32,
    /// The lower 16 bits contains a Primitive::Type value. The upper 16
    /// bits contains the size shift of the primitive type.
    pub primitive_type: u32,
    /// Bitmap of offsets of ifields.
    pub reference_instance_offsets: u32,
    /// Subtype check bits and class status.
    pub status: u32,
    /// The offset of the first virtual method that is copied from an interface.
    pub copied_methods_offset: u16,
    /// The offset of the first declared virtual methods in the methods array.
    pub virtual_methods_offset: u16,
    // The following data exist in real class objects.
    // Embedded Vtable length, for class object that's instantiable, fixed size.
    // pub vtable_length: u32,
    // Embedded Imtable pointer, for class object that's not an interface, fixed size.
    // pub embedded_imtable: ImTableEntry,
    // Embedded Vtable, for class object that's not an interface, variable size.
    // pub embedded_vtable[0]: VTableEntry;
    // Static fields, variable size.
    // pub fields[0]: u32;
    // Embedded bitmap of offsets of ifields, for classes that need more than 31
    // reference-offset bits. 'reference_instance_offsets' stores the number of
    // 32-bit entries that hold the entire bitmap. We compute the offset of first
    // entry by subtracting this number from class_size.
    // pub reference_bitmap[0]: u32;
}

impl From<&ArtHeapReference<Class>> for &Class {
    fn from(c: &ArtHeapReference<Class>) -> Self {
        debug_assert_ne!(c.reference, None);
        // SAFETY: c is a valid Rust reference.
        unsafe { std::mem::transmute(c) }
    }
}

impl From<&Class> for &ArtHeapReference<Class> {
    fn from(c: &Class) -> Self {
        // SAFETY: c is a valid Rust reference.
        unsafe { std::mem::transmute(c) }
    }
}

impl From<&Object> for &Class {
    fn from(o: &Object) -> Self {
        debug_assert!(o.get_class().is_class_class());
        // SAFETY: o is a valid Class from above.
        unsafe { std::mem::transmute(o) }
    }
}

impl From<&Class> for &Object {
    fn from(c: &Class) -> Self {
        // SAFETY: c is a valid Rust reference.
        unsafe { std::mem::transmute(c) }
    }
}

#[allow(non_upper_case_globals)]
impl Class {
    /// Normal instance with at least one ref field other than the class.
    pub const kClassFlagNormal: u32 = 0x00000000;
    /// Only normal objects which have no reference fields, e.g. string or primitive array or normal
    /// class instance with no fields other than klass.
    pub const kClassFlagNoReferenceFields: u32 = 0x00000001;
    /// Class is java.lang.String.class.
    pub const kClassFlagString: u32 = 0x00000004;
    /// Class is an object array class.
    pub const kClassFlagObjectArray: u32 = 0x00000008;
    /// Class is java.lang.Class.class.
    pub const kClassFlagClass: u32 = 0x00000010;
    /// Class is ClassLoader or one of its subclasses.
    pub const kClassFlagClassLoader: u32 = 0x00000020;
    /// Class is DexCache.
    pub const kClassFlagDexCache: u32 = 0x00000040;
    /// Class is a soft/weak/phantom class.
    pub const kClassFlagSoftReference: u32 = 0x00000080;
    /// Class is a weak reference class.
    pub const kClassFlagWeakReference: u32 = 0x00000100;
    /// Class is a finalizer reference class.
    pub const kClassFlagFinalizerReference: u32 = 0x00000200;
    /// Class is the phantom reference class.
    pub const kClassFlagPhantomReference: u32 = 0x00000400;
    /// Class is a record class. See doc at java.lang.Class#isRecord().
    pub const kClassFlagRecord: u32 = 0x00000800;
    /// Class is a primitive array class.
    pub const kClassFlagPrimitiveArray: u32 = 0x00001000;
    /// The most significant 2 bits are used to store the component size shift
    /// for arrays (both primitive and object).
    pub const kArrayComponentSizeShiftShift: u32 = 30;
    /// Combination of flags to figure out if the class is either the weak/soft/phantom/finalizer
    /// reference class.
    pub const kClassFlagReference: u32 = Class::kClassFlagSoftReference
        | Class::kClassFlagWeakReference
        | Class::kClassFlagFinalizerReference
        | Class::kClassFlagPhantomReference;

    /// Interface
    pub const kAccInterface: u32 = 0x0200; // class, ic
    /// Abstract
    pub const kAccAbstract: u32 = 0x0400; // class, method, ic

    /// 'reference_instance_offsets' may contain up to 31 reference offsets. If
    /// more bits are required, then we set the most-significant bit and store the
    /// number of 32-bit bitmap entries required in the remaining bits. All the
    /// required bitmap entries after stored after static fields (at the end of the class).
    pub const kVisitReferencesSlowpathShift: u32 = 31;
    /// Mask to get the slow path flag.
    pub const kVisitReferencesSlowpathMask: u32 = 1_u32 << Class::kVisitReferencesSlowpathShift;

    /// Shift primitive type by kPrimitiveTypeSizeShiftShift to get the component type size shift
    /// Used for computing array size as follows:
    /// array_bytes = header_size + (elements << (primitive_type >> kPrimitiveTypeSizeShiftShift))
    pub const kPrimitiveTypeSizeShiftShift: u32 = 16;
    /// Mask to get the primitive type
    pub const kPrimitiveTypeMask: u32 = (1_u32 << Class::kPrimitiveTypeSizeShiftShift) - 1;

    /// Check that the Class is a normal class. Note that this function may not return
    /// for invalid normal classes.
    pub fn check_normal_class(&self) {
        debug_assert!(!self.is_variable_size());
        debug_assert!(!self.is_class_class());
        debug_assert!(!self.is_string_class());
        debug_assert!(!self.is_classloader_class());
        debug_assert!(!self.is_array_class());
    }

    /// Check that the Class has no reference fields.
    pub fn check_no_reference_fields(&self) {
        debug_assert!(!self.is_class_class());
        debug_assert!(!self.is_object_array_class());

        // String still has instance fields for reflection purposes but these don't exist in
        // actual string instances.
        #[cfg(debug_assertions)]
        if !self.is_string_class() {
            let mut total_reference_instance_fields: usize = 0;
            let mut super_class = self;
            loop {
                total_reference_instance_fields +=
                    super_class.num_reference_instance_fields as usize;
                if super_class.super_class.reference.is_none() {
                    break;
                } else {
                    super_class = &super_class.super_class;
                }
            }
            // The only reference field should be the object's class.
            assert_eq!(total_reference_instance_fields, 1_usize);
        }
    }

    /// Check that the Class should have an embedded vtable.
    pub fn should_have_embedded_vtable(&self) -> bool {
        self.is_instantiable()
    }

    /// Returns true if this class is the placeholder and should retire and
    /// be replaced with a class with the right size for embedded imt/vtable.
    pub fn is_temp(&self) -> bool {
        let status = self.get_status();
        status < ClassStatus::Resolving
            && status != ClassStatus::ErrorResolved
            && self.should_have_embedded_vtable()
    }

    /// Is the Class resolved?
    pub fn is_resolved(&self) -> bool {
        let status = self.get_status();
        status >= ClassStatus::Resolved || status == ClassStatus::ErrorResolved
    }

    /// Is the Class erroneous?
    pub fn is_erroneous(&self) -> bool {
        let status = self.get_status();
        status == ClassStatus::ErrorResolved || status == ClassStatus::ErrorUnresolved
    }

    /// Can the Class be instantiated?
    pub fn is_instantiable(&self) -> bool {
        (!self.is_primitive() && !self.is_interface() && !self.is_abstract())
            || (self.is_abstract() && self.is_array_class())
    }

    /// Is the Class the java.lang.Class class?
    pub fn is_class_class(&self) -> bool {
        // OK to look at old copies since java.lang.Class.class is non-moveable
        let java_lang_class = &self.header.get_class();
        // SAFETY: java_lang_class.reference is always a valid u32
        let java_lang_class_ref: u32 = unsafe { std::mem::transmute(java_lang_class.reference) };
        // SAFETY: self is a valid Rust address
        let self_usize: usize = unsafe { std::mem::transmute(self) };
        self_usize == java_lang_class_ref as usize
    }

    /// Is the Class the java.lang.String class?
    pub fn is_string_class(&self) -> bool {
        self.class_flags & Class::kClassFlagString != 0
    }

    /// Is the Class the java.lang.ClassLoader or one of its subclasses?
    pub fn is_classloader_class(&self) -> bool {
        self.class_flags & Class::kClassFlagClassLoader != 0
    }

    /// Is the Class an array class?
    pub fn is_array_class(&self) -> bool {
        self.component_type.reference.is_some()
    }

    /// Is the Class an object array class?
    pub fn is_object_array_class(&self) -> bool {
        let component_type = self.component_type;
        component_type.reference.is_some() && !component_type.is_primitive()
    }

    /// Is the Class a DexCache class?
    pub fn is_dexcache_class(&self) -> bool {
        self.class_flags & Class::kClassFlagDexCache != 0
    }

    /// Is the Class a type of java.lang.Reference class?
    pub fn is_type_of_reference_class(&self) -> bool {
        self.class_flags & Class::kClassFlagReference != 0
    }

    /// Is the Class a variable sized class?
    pub fn is_variable_size(&self) -> bool {
        self.is_class_class() || self.is_array_class() || self.is_string_class()
    }

    /// Is the Class an abstract class?
    pub fn is_abstract(&self) -> bool {
        self.access_flags & Class::kAccAbstract != 0
    }

    /// Is the Class an interface?
    pub fn is_interface(&self) -> bool {
        self.access_flags & Class::kAccInterface != 0
    }

    /// Is the Class a primitive class?
    pub fn is_primitive(&self) -> bool {
        self.get_primitive_type() != ArtPrimitive::Not
    }

    /// Is the Class instantiable and not an array?
    pub fn is_instantiable_non_array(&self) -> bool {
        !self.is_primitive()
            && !self.is_interface()
            && !self.is_abstract()
            && !self.is_array_class()
    }

    /// Get the status of the Class.
    pub fn get_status(&self) -> ClassStatus {
        let addr = get_field_address(self, offset_of!(Class, status) as i32);
        // SAFETY: addr is a valid address from above.
        // XXX(kunals): Reading the field without barrier is used exclusively for IsVisiblyInitialized().
        let status = unsafe { addr.atomic_load::<AtomicI32>(Ordering::SeqCst) as u32 };
        ClassStatus::from_u32(status >> (32 - 4))
    }

    /// Get the component size shift of the Class.
    pub fn get_component_size_shift(&self) -> usize {
        self.component_type.get_primitive_type_size_shift()
    }

    /// Get the primitive type of the Class.
    pub fn get_primitive_type(&self) -> ArtPrimitive {
        let r#type = ArtPrimitive::from_u32(self.primitive_type & Class::kPrimitiveTypeMask);
        debug_assert_eq!(
            (self.primitive_type >> Class::kPrimitiveTypeSizeShiftShift) as usize,
            ArtPrimitive::component_size_shift(r#type),
        );
        r#type
    }

    /// Get the size shift of the primitive type of the Class.
    pub fn get_primitive_type_size_shift(&self) -> usize {
        let size_shift = (self.primitive_type >> Class::kPrimitiveTypeSizeShiftShift) as usize;
        debug_assert_eq!(
            size_shift,
            ArtPrimitive::component_size_shift(ArtPrimitive::from_u32(
                self.primitive_type & Class::kPrimitiveTypeMask
            )),
        );
        size_shift
    }

    /// Get the reference_instance_offsets field of the Class.
    pub fn get_reference_instance_offsets(&self) -> u32 {
        debug_assert!(self.is_resolved() || self.is_erroneous());
        self.reference_instance_offsets
    }

    /// Get the offset of the first reference static field of the Class.
    pub fn get_first_reference_static_fields_offset(&self, ptr_size: usize) -> usize {
        debug_assert!(self.is_resolved());
        if self.should_have_embedded_vtable() {
            Class::compute_class_size(
                true,
                self.get_embedded_vtable_length(),
                0,
                0,
                0,
                0,
                0,
                0,
                ptr_size,
            )
        } else {
            std::mem::size_of::<Class>()
        }
    }

    /// Get the embedded vtable length of the Class.
    pub fn get_embedded_vtable_length(&self) -> u32 {
        let addr = get_field_address(self, std::mem::size_of::<Class>() as i32);
        // SAFETY: addr is a valid address from above.
        // Have to use an atomic load to comply with Java semantics
        unsafe { addr.atomic_load::<AtomicI32>(Ordering::Relaxed) as u32 }
    }

    /// Get the object size from the Class.
    pub fn get_object_size(&self) -> usize {
        debug_assert!(!self.is_variable_size());
        self.object_size as usize
    }

    #[allow(clippy::too_many_arguments)]
    fn compute_class_size(
        has_embedded_vtable: bool,
        num_vtable_entries: u32,
        mut num_8bit_static_fields: u32,
        mut num_16bit_static_fields: u32,
        mut num_32bit_static_fields: u32,
        num_64bit_static_fields: u32,
        num_ref_static_fields: u32,
        num_ref_bitmap_entries: u32,
        ptr_size: usize,
    ) -> usize {
        // Space used by java.lang.Class and its instance fields.
        let mut size = std::mem::size_of::<Class>();

        // Space used by the embedded vtable.
        if has_embedded_vtable {
            size = raw_align_up(size + std::mem::size_of::<u32>(), ptr_size);
            size += ptr_size; // size of pointer to IMT
            size += num_vtable_entries as usize * ptr_size;
        }

        // Space used by reference statics.
        size += num_ref_static_fields as usize * ART_HEAP_REFERENCE_SIZE;
        if !raw_is_aligned(size, 8_usize) && num_64bit_static_fields > 0_u32 {
            let mut gap: u32 = 8_u32 - (size & 0x7) as u32;
            size += gap as usize; // will be padded

            // Shuffle 4-byte fields forward.
            while gap >= std::mem::size_of::<u32>() as u32 && num_32bit_static_fields != 0_u32 {
                num_32bit_static_fields -= 1_u32;
                gap -= std::mem::size_of::<u32>() as u32;
            }
            // Shuffle 2-byte fields forward.
            while gap >= std::mem::size_of::<u16>() as u32 && num_16bit_static_fields != 0_u32 {
                num_16bit_static_fields -= 1_u32;
                gap -= std::mem::size_of::<u16>() as u32;
            }
            // Shuffle byte fields forward.
            while gap >= std::mem::size_of::<u8>() as u32 && num_8bit_static_fields != 0_u32 {
                num_8bit_static_fields -= 1_u32;
                gap -= std::mem::size_of::<u8>() as u32;
            }
        }

        // Guaranteed to be at least 4 byte aligned. No need for further alignments.
        // Space used for primitive static fields.
        size += num_8bit_static_fields as usize * std::mem::size_of::<u8>()
            + num_16bit_static_fields as usize * std::mem::size_of::<u16>()
            + num_32bit_static_fields as usize * std::mem::size_of::<u32>()
            + num_64bit_static_fields as usize * std::mem::size_of::<u64>();

        // Space used by reference-offset bitmap.
        if num_ref_bitmap_entries > 0 {
            size = raw_align_up(size, std::mem::size_of::<u32>());
            size += num_ref_bitmap_entries as usize * std::mem::size_of::<u32>();
        }

        size
    }
}

/// Rust mirror of an ART Array
#[repr(C)]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct Array {
    /// Object header
    pub header: Object,
    /// The length of the array
    pub length: i32,
    /// The start of the array payload
    pub first_element: u32,
}

impl From<&Object> for &Array {
    fn from(o: &Object) -> Self {
        debug_assert!(o.get_class().is_array_class());
        // SAFETY: o is a valid Array from above
        unsafe { std::mem::transmute(o) }
    }
}

impl From<&Array> for &Object {
    fn from(a: &Array) -> Self {
        // SAFETY: a is a valid Array and all arrays are valid java.lang.Objects
        unsafe { std::mem::transmute(a) }
    }
}

impl Array {
    /// Get the offset to the payload of the array
    pub fn data_offset(&self, component_size: usize) -> usize {
        debug_assert!(component_size.is_power_of_two());
        let data_offset = raw_align_up(offset_of!(Array, first_element), component_size);
        debug_assert_eq!(raw_align_up(data_offset, component_size), data_offset);
        data_offset
    }

    /// Get the component size shift of the array
    pub fn get_component_size_shift(&self) -> usize {
        self.header.get_class().get_component_size_shift()
    }

    /// Get the size of the array
    pub fn size_of(&self) -> usize {
        let component_size_shift = self.get_component_size_shift();
        let component_count = self.length as usize;
        let header_size = self.data_offset(1 << component_size_shift);
        let data_size = component_count << component_size_shift;
        header_size + data_size
    }
}

/// Rust mirror of an ART ObjectArray
pub struct ObjectArray<T> {
    /// The embedded array
    pub array: Array,
    /// Phantom data
    phantom: PhantomData<T>,
}

impl<T> From<&Object> for &ObjectArray<T> {
    fn from(o: &Object) -> Self {
        debug_assert!(o.get_class().is_object_array_class());
        // SAFETY: o is a valid ObjectArray from above
        unsafe { std::mem::transmute(o) }
    }
}

impl<T> From<&ObjectArray<T>> for &Object {
    fn from(o: &ObjectArray<T>) -> Self {
        // SAFETY: o is a valid ObjectArray and all arrays are valid java.lang.Objects
        unsafe { std::mem::transmute(o) }
    }
}

impl<T> ObjectArray<T> {
    /// Get the offset of an element in the array
    pub fn offset_of_element(&self, index: usize) -> usize {
        self.array.data_offset(ART_HEAP_REFERENCE_SIZE) + index * ART_HEAP_REFERENCE_SIZE
    }
}

/// Rust mirror of a java.lang.String
#[repr(C)]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct ArtString {
    /// Object header
    pub header: Object,
    /// Count of the string
    pub count: i32,
    /// Hash code of the string
    pub hash_code: u32,
    // String value
    // pub string_value: u16,
}

impl From<&Object> for &ArtString {
    fn from(o: &Object) -> Self {
        debug_assert!(o.get_class().is_string_class());
        // SAFETY: o is a valid ArtString from above
        unsafe { std::mem::transmute(o) }
    }
}

impl From<&ArtString> for &Object {
    fn from(s: &ArtString) -> Self {
        // SAFETY: s is a valid ArtString and all strings are valid java.lang.Objects
        unsafe { std::mem::transmute(s) }
    }
}

impl ArtString {
    /// Is the string compressed?
    pub fn is_compressed(&self) -> bool {
        self.count & 0b1 == 0b0
    }

    /// Get the length of the string.
    pub fn get_length(&self) -> i32 {
        self.count >> 1
    }

    /// Get the size of the string.
    pub fn size_of(&self) -> usize {
        let mut size = std::mem::size_of::<ArtString>();
        if self.is_compressed() {
            size += std::mem::size_of::<u8>() * (self.get_length() as usize);
        } else {
            size += std::mem::size_of::<u16>() * (self.get_length() as usize);
        }
        mmtk::util::conversions::raw_align_up(size, crate::Art::MIN_ALIGNMENT)
    }
}

/// Rust mirror of java.lang.Reference
#[repr(C)]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct Reference {
    /// Object header
    pub header: Object,
    /// Circular linked list of pending references.
    pub pending_next: ArtHeapReference<Reference>,
    /// Circular linked list of enqueued references.
    pub queue: ArtHeapReference<Object>,
    /// Next reference in the queue.
    pub queue_next: ArtHeapReference<Reference>,
    /// The object to which this reference refers to.
    pub referent: ArtHeapReference<Object>,
}

impl From<&Object> for &Reference {
    fn from(o: &Object) -> Self {
        debug_assert!(o.get_class().is_type_of_reference_class());
        // SAFETY: o is a valid type of java.lang.Reference from above
        unsafe { std::mem::transmute(o) }
    }
}

impl From<&Reference> for &Object {
    fn from(r: &Reference) -> Self {
        // SAFETY: r is a valid java.lang.Reference
        unsafe { std::mem::transmute(r) }
    }
}

/// Rust mirror of java.lang.DexCache
#[repr(C)]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct DexCache {
    /// Object header
    pub header: Object,
    /// The ClassLoader that this DexCache is associated with
    pub class_loader: ArtHeapReference<ClassLoader>,
    /// The location of the dex file
    pub location: ArtHeapReference<Object>, // String
    /// const DexFile*
    pub dex_file: u64,
    /// Array of call sites
    pub resolved_call_sites: u64,
    /// NativeDexCacheArray holding ArtField's
    pub resolved_fields: u64,
    /// Array of ArtField's
    pub resolved_fields_array: u64,
    /// DexCacheArray holding mirror::MethodType's
    pub resolved_method_types: u64,
    /// Array of mirror::MethodType's
    pub resolved_method_types_array: u64,
    /// NativeDexCacheArray holding ArtMethod's
    pub resolved_methods: u64,
    /// Array of ArtMethod's
    pub resolved_methods_array: u64,
    /// DexCacheArray holding mirror::Class's
    pub resolved_types: u64,
    /// Array of resolved types
    pub resolved_types_array: u64,
    /// DexCacheArray holding mirror::String's
    pub strings: u64,
    /// Array of String's
    pub strings_array: u64,
}

impl From<&Object> for &DexCache {
    fn from(o: &Object) -> Self {
        debug_assert!(o.get_class().is_dexcache_class());
        // SAFETY: o is a valid DexCache from above
        unsafe { std::mem::transmute(o) }
    }
}

impl From<&DexCache> for &Object {
    fn from(d: &DexCache) -> Self {
        // SAFETY: d is a valid DexCache
        unsafe { std::mem::transmute(d) }
    }
}

/// Rust mirror of java.lang.ClassLoader
#[repr(C)]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct ClassLoader {
    /// Object header
    pub header: Object,
    /// Class loader name.
    pub name: ArtHeapReference<Object>, // String
    /// Packages defined by this class loader.
    pub packages: ArtHeapReference<Object>,
    /// The parent class loader for delegation
    pub parent: ArtHeapReference<ClassLoader>,
    /// Proxy cache map.
    pub proxy_cache: ArtHeapReference<Object>,
    /// Pointer to the allocator used by the runtime to allocate metadata such as
    /// ArtFields and ArtMethods.
    pub allocator: u64,
    /// Pointer to the class table.
    pub class_table: u64,
}

impl From<&Object> for &ClassLoader {
    fn from(o: &Object) -> Self {
        debug_assert!(o.get_class().is_classloader_class());
        // SAFETY: o is a valid ClassLoader from above
        unsafe { std::mem::transmute(o) }
    }
}

impl From<&ClassLoader> for &Object {
    fn from(cl: &ClassLoader) -> Self {
        // SAFETY: cl is a valid ClassLoader
        unsafe { std::mem::transmute(cl) }
    }
}

/// Get the size of an object
#[inline]
pub fn get_object_size(object: ObjectReference) -> usize {
    let o: &Object = object.into();
    let result = if o.get_class().is_array_class() {
        let a: &Array = o.into();
        a.size_of()
    } else if o.get_class().is_class_class() {
        let klass: &Class = o.into();
        klass.class_size as usize
    } else if o.get_class().is_string_class() {
        let s: &ArtString = o.into();
        s.size_of()
    } else {
        o.get_class().get_object_size()
    };
    debug_assert!(result >= OBJECT_HEADER_SIZE as usize);
    result
}

/// Rust mirror of `ArtField`
#[repr(C)]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct ArtField {
    /// Field's declaring class
    pub declaring_class: ArtHeapReference<Class>,
    /// Access flags; low 16 bits are defined by spec.
    pub access_flags: u32,
    /// Dex cache index of field id
    pub field_dex_idx: u32,
    /// Offset of field within an instance or in the Class' static fields
    pub offset: u32,
}

/// Rust mirror of `ArtMethod`
#[repr(C)]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct ArtMethod {
    /// Method's declaring class
    pub declaring_class: ArtHeapReference<Class>,
    /// Access flags; low 16 bits are defined by spec.
    pub access_flags: u32,
    /// Index into method_ids of the dex file associated with this method
    pub dex_method_index: u32,
    /// Entry within a dispatch table for this method.
    pub method_index: u16,
    /// Union of hotness count or an IMT index
    pub hotness_count_or_imt_index: u16,
    /// Method data depending on its type
    pub data: OpaquePointer,
    /// Method dispatch from quick compiled code
    pub entry_point_from_quick_compiled_code: OpaquePointer,
}
