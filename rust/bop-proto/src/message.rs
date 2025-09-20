use core::marker::PhantomData;
use core::mem;
use core::ops::{Deref, DerefMut};
use core::ptr;
use core::hint::unlikely;

use crate::alloc::{Allocator, Global, Grow, TruncResult};
use crate::block::Block;
use crate::header::Header;

pub trait Root {
    fn new(base: *const u8) -> Self;

    fn base_ptr(&self) -> *const u8;
}

pub trait RootUnsafe {
    fn new(base: *const u8, bounds: *const u8) -> Self;

    fn base_ptr(&self) -> *const u8;
}

/// Message represents a single contiguous chunk of memory.
///
/// Layout:
///     | Header | Base Struct | Blocks |
pub trait Message {
    type Header: Header;
    type Block: Block;
    type Layout: Sized;
    type Root: Root;
    type RootUnsafe: RootUnsafe;

    const SIZE: usize;
    const INITIAL_SIZE: usize;
    const BASE_OFFSET: isize;
    const BASE_SIZE: isize;
}

/// Flex messages have a variable length.
pub trait Flex: Message {}

pub trait MessageBox: Deref<Target=Self::Root> {
    type Header: Header;
    type Message: Message;
    type Allocator: Allocator;
    type Root: Sized;

    fn base_ptr(&self) -> *const u8;

    #[inline(always)]
    fn size(&self) -> usize {
        self.header().size()
    }

    #[inline(always)]
    fn capacity(&self) -> usize {
        self.header().size()
    }

    #[inline(always)]
    fn header(&self) -> &Self::Header {
        unsafe { &*(self.base_ptr().offset(-Self::Header::SIZE) as *mut Self::Header) }
    }
}

pub trait MessageBoxMut: MessageBox + DerefMut {
    #[inline(always)]
    fn header_mut(&mut self) -> &mut Self::Header {
        unsafe { &mut *(self.base_ptr().offset(-Self::Header::SIZE) as *mut Self::Header) }
    }
}

pub trait RootBuilder<B: Builder>: Sized {
    fn root_ptr(&self) -> *mut u8;

    fn finish(self) -> Safe<B::Message, B::Allocator>;
}

/// Builder manages a dynamic underlying buffer rather than a fixed buffer for
/// messages that contain dynamically sized types like strings, vectors, maps, etc.
pub trait Builder: Sized {
    type Message: Message;
    type Header: Header;
    type Block: Block;
    type Grow: Grow;
    type Allocator: Allocator;

    fn root_ptr(&self) -> *mut u8;

    fn end_ptr(&self) -> *mut u8;

    #[inline(always)]
    fn to_vptr(&self, block: *mut Self::Block) -> usize {
        Self::Block::offset(self.root_ptr(), block) as usize
    }

    #[inline(always)]
    fn offset(&self, offset: isize) -> *mut u8 {
        unsafe { self.root_ptr().offset(offset) }
    }

    #[inline(always)]
    fn base_ptr(&self) -> *mut u8 {
        unsafe { self.root_ptr().offset(Self::Header::SIZE) }
    }

    #[inline(always)]
    fn tail_ptr(&self) -> *mut u8 {
        unsafe { self.root_ptr().offset(self.size() as isize) }
    }

    #[inline(always)]
    fn base_size(&self) -> usize {
        (unsafe { &mut *(self.root_ptr() as *mut Self::Header) }).base_size()
    }

    #[inline(always)]
    fn capacity(&self) -> usize {
        unsafe { self.end_ptr() as usize - self.root_ptr() as usize }
    }

    #[inline(always)]
    fn size(&self) -> usize {
        unsafe { Self::Header::get_size(self.root_ptr()) }
    }

    #[inline(always)]
    fn take(self) -> (*const u8, *const u8) {
        let root = self.root_ptr();
        let end = self.end_ptr();
        mem::forget(self);
        (root, end)
    }

    #[inline(always)]
    fn finish(self) -> Safe<Self::Message, Self::Allocator> {
        let r = Safe::from_base(self.base_ptr());
        mem::forget(self);
        r
    }

    fn truncate(&mut self, by: usize) -> TruncResult;

    fn allocate(&mut self, size: usize) -> *mut Self::Block;

    fn append(&mut self, size: usize) -> *mut Self::Block;

    fn reallocate(&mut self, block: &mut Self::Block, size: usize) -> *mut Self::Block;

    fn deallocate(&mut self, block: &mut Self::Block);
}

/// Slice provides a safe read-only byte range accessor of an underlying Builder.
pub struct Slice<'a, B>(B, PhantomData<&'a ()>);

