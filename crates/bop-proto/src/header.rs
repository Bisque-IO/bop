use std::marker::PhantomData;
use std::mem;

use crate::block;
use crate::block::{Block, Block16};

/// Header is the small structure before the base type that provides
/// the size and base size at minimum.
pub trait Header: Sized {
    type Block: Block;

    const SIZE: isize;

    /// Size of message in bytes.
    fn size(&self) -> usize;

    // Set the message size.
    fn set_size(&mut self, size: usize) -> &mut Self;

    // The size of the base struct.
    fn base_size(&self) -> usize;

    // Set the base struct size.
    fn set_base_size(&mut self, size: usize) -> &mut Self;

    // Dereference the size from the raw message.
    unsafe fn get_size(p: *const u8) -> usize;

    // Set the size in the raw message.
    unsafe fn put_size(p: *const u8, size: usize);

    unsafe fn put_size_value(p: *const u8, size: usize);

    unsafe fn get_base_size(p: *const u8) -> usize;

    unsafe fn init(p: *mut u8, size: usize, base_size: usize) -> *mut u8 {
        (unsafe { &mut *(p as *mut Self) }).
            set_size(size).
            set_base_size(base_size);
        p
    }
}

/// Only represents a type that has no header and is exactly the size of T.
pub struct Only<T: Sized> {
    _phantom: PhantomData<T>,
}

impl<T: Sized> Header for Only<T> {
    type Block = Block16;

    const SIZE: isize = mem::size_of::<T>() as isize;

    fn size(&self) -> usize {
        mem::size_of::<T>()
    }

    fn set_size(&mut self, _size: usize) -> &mut Self {
        self
    }

    fn base_size(&self) -> usize {
        mem::size_of::<T>()
    }

    fn set_base_size(&mut self, _size: usize) -> &mut Self {
        self
    }

    unsafe fn get_size(p: *const u8) -> usize {
        mem::size_of::<T>()
    }

    unsafe fn put_size(p: *const u8, _size: usize) {}

    unsafe fn put_size_value(p: *const u8, _size: usize) {}

    unsafe fn get_base_size(p: *const u8) -> usize {
        mem::size_of::<T>()
    }

    unsafe fn init(p: *mut u8, _size: usize, _base_size: usize) -> *mut u8 {
        p
    }
}

pub trait Header16: Header {}

pub trait Header32 {}

pub trait Header64 {}

#[repr(C, packed)]
pub struct Fixed16 {
    size: u16,
}

impl Header16 for Fixed16 {}

impl Header for Fixed16 {
    type Block = block::Block16;

    const SIZE: isize = mem::size_of::<Self>() as isize;

    fn size(&self) -> usize {
        u16::from_le(self.size) as usize
    }

    fn set_size(&mut self, size: usize) -> &mut Self {
        self.size = (size as u16).to_le();
        self
    }

    fn base_size(&self) -> usize {
        self.size()
    }

    fn set_base_size(&mut self, _size: usize) -> &mut Self {
        self
    }

    #[inline(always)]
    unsafe fn get_size(p: *const u8) -> usize {
        Self::Block::get_size_value(p)
    }

    #[inline(always)]
    unsafe fn put_size(p: *const u8, size: usize) {
        Self::Block::put_size_value_usize(unsafe { p as *mut u8 }, size);
    }

    #[inline(always)]
    unsafe fn put_size_value(p: *const u8, size: usize) {
        Self::Block::put_size_value_usize(unsafe { p as *mut u8 }, size);
    }

    #[inline(always)]
    unsafe fn get_base_size(p: *const u8) -> usize {
        Self::get_size(p)
    }
}

#[repr(C, packed)]
pub struct Flex16 {
    size: u16,
    base_size: u16,
}

impl Header16 for Flex16 {}

impl Header for Flex16 {
    type Block = Block16;

    const SIZE: isize = mem::size_of::<Self>() as isize;

    fn size(&self) -> usize {
        u16::from_le(self.size) as usize
    }

    fn set_size(&mut self, size: usize) -> &mut Self {
        self.size = (size as u16).to_le();
        self
    }

    fn base_size(&self) -> usize {
        u16::from_le(self.base_size) as usize
    }

    fn set_base_size(&mut self, size: usize) -> &mut Self {
        self.base_size = (size as u16).to_le();
        self
    }

    #[inline(always)]
    unsafe fn get_size(p: *const u8) -> usize {
        Self::Block::get_size_value(p)
    }

    #[inline(always)]
    unsafe fn put_size(p: *const u8, size: usize) {
        Self::Block::put_size_value_usize(p as *mut u8, size);
    }

    #[inline(always)]
    unsafe fn put_size_value(p: *const u8, size: usize) {
        Self::Block::put_size_value_usize(unsafe { p as *mut u8 }, size);
    }

    #[inline(always)]
    unsafe fn get_base_size(p: *const u8) -> usize {
        Self::get_size(p)
    }
}
