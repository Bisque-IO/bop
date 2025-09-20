pub trait Block {
    type Size;

    // The size of the block header exposed to used blocks is the size field.
    // The prev_phys field is stored *inside* the previous free block.
    const OVERHEAD: isize;
    const DATA_OFFSET: isize;
    const MIN_SIZE: isize;

    const SIZE_LIMIT: usize;
    const SIZE_BYTES: isize;
    const SIZE_BYTES_USIZE: usize;

    fn size(&self) -> Self::Size;
    fn size_usize(&self) -> usize;
    fn prev_phys(&self) -> Self::Size;
    fn next_free(&self) -> Self::Size;
    fn prev_free(&self) -> Self::Size;
    fn is_free(&self) -> bool;
    fn is_prev_free(&self) -> bool;

    fn set_size(&mut self, size: Self::Size);
    fn set_size_usize(&mut self, size: usize);
    fn set_used(&mut self);
    fn set_free(&mut self);
    fn set_prev_free(&mut self);
    fn set_prev_used(&mut self);

    fn offset(base: *const u8, block: *mut Self) -> isize;
    fn to_ptr(&self) -> *const u8;
    fn data_ptr(&self) -> *const u8;
    fn from_ptr<'s>(p: *const u8) -> &'s mut Self;

    fn get_size_value(p: *const u8) -> usize;
    fn get_size_value_mut(p: *mut u8) -> usize;
    fn put_size_value(p: *mut u8, size: Self::Size);
    fn put_size_value_usize(p: *mut u8, size: usize);
}

struct Block_<S> {
    prev_phys: S,
    size: S,
    next_free: S,
    prev_free: S,
}

impl<S> Block_<S> {
    // Since block sizes are always at least a multiple of 4, the two least
    // significant bits of the size field are used to store the block status:
    // - bit 0: whether block is used or free
    // - bit 1: whether previous block is used or free
    // const FREE_BIT: S = S::shift_left(S::from(1), S::from(0));
    // const PREV_FREE_BIT: S = S::shift_left(S::from(1), S::from(1));
}

pub struct Block16 {
    prev_phys: u16,
    size: u16,
    next_free: u16,
    prev_free: u16,
}

pub struct Block32 {
    prev_phys: u32,
    size: u32,
    next_free: u32,
    prev_free: u32,
}

pub struct Block64 {
    prev_phys: u64,
    size: u64,
    next_free: u64,
    prev_free: u64,
}

impl Block16 {
    // Since block sizes are always at least a multiple of 4, the two least
    // significant bits of the size field are used to store the block status:
    // - bit 0: whether block is used or free
    // - bit 1: whether previous block is used or free
    const FREE_BIT: u16 = 1 << 0;
    const PREV_FREE_BIT: u16 = 1 << 1;
    const CLEAR: u16 = !(Self::FREE_BIT | Self::PREV_FREE_BIT);
}

impl Block for Block16 {
    type Size = u16;

    // The size of the block header exposed to used blocks is the size field.
    // The prev_phys field is stored *inside* the previous free block.
    const OVERHEAD: isize = size_of::<Self::Size>() as isize;
    const DATA_OFFSET: isize = (size_of::<Self::Size>() * 2) as isize;
    const MIN_SIZE: isize = (size_of::<Self::Size>() * 3) as isize;

    const SIZE_LIMIT: usize = u16::MAX as usize;


    const SIZE_BYTES: isize = 2;
    const SIZE_BYTES_USIZE: usize = 2;

    #[inline(always)]
    fn size(&self) -> u16 {
        self.size & !(Block16::FREE_BIT | Block16::PREV_FREE_BIT)
    }

    #[inline(always)]
    fn size_usize(&self) -> usize {
        self.size() as usize
    }

    #[inline(always)]
    fn prev_phys(&self) -> u16 {
        self.prev_phys
    }

    #[inline(always)]
    fn next_free(&self) -> u16 {
        self.next_free
    }

    #[inline(always)]
    fn prev_free(&self) -> u16 {
        self.prev_free
    }

    #[inline(always)]
    fn is_free(&self) -> bool {
        self.size & Block16::FREE_BIT != 0
    }

    #[inline(always)]
    fn is_prev_free(&self) -> bool {
        self.size & Block16::PREV_FREE_BIT != 0
    }

    #[inline(always)]
    fn set_size(&mut self, size: u16) {
        self.size = size | (self.size & (Block16::FREE_BIT | Block16::PREV_FREE_BIT));
    }

    #[inline(always)]
    fn set_size_usize(&mut self, size: usize) {
        self.set_size(size as u16);
    }

    #[inline(always)]
    fn set_used(&mut self) {
        self.size &= !Block16::FREE_BIT;
    }

    #[inline(always)]
    fn set_free(&mut self) {
        self.size |= Block16::FREE_BIT;
    }

    #[inline(always)]
    fn set_prev_free(&mut self) {
        self.size |= Block16::PREV_FREE_BIT;
    }

    #[inline(always)]
    fn set_prev_used(&mut self) {
        self.size &= !Block16::PREV_FREE_BIT;
    }

    #[inline(always)]
    fn offset(base: *const u8, block: *mut Self) -> isize {
        unsafe { (block as *mut u8).offset(Self::OVERHEAD) as isize - base as isize }
    }

    #[inline(always)]
    fn to_ptr(&self) -> *const u8 {
        unsafe { (self as *const Self as *const u8).offset(Self::SIZE_BYTES) }
    }