impl<'a, B> Deref for Slice<'a, B> {
    type Target = B;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// SliceMut provides a safe mutable byte range accessor of an underlying Builder.
pub struct SliceMut<'a, B>(B, PhantomData<&'a mut ()>);

impl<'a, B> Deref for SliceMut<'a, B> {
    type Target = B;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a, B> DerefMut for SliceMut<'a, B> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

fn alloc<M: Message, A: Allocator>() -> *mut u8 {
    let size = M::INITIAL_SIZE;
    let p = A::allocate(size);
    if p == ptr::null_mut() {
        return ptr::null_mut();
    }
    unsafe { M::Header::init(p, M::INITIAL_SIZE, M::SIZE) }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Safe Box
////////////////////////////////////////////////////////////////////////////////////////////////////
pub struct Safe<M: Message, A: Allocator = Global> {
    root: M::Root,
    _p: PhantomData<(M, A)>,
}

impl<M: Message, A: Allocator> Clone for Safe<M, A> {
    fn clone(&self) -> Self {
        let p = A::allocate(self.size());
        if unlikely(p == ptr::null_mut()) {
            panic!("out of memory");
        } else {
            unsafe {
                ptr::copy_nonoverlapping(
                    self.base_ptr().offset(-M::Header::SIZE) as *mut u8,
                    p,
                    self.size());
            }
            Self { root: M::Root::new(unsafe { p.offset(M::Header::SIZE) }), _p: PhantomData }
        }
    }
}

impl<M: Message, A: Allocator> Safe<M, A> {
    pub fn new() -> Option<Self> {
        let p = alloc::<M, A>();
        if p == ptr::null_mut() {
            None
        } else {
            Some(Self { root: M::Root::new(unsafe { p.offset(M::Header::SIZE) }), _p: PhantomData })
        }
    }
}

impl<M: Message, A: Allocator> Safe<M, A> {
    pub(crate) fn from_root(root: *const u8) -> Self {
        Self { root: M::Root::new(unsafe { root.offset(M::Header::SIZE) }), _p: PhantomData }
    }

    pub(crate) fn from_base(base: *const u8) -> Self {
        Self { root: M::Root::new(base), _p: PhantomData }
    }

    pub fn into_mut(self) -> SafeMut<M, A> {
        let to = SafeMut::from_base(self.base_ptr());
        mem::forget(self);
        to
    }
}

impl<M: Message, A: Allocator> Deref for Safe<M, A> {
    type Target = M::Root;

    fn deref(&self) -> &Self::Target {
        &self.root
    }
}

impl<M: Message, A: Allocator> MessageBox for Safe<M, A> {
    type Header = M::Header;
    type Message = M;
    type Allocator = A;
    type Root = M::Root;

    #[inline(always)]
    fn base_ptr(&self) -> *const u8 {
        self.root.base_ptr()
    }
}

impl<R: Message, A: Allocator> Drop for Safe<R, A> {
    fn drop(&mut self) {
        A::deallocate(
            unsafe { self.root.base_ptr().offset(-R::Header::SIZE) as *mut u8 },
            self.capacity(),
        )
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Safe Box
////////////////////////////////////////////////////////////////////////////////////////////////////
pub struct SafeMut<M: Message, A: Allocator = Global> {
    root: M::Root,
    _p: PhantomData<(M, A)>,
}

impl<M: Message, A: Allocator> Clone for SafeMut<M, A> {
    fn clone(&self) -> Self {
        let p = A::allocate(self.size());
        if p == ptr::null_mut() {
            panic!("Out of memory");
        } else {
            unsafe {
                ptr::copy_nonoverlapping(
                    self.base_ptr().offset(-M::Header::SIZE) as *mut u8,
                    p,
                    self.size());
            }
            Self { root: M::Root::new(unsafe { p.offset(M::Header::SIZE) }), _p: PhantomData }
        }
    }
}

impl<M: Message, A: Allocator> SafeMut<M, A> {
    pub fn new() -> Option<Self> {
        let p = alloc::<M, A>();
        if p == ptr::null_mut() {
            None
        } else {
            Some(Self { root: M::Root::new(unsafe { p.offset(M::Header::SIZE) }), _p: PhantomData })
        }
    }
}

impl<M: Message, A: Allocator> SafeMut<M, A> {
    pub(crate) fn from_header(h: *const u8) -> Self {
        Self { root: M::Root::new(unsafe { h.offset(M::Header::SIZE) }), _p: PhantomData }
    }

    pub(crate) fn from_base(b: *const u8) -> Self {
        Self { root: M::Root::new(b), _p: PhantomData }
    }

    pub fn into(self) -> Safe<M, A> {
        let b = Safe::from_base(self.base_ptr());
        mem::forget(self);
        b
    }

    pub fn finish(self) -> Safe<M, A> {
        let b = Safe::from_base(self.base_ptr());
        mem::forget(self);
        b
    }
}

impl<M: Message, A: Allocator> Deref for SafeMut<M, A> {
    type Target = M::Root;

    fn deref(&self) -> &Self::Target {
        &self.root
    }
}

impl<M: Message, A: Allocator> DerefMut for SafeMut<M, A> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.root
    }
}

impl<M: Message, A: Allocator> MessageBox for SafeMut<M, A> {
    type Header = M::Header;
    type Message = M;
    type Allocator = A;
    type Root = M::Root;

    #[inline(always)]
    fn base_ptr(&self) -> *const u8 {
        self.root.base_ptr()
    }
}

impl<M: Message, A: Allocator> MessageBoxMut for SafeMut<M, A> {}

impl<M: Message, A: Allocator> Drop for SafeMut<M, A> {
    fn drop(&mut self) {
        A::deallocate(
            unsafe { self.base_ptr().offset(-M::Header::SIZE) as *mut u8 },
            self.capacity(),
        )
    }
}

impl<M: Message, A: Allocator> Into<Safe<M, A>> for SafeMut<M, A> {
    fn into(self) -> Safe<M, A> {
        let into = Safe::from_base(self.base_ptr());
        mem::forget(self);
        into
    }
}

impl<M: Message, A: Allocator> From<Safe<M, A>> for SafeMut<M, A> {
    fn from(s: Safe<M, A>) -> Self {
        let to = Self { root: M::Root::new(s.base_ptr()), _p: PhantomData };
        mem::forget(s);
        to
    }
}


////////////////////////////////////////////////////////////////////////////////////////////////////
// Unsafe
////////////////////////////////////////////////////////////////////////////////////////////////////
pub struct Unsafe<M: Message, A: Allocator = Global> {
    root: M::RootUnsafe,
    _p: PhantomData<(M, A)>,
}

impl<M: Message, A: Allocator> Unsafe<M, A> {}

impl<M: Message, A: Allocator> Deref for Unsafe<M, A> {
    type Target = M::RootUnsafe;

    fn deref(&self) -> &Self::Target {
        &self.root
    }
}

impl<M: Message, A: Allocator> DerefMut for Unsafe<M, A> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.root
    }
}

impl<M: Message, A: Allocator> MessageBox for Unsafe<M, A> {
    type Header = M::Header;
    type Message = M;
    type Allocator = A;
    type Root = M::RootUnsafe;

    #[inline(always)]
    fn base_ptr(&self) -> *const u8 {
        self.root.base_ptr()
    }
}

impl<M: Message, A: Allocator> Drop for Unsafe<M, A> {
    fn drop(&mut self) {
        A::deallocate(unsafe { self.root.base_ptr().offset(-M::Header::SIZE) as *mut u8 },
                      self.capacity())
    }
}


////////////////////////////////////////////////////////////////////////////////////////////////////
// Unsafe Box
////////////////////////////////////////////////////////////////////////////////////////////////////
pub struct UnsafeMut<M: Message, A: Allocator = Global> {
    root: M::RootUnsafe,
    _p: PhantomData<(M, A)>,
}

impl<R: Message, A: Allocator> UnsafeMut<R, A> {}

impl<R: Message, A: Allocator> UnsafeMut<R, A> {
    // pub(crate) fn from_header(h: *const u8) -> Self {
    //     Self { root: R::Safe::new(unsafe { h.offset(R::Header::SIZE) }), _p: PhantomData }
    // }
    //
    // pub(crate) fn from_base(b: *const u8) -> Self {
    //     Self { root: R::Safe::new(b), _p: PhantomData }
    // }
    //
    // pub fn into(self) -> Box<R, A> {
    //     let b = Box::from_base(self.base_ptr());
    //     mem::forget(self);
    //     b
    // }
    //
    // pub fn finish(self) -> Box<R, A> {
    //     let b = Box::from_base(self.base_ptr());
    //     mem::forget(self);
    //     b
    // }
}

impl<M: Message, A: Allocator> Deref for UnsafeMut<M, A> {
    type Target = M::RootUnsafe;

    fn deref(&self) -> &Self::Target {
        &self.root
    }
}

impl<M: Message, A: Allocator> DerefMut for UnsafeMut<M, A> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.root
    }
}