    #[inline(always)]
    fn data_ptr(&self) -> *const u8 {
        unsafe { (self as *const Self as *const u8).offset(Self::DATA_OFFSET) }
    }

    #[inline(always)]
    fn from_ptr<'s>(p: *const u8) -> &'s mut Self {
        unsafe { &mut *(p.offset(-Self::SIZE_BYTES) as *mut u8 as *mut Self) }
    }

    #[inline(always)]
    fn get_size_value(p: *const u8) -> usize {
        unsafe { Self::Size::from_le(*(p as *const Self::Size)) as usize }
    }

    #[inline(always)]
    fn get_size_value_mut(p: *mut u8) -> usize {
        unsafe { Self::Size::from_le(*(p as *const Self::Size)) as usize }
    }

    #[inline(always)]
    fn put_size_value(p: *mut u8, size: Self::Size) {
        unsafe { *(p as *mut Self::Size) = size.to_le(); }
    }

    #[inline(always)]
    fn put_size_value_usize(p: *mut u8, size: usize) {
        unsafe { *(p as *mut Self::Size) = (size as Self::Size).to_le(); }
    }
}

impl Block32 {
    // Since block sizes are always at least a multiple of 4, the two least
    // significant bits of the size field are used to store the block status:
    // - bit 0: whether block is used or free
    // - bit 1: whether previous block is used or free
    const FREE_BIT: u32 = 1 << 0;
    const PREV_FREE_BIT: u32 = 1 << 1;
    const CLEAR: u32 = !(Self::FREE_BIT | Self::PREV_FREE_BIT);
}

impl Block for Block32 {
    type Size = u32;

    // The size of the block header exposed to used blocks is the size field.
    // The prev_phys field is stored *inside* the previous free block.
    const OVERHEAD: isize = core::mem::size_of::<u32>() as isize;
    const DATA_OFFSET: isize = (core::mem::size_of::<u32>() * 2) as isize;
    const MIN_SIZE: isize = (core::mem::size_of::<Self::Size>() * 3) as isize;

    const SIZE_LIMIT: usize = u32::MAX as usize;
    const SIZE_BYTES: isize = 4;
    const SIZE_BYTES_USIZE: usize = 4;

    #[inline(always)]
    fn size(&self) -> u32 {
        self.size & !(Block32::FREE_BIT | Block32::PREV_FREE_BIT)
    }

    #[inline(always)]
    fn size_usize(&self) -> usize {
        self.size() as usize
    }

    #[inline(always)]
    fn prev_phys(&self) -> u32 {
        self.prev_phys
    }

    #[inline(always)]
    fn next_free(&self) -> u32 {
        self.next_free
    }

    #[inline(always)]
    fn prev_free(&self) -> u32 {
        self.prev_free
    }

    #[inline(always)]
    fn is_free(&self) -> bool {
        self.size & Block32::FREE_BIT != 0
    }

    #[inline(always)]
    fn is_prev_free(&self) -> bool {
        self.size & Block32::PREV_FREE_BIT != 0
    }

    #[inline(always)]
    fn set_size(&mut self, size: u32) {
        self.size = size | (self.size & (Block32::FREE_BIT | Block32::PREV_FREE_BIT));
    }

    #[inline(always)]
    fn set_size_usize(&mut self, size: usize) {
        self.set_size(size as u32);
    }

    #[inline(always)]
    fn set_used(&mut self) {
        self.size &= !Block32::FREE_BIT;
    }

    #[inline(always)]
    fn set_free(&mut self) {
        self.size |= Block32::FREE_BIT;
    }

    #[inline(always)]
    fn set_prev_free(&mut self) {
        self.size |= Block32::PREV_FREE_BIT;
    }

    #[inline(always)]
    fn set_prev_used(&mut self) {
        self.size &= !Block32::PREV_FREE_BIT;
    }

    #[inline(always)]
    fn offset(base: *const u8, block: *mut Self) -> isize {
        unsafe { (block as *mut u8).offset(Self::OVERHEAD) as isize - base as isize }
    }

    #[inline(always)]
    fn to_ptr(&self) -> *const u8 {
        unsafe { (self as *const Self as *const u8).offset(Self::SIZE_BYTES) }
    }

    #[inline(always)]
    fn data_ptr(&self) -> *const u8 {
        unsafe { (self as *const Self as *const u8).offset(Self::DATA_OFFSET) }
    }

    #[inline(always)]
    fn from_ptr<'s>(p: *const u8) -> &'s mut Self {
        unsafe { &mut *(p.offset(-Self::SIZE_BYTES) as *mut u8 as *mut Self) }
    }

    #[inline(always)]
    fn get_size_value(p: *const u8) -> usize {
        unsafe { Self::Size::from_le(*(p as *const Self::Size)) as usize }
    }

    #[inline(always)]
    fn get_size_value_mut(p: *mut u8) -> usize {
        unsafe { Self::Size::from_le(*(p as *const Self::Size)) as usize }
    }

    #[inline(always)]
    fn put_size_value(p: *mut u8, size: Self::Size) {
        unsafe { *(p as *mut Self::Size) = size.to_le(); }
    }

    #[inline(always)]
    fn put_size_value_usize(p: *mut u8, size: usize) {
        unsafe { *(p as *mut Self::Size) = (size as Self::Size).to_le(); }
    }
}