impl<M: Message, A: Allocator> MessageBox for UnsafeMut<M, A> {
    type Header = M::Header;
    type Message = M;
    type Allocator = A;
    type Root = M::RootUnsafe;

    #[inline(always)]
    fn base_ptr(&self) -> *const u8 {
        self.root.base_ptr()
    }
}

impl<R: Message, A: Allocator> Drop for UnsafeMut<R, A> {
    fn drop(&mut self) {
        A::deallocate(
            unsafe { self.base_ptr().offset(-R::Header::SIZE) as *mut u8 },
            self.capacity(),
        )
    }
}

#[inline(always)]
pub const fn f32_to_le(x: f32) -> f32 {
    #[cfg(target_endian = "little")]
    {
        x
    }
    #[cfg(not(target_endian = "little"))]
    {
        f32::from_bits(u32::swap_bytes(f32::to_bits(x)))
    }
}

#[inline(always)]
pub const fn f32_from_le(x: f32) -> f32 {
    #[cfg(target_endian = "little")]
    {
        x
    }
    #[cfg(not(target_endian = "little"))]
    {
        f32::from_bits(u32::swap_bytes(f32::to_bits(x)))
    }
}

#[inline(always)]
pub const fn f64_to_le(x: f64) -> f64 {
    #[cfg(target_endian = "little")]
    {
        x
    }
    #[cfg(not(target_endian = "little"))]
    {
        f64::from_bits(u64::swap_bytes(f64::to_bits(x)))
    }
}

#[inline(always)]
pub const fn f64_from_le(x: f64) -> f64 {
    #[cfg(target_endian = "little")]
    {
        x
    }
    #[cfg(not(target_endian = "little"))]
    {
        f64::from_bits(u64::swap_bytes(f64::to_bits(x)))
    }
}

#[cfg(test)]
mod tests {
    use core::mem;
    use crate::appender::Appender;
    // use crate::hash::{hash, hash_default, hash_sized};
    use crate::header::{Only, Flex16, Header16};

    use super::*;

    pub trait Order {
        type Price: Price;

        // Read
        fn id(&self) -> u64;
        fn price(&self) -> Slice<Self::Price>;

        // Stable Write - underlying buffer must never reallocate
        fn set_id(&mut self, value: u64) -> &mut Self;
        fn price_mut(&mut self) -> SliceMut<Self::Price>;
    }

    pub trait OrderBuilder: Order {
        type PriceBuilder: Price;

        // Unstable Write - underlying buffer may reallocate
        fn set_client_id(&mut self, value: &str) -> &mut Self;
    }

    pub trait Price {
        fn open(&self) -> f64;
        fn high(&self) -> f64;
        fn low(&self) -> f64;
        fn close(&self) -> f64;

        fn set_open(&mut self, value: f64) -> &mut Self;
        fn set_high(&mut self, value: f64) -> &mut Self;
        fn set_low(&mut self, value: f64) -> &mut Self;
        fn set_close(&mut self, value: f64) -> &mut Self;

        fn copy_to(&self, to: &mut impl Price) {
            to.set_open(self.open())
                .set_high(self.high())
                .set_low(self.low())
                .set_close(self.close());
        }
    }

    /////////////////////////////////////////////////////////////////////////////
    // Order - Layout
    /////////////////////////////////////////////////////////////////////////////
    #[repr(C, packed)]
    pub struct OrderData {
        id: u64,
        price: PriceData,
        client_id: [u8; 10],
        _pad: [u8; 6],
    }

    impl OrderData {
        pub fn id(&self) -> u64 {
            u64::from_le(self.id)
        }

        pub fn price(&self) -> &PriceData {
            &self.price
        }

        pub fn set_id(&mut self, value: u64) -> &mut Self {
            self.id = value.to_le();
            self
        }

        pub fn price_mut(&mut self) -> &mut PriceData {
            &mut self.price
        }
    }

    //////////////////////////////////////////////////////////////////////////////
    // OrderMessage
    //////////////////////////////////////////////////////////////////////////////

    pub struct OrderMessage<H: Header = Flex16>(PhantomData<H>);

    impl<H: Header> Message for OrderMessage<H> {
        type Header = H;
        type Block = H::Block;
        type Layout = OrderData;
        type Root = OrderSafe<Self>;
        type RootUnsafe = OrderUnsafe<Self>;

        const SIZE: usize = mem::size_of::<Self::Layout>();
        const INITIAL_SIZE: usize = H::SIZE as usize + Self::SIZE;
        const BASE_OFFSET: isize = H::SIZE;
        const BASE_SIZE: isize = Self::SIZE as isize;
    }

    impl<H: Header> Flex for OrderMessage<H> {}

    pub type OrderMsg = OrderMessage<Flex16>;

    impl OrderMessage<Flex16> {
        pub fn new() -> Option<SafeMut<OrderMsg>> {
            SafeMut::new()
        }

        fn new_builder(extra: usize) -> Option<OrderMut<Appender<Self>>> {
            if let Some(a) = Appender::<Self>::new(extra) {
                Some(OrderMut::<Appender<Self>>(a, PhantomData))
            } else {
                None
            }
        }
    }

    impl<H: Header + Header16> OrderMessage<H> {
        pub fn new_with_header() -> Option<Safe<OrderMessage<H>>> {
            Safe::new()
        }

        fn new_builder_with_header(extra: usize) -> Option<OrderMut<Appender<Self>>> {
            if let Some(a) = Appender::<Self>::new(extra) {
                Some(OrderMut::<Appender<Self>>(a, PhantomData))
            } else {
                None
            }
        }
    }

    pub struct OrderMut<B: Builder>(B, PhantomData<B>);

    impl<B: Builder> RootBuilder<B> for OrderMut<B> {
        fn root_ptr(&self) -> *mut u8 {
            self.0.root_ptr()
        }

        fn finish(self) -> Safe<B::Message, B::Allocator> {
            self.0.finish()
        }
    }

    impl<B: Builder> OrderMut<B> {
        #[inline]
        fn layout_ptr(&self) -> &OrderData {
            unsafe { &*((self.0).offset(B::Message::BASE_OFFSET) as *const OrderData) }
        }

        #[inline]
        fn layout_ptr_mut(&mut self) -> &mut OrderData {
            unsafe { &mut *((self.0).offset(B::Message::BASE_OFFSET) as *const OrderData as *mut OrderData) }
        }
    }

    impl<B: Builder> Order for OrderMut<B> {
        type Price = PriceBuilder<B>;

        fn id(&self) -> u64 {
            self.layout_ptr().id()
        }

        fn price(&self) -> Slice<Self::Price> {
            Slice(PriceBuilder::<B>(
                &self.0, B::Message::BASE_OFFSET + 8, PhantomData), PhantomData)
        }

        fn set_id(&mut self, value: u64) -> &mut Self {
            self.layout_ptr_mut().set_id(value);
            self
        }

        fn price_mut(&mut self) -> SliceMut<Self::Price> {
            SliceMut(PriceBuilder::<B>(
                &self.0, B::Message::BASE_OFFSET + 8, PhantomData), PhantomData)
        }
    }

    impl<B: Builder> OrderBuilder for OrderMut<B> {
        type PriceBuilder = PriceBuilder<B>;

        fn set_client_id(&mut self, value: &str) -> &mut Self {
            self
        }
    }

    pub struct OrderMutNested<B: Builder>(*const B, isize, PhantomData<B>);

    impl<B: Builder> OrderMutNested<B> {
        #[inline(always)]
        fn builder_ptr(&self) -> &B {
            unsafe { &*self.0 }
        }

        #[inline(always)]
        fn layout_ptr(&self) -> &OrderData {
            unsafe { &*(self.builder_ptr().offset(self.1) as *const OrderData) }
        }

        #[inline(always)]
        fn layout_ptr_mut(&mut self) -> &mut OrderData {
            unsafe { &mut *(self.builder_ptr().offset(self.1) as *mut OrderData) }
        }
    }

    impl<B: Builder> Order for OrderMutNested<B> {
        type Price = PriceBuilder<B>;

        fn id(&self) -> u64 {
            self.layout_ptr().id()
        }

        fn price(&self) -> Slice<Self::Price> {
            Slice(PriceBuilder::<B>(
                self.0, self.1 + 8, PhantomData),
                  PhantomData)
        }

        fn set_id(&mut self, value: u64) -> &mut Self {
            self.layout_ptr_mut().set_id(value);
            self
        }

        fn price_mut(&mut self) -> SliceMut<Self::Price> {
            SliceMut(PriceBuilder::<B>(
                self.0, self.1 + 8, PhantomData),
                     PhantomData)
        }
    }


    pub struct PriceBuilder<B: Builder>(*const B, isize, PhantomData<B>);

    impl<B: Builder> PriceBuilder<B> {
        #[inline(always)]
        fn builder_ptr(&self) -> &B {
            unsafe { &*self.0 }
        }

        #[inline(always)]
        fn layout_ptr(&self) -> &PriceData {
            unsafe { &*(self.builder_ptr().offset(self.1) as *const PriceData) }
        }

        #[inline(always)]
        fn layout_ptr_mut(&mut self) -> &mut PriceData {
            unsafe { &mut *(self.builder_ptr().offset(self.1) as *mut PriceData) }
        }
    }

    impl<B: Builder> Price for PriceBuilder<B> {
        #[inline]
        fn open(&self) -> f64 {
            self.layout_ptr().open()
        }

        #[inline]
        fn high(&self) -> f64 {
            self.layout_ptr().high()
        }

        #[inline]
        fn low(&self) -> f64 {
            self.layout_ptr().low()
        }

        #[inline]
        fn close(&self) -> f64 {
            self.layout_ptr().close()
        }

        #[inline]
        fn set_open(&mut self, value: f64) -> &mut Self {
            self.layout_ptr_mut().set_open(value);
            self
        }

        #[inline]
        fn set_high(&mut self, value: f64) -> &mut Self {
            self.layout_ptr_mut().set_high(value);
            self
        }

        #[inline]
        fn set_low(&mut self, value: f64) -> &mut Self {
            self.layout_ptr_mut().set_low(value);
            self
        }

        #[inline]
        fn set_close(&mut self, value: f64) -> &mut Self {
            self.layout_ptr_mut().set_close(value);
            self
        }
    }

    ////////////////////////////////////////////////////////////////////////////////////////////////
    // OrderSafe
    ////////////////////////////////////////////////////////////////////////////////////////////////

    pub struct OrderSafe<R: Message> {
        base: *const OrderData,
        _p: PhantomData<R>,
    }

    impl<R: Message> Root for OrderSafe<R> {
        fn new(base: *const u8) -> Self {
            Self { base: unsafe { base as *const OrderData }, _p: PhantomData }
        }

        fn base_ptr(&self) -> *const u8 {
            unsafe { self.base as *const u8 }
        }
    }

    impl<R: Message> Order for OrderSafe<R> {
        // type Price = impl Price;
        type Price = PriceSafe<R>;

        fn id(&self) -> u64 {
            unsafe { (&*(self.base)).id }
        }

        fn price(&self) -> Slice<Self::Price> {
            Slice(PriceSafe::<R>::new(
                unsafe { (self.base as *const u8).offset(8) }),
                  PhantomData)
        }

        fn set_id(&mut self, value: u64) -> &mut Self {
            unsafe { (&mut *(self.base as *mut OrderData)).id = value; }
            self
        }

        fn price_mut(&mut self) -> SliceMut<Self::Price> {
            SliceMut(PriceSafe::<R>::new(
                unsafe { (self.base as *const u8).offset(8) }),
                     PhantomData)
        }
    }

    ////////////////////////////////////////////////////////////////////////////////////////////////
    // OrderUnsafe
    ////////////////////////////////////////////////////////////////////////////////////////////////
    pub struct OrderUnsafe<R: Message> {
        base: *const OrderData,
        bounds: *const u8,
        _p: PhantomData<R>,
    }

    impl<R: Message> RootUnsafe for OrderUnsafe<R> {
        fn new(base: *const u8, bounds: *const u8) -> Self {
            Self { base: unsafe { base as *const OrderData }, bounds, _p: PhantomData }
        }

        fn base_ptr(&self) -> *const u8 {
            unsafe { self.base as *const u8 }
        }
    }

    pub struct OrderNested<R: Message> {
        base: *const OrderData,
        root: *const u8,
        _p: PhantomData<R>,
    }

    pub struct OrderUnsafeNested<R: Message> {
        base: *const OrderData,
        bounds: *const u8,
        root: *const u8,
        _p: PhantomData<R>,
    }

    impl<R: Message> OrderNested<R> {
        fn new(base: *const u8, root: *const u8) -> Self {
            Self { base: unsafe { base as *const OrderData }, root, _p: PhantomData }
        }
    }

    impl<R: Message> OrderUnsafeNested<R> {
        fn new(base: *const u8, bounds: *const u8, root: *const u8) -> Self {
            Self { base: unsafe { base as *const OrderData }, bounds, root, _p: PhantomData }
        }
    }

    pub struct PriceMessage<H: Header>(PhantomData<H>);

    impl<H: Header> Message for PriceMessage<H> {
        type Header = H;
        type Block = H::Block;
        type Layout = PriceData;
        type Root = PriceSafe<Self>;
        type RootUnsafe = PriceUnsafe<Self>;

        const SIZE: usize = mem::size_of::<Self::Layout>();
        const INITIAL_SIZE: usize = H::SIZE as usize + Self::SIZE;
        const BASE_OFFSET: isize = H::SIZE;
        const BASE_SIZE: isize = Self::SIZE as isize;
    }

    impl PriceMessage<Only<PriceData>> {
        pub fn alloc() -> Option<SafeMut<PriceMessage<Only<PriceData>>>> {
            SafeMut::new()
        }

        pub fn new() -> impl Price {
            PriceData::new(0.0, 0.0, 0.0, 0.0)
        }

        #[inline]
        fn copy(from: &impl Price, to: &mut impl Price) {
            to.set_open(from.open())
                .set_high(from.high())
                .set_low(from.low())
                .set_close(from.close());
        }
    }

    pub struct PriceSafe<M: Message>(*const PriceData, PhantomData<M>);

    impl<M: Message> Root for PriceSafe<M> {
        fn new(base: *const u8) -> Self {
            Self(unsafe { base as *const PriceData }, PhantomData)
        }

        fn base_ptr(&self) -> *const u8 {
            unsafe { self.0 as *const u8 }
        }
    }

    impl<M: Message> Price for PriceSafe<M> {
        #[inline(always)]
        fn open(&self) -> f64 {
            unsafe { (&*self.0).open() }
        }

        #[inline(always)]
        fn high(&self) -> f64 {
            unsafe { (&*self.0).high() }
        }

        #[inline(always)]
        fn low(&self) -> f64 {
            unsafe { (&*self.0).low() }
        }

        #[inline(always)]
        fn close(&self) -> f64 {
            unsafe { (&*self.0).close() }
        }

        #[inline(always)]
        fn set_open(&mut self, value: f64) -> &mut Self {
            unsafe { (&mut *(self.0 as *mut PriceData)).set_open(value); }
            self
        }

        #[inline(always)]
        fn set_high(&mut self, value: f64) -> &mut Self {
            unsafe { (&mut *(self.0 as *mut PriceData)).set_open(value); }
            self
        }

        #[inline(always)]
        fn set_low(&mut self, value: f64) -> &mut Self {
            unsafe { (&mut *(self.0 as *mut PriceData)).set_low(value); }
            self
        }

        #[inline(always)]
        fn set_close(&mut self, value: f64) -> &mut Self {
            unsafe { (&mut *(self.0 as *mut PriceData)).set_close(value); }
            self
        }
    }

    pub struct PriceUnsafe<R: Message>(*const u8, *const u8, PhantomData<R>);

    impl<R: Message> RootUnsafe for PriceUnsafe<R> {
        fn new(base: *const u8, bounds: *const u8) -> Self {
            Self(base, bounds, PhantomData)
        }

        fn base_ptr(&self) -> *const u8 {
            unsafe { self.0 as *const u8 }
        }
    }

    impl<M: Message> Price for PriceUnsafe<M> {
        #[inline(always)]
        fn open(&self) -> f64 {
            unsafe {
                if self.0.offset(8) as usize > self.1 as usize { 0f64 } else { (&*(self.0 as *const PriceData)).open() }
            }
        }

        #[inline(always)]
        fn high(&self) -> f64 {
            unsafe {
                if self.0.offset(16) as usize > self.1 as usize { 0f64 } else { (&*(self.0 as *const PriceData)).close() }
            }
        }

        #[inline(always)]
        fn low(&self) -> f64 {
            unsafe {
                if self.0.offset(24) as usize > self.1 as usize { 0f64 } else { (&*(self.0 as *const PriceData)).low() }
            }
        }

        #[inline(always)]
        fn close(&self) -> f64 {
            unsafe {
                if unlikely(self.0.offset(32) as usize > self.1 as usize) { 0f64 } else { (&*(self.0 as *const PriceData)).close() }
            }
        }

        #[inline(always)]
        fn set_open(&mut self, value: f64) -> &mut Self {
            unsafe {
                if self.0.offset(8) as usize > self.1 as usize { self } else {
                    (&mut *(self.0 as *mut PriceData)).set_open(value);
                    self
                }
            }
        }

        #[inline(always)]
        fn set_high(&mut self, value: f64) -> &mut Self {
            unsafe {
                if self.0.offset(16) as usize > self.1 as usize { self } else {
                    (&mut *(self.0 as *mut PriceData)).set_high(value);
                    self
                }
            }
        }

        #[inline(always)]
        fn set_low(&mut self, value: f64) -> &mut Self {
            unsafe {
                if self.0.offset(24) as usize > self.1 as usize { self } else {
                    (&mut *(self.0 as *mut PriceData)).set_low(value);
                    self
                }
            }
        }

        #[inline(always)]
        fn set_close(&mut self, value: f64) -> &mut Self {
            unsafe {
                if self.0.offset(32) as usize > self.1 as usize { self } else {
                    (&mut *(self.0 as *mut PriceData)).set_close(value);
                    self
                }
            }
        }
    }

    #[derive(Copy, Clone, Debug)]
    #[repr(C, packed)]
    pub struct PriceData {
        open: f64,
        high: f64,
        low: f64,
        close: f64,
    }

    impl PriceData {
        pub fn new(open: f64, high: f64, low: f64, close: f64) -> Self {
            Self { open, high, low, close }
        }
    }

    impl Price for PriceData {
        #[inline]
        fn open(&self) -> f64 {
            f64_from_le(self.open)
        }
        #[inline]
        fn high(&self) -> f64 {
            f64_from_le(self.high)
        }
        #[inline]
        fn low(&self) -> f64 {
            f64_from_le(self.low)
        }
        #[inline]
        fn close(&self) -> f64 {
            f64_from_le(self.close)
        }

        #[inline]
        fn set_open(&mut self, value: f64) -> &mut Self {
            self.open = f64_to_le(value);
            self
        }

        #[inline]
        fn set_high(&mut self, value: f64) -> &mut Self {
            self.close = f64_to_le(value);
            self
        }

        #[inline]
        fn set_low(&mut self, value: f64) -> &mut Self {
            self.low = f64_to_le(value);
            self
        }

        #[inline]
        fn set_close(&mut self, value: f64) -> &mut Self {
            self.close = f64_to_le(value);
            self
        }
    }

    fn process_price(price: &mut impl Price) {
        price.set_open(4198.25);
        println!("open: {:}", price.open());
    }

    fn build_order2(o: &mut impl Order) {
        o.price_mut().close();
        o.price_mut().set_close(113.34);
    }

    fn build_order22<O: Order>(o: &mut O) {
        o.price_mut().close();
        o.price_mut().set_close(113.34);
    }

    fn build_order222(o: &impl Order) -> Slice<impl Price> {
        o.price().close();
        o.price()
    }

    fn build_order2222(o: &impl Order) -> Slice<impl Price> {
        o.price().close();
        o.price()
    }

    fn build_order3() -> impl OrderBuilder {
        // let mut o = OrderMessage::new().unwrap();
        let mut o = OrderMessage::new_builder(0).unwrap();
        let mut p = o.price_mut();
        p.open();
        p.set_open(123.45);
        o.price_mut().set_low(11.0);
        o
    }

    fn build_order() -> impl MessageBox<Root = impl Order> {
        // fn build_order() -> Safe<OrderMessage> {
        // let mut o = OrderMessage::new().unwrap();
        let mut o = OrderMessage::new_builder(0).unwrap();

        o.set_id(234);
        // build_order2(o.deref_mut());
        build_order2(&mut o);

        let mut price = o.price_mut();
        price.set_open(133.34);
        price.set_low(11.33);
        o.price_mut();

        o.price_mut().set_close(34.44);
        o.price().open();

        let o = o.finish();

        println!("open: {:}", o.price().open());
        o.price();
        o
    }

    #[test]
    fn root_message() {
        let o = build_order();
        o.price();

        let o = OrderMessage::new().unwrap();
        println!("size: {:}", o.size());
        println!("capacity: {:}", o.capacity());

        // let (mut p, mut o) = o.price_mut();
        // p.set_high(11.0);
        // o.price().high();
        // o.price_mut().0.set_open(10.0);
    }

    fn build() {
        let mut order = OrderMessage::new_builder(0).unwrap();
        order.set_client_id("12345");

        let pa = PriceMessage::alloc().unwrap();
        println!("PriceMessage size: {:}", pa.size());
        let mut p = PriceMessage::new();
        p.set_open(11.0);

        let mut p2 = PriceMessage::new();
        PriceMessage::copy(&p, &mut p2);
    }

    // fn return_price<'a>(o: &'a impl Order<'a>) -> impl Price {
    //     o.price()
    // }

    #[test]
    fn message() {
        build();
        println!("done");

        #[repr(C, packed)]
        union Union {
            x: u64,
            y: u64,
            z: [u8; 9],
            zz: [u8; 13],
        }

        struct Struct {
            u: Union,
        }

        impl Union {
            pub fn x(&self) -> u64 {
                unsafe { self.x }
            }
        }

        println!("{:}", mem::size_of::<Union>())
    }
}