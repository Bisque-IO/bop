use core::mem;
use core::ptr::read_unaligned;

//
pub struct Rand {
    seed: u64,
}

impl Rand {
    #[inline(always)]
    pub fn new(seed: u64) -> Self {
        Rand { seed }
    }

    pub fn seed(&self) -> u64 {
        return self.seed;
    }

    #[inline(always)]
    pub fn next(&mut self) -> u64 {
        self.seed = self.seed * 0xa0761d6478bd642fu64;
        mix(self.seed, self.seed ^ 0xe7037ed1a0b428dbu64)
    }

    #[inline(always)]
    pub fn next_float(&mut self) -> f64 {
        compute_float(self.next())
    }

    #[inline(always)]
    pub fn next_gaussian(&mut self) -> f64 {
        compute_gaussian(self.next())
    }
}

const DEFAULT_SEED: u64 = 0xa0761d6478bd642f;
const S1: u64 = 0xe7037ed1a0b428db;
const S2: u64 = 0x8ebc6af09c88c6e3;
const S3: u64 = 0x589965cc75374cc3;
const S4: u64 = 0x1d8e4e27c47d124f;

#[inline(always)]
pub fn mix(x: u64, y: u64) -> u64 {
    // let hi = (r >> 64u128 & 0xFFFFFFFFFFFFFFFFu128) as u64;
    // let lo = (r & 0xFFFFFFFFFFFFFFFFu128) as u64;
    // let (hi, lo) = x.widening_mul(y);
    // hi ^ lo
    // let r = u128::from(x).mul(u128::from(y));
    let r = u128::from(x) * u128::from(y);
    ((r >> 64) ^ r) as u64
}

#[inline(always)]
pub fn compute_rand(seed: *mut u64) -> u64 {
    unsafe {
        *seed = *seed * 0xa0761d6478bd642fu64;
        mix(*seed, *seed ^ 0xe7037ed1a0b428dbu64)
    }
}

#[inline(always)]
pub fn compute_float(r: u64) -> f64 {
    static NORM: f64 = 1.0f64 / (1u64 << 52u64) as f64;
    (r >> 12) as f64 * NORM
}

#[inline(always)]
pub fn compute_gaussian(r: u64) -> f64 {
    static NORM: f64 = 1.0f64 / (1u64 << 20u64) as f64;
    ((r & 0x1fffffu64) + ((r >> 21u64) & 0x1fffffu64) + ((r >> 42u64) & 0x1fffffu64)) as f64 * NORM - 3.0f64
}

#[inline(always)]
fn read32(b: *const u8) -> u64 {
    u64::from(u32::from_le(unsafe { read_unaligned(b as *const u32) }))
    // u32::from_le(unsafe { *(b as *const u32) }) as u64
}

#[inline(always)]
fn read64(b: *const u8) -> u64 {
    u64::from_le(unsafe { read_unaligned(b as *const u64) })
}

#[inline(always)]
fn hash_bool(v: bool) -> u64 {
    if v { hash_u8(1) } else { hash_u8(0) }
}

#[inline(always)]
pub fn hash_i8(v: i8) -> u64 {
    hash_u8(unsafe { *(&v as *const i8 as *const u8) })
}

#[inline(always)]
pub fn hash_u8(v: u8) -> u64 {
    // unsafe { hash(&v as *const u8, 1, DEFAULT_SEED)}
    let v = v as u64;
    mix(S1 ^ 1u64, mix(((v << 16) | (v << 8) | v) ^ S1, 0 ^ DEFAULT_SEED))
}

#[inline(always)]
pub fn hash_i16(v: i16) -> u64 {
    hash_u16(unsafe { *(&v as *const i16 as *const u16) })
}

#[inline(always)]
pub fn hash_u16(v: u16) -> u64 {
    // unsafe { hash(&v as *const u16 as *const u8, 2, DEFAULT_SEED)}
    unsafe {
        let mut v = v.to_le();
        let bytes = &v as *const u16 as *const u8;
        let a = ((*bytes as u64) << 16) |
            ((*bytes.offset(1) as u64) << 8) |
            (*bytes.offset(1) as u64);
        mix(S1 ^ 2u64, mix(a ^ S1, 0 ^ DEFAULT_SEED))
    }
}

#[inline(always)]
pub fn hash_i32(v: i32) -> u64 {
    hash_u32(unsafe { *(&v as *const i32 as *const u32) })
}

#[inline(always)]
pub fn hash_u32(v: u32) -> u64 {
    let mut v = (v as u64).to_le();
    v = (v << 32) | v;
    mix(S1 ^ 4u64, mix(v ^ S1, v ^ DEFAULT_SEED))
    // unsafe { hash(&v as *const u32 as *const u8, 4, DEFAULT_SEED)}
}

#[inline(always)]
pub fn hash_i64(v: i64) -> u64 {
    hash_u64(unsafe { *(&v as *const i64 as *const u64) })
}

#[inline(always)]
pub fn hash_u64(v: u64) -> u64 {
    // unsafe {
    //     let bytes = &v as *const u64 as *const u8;
    //     let a = read32(bytes) << 32 | read32(bytes.offset(((8 >> 3) << 2) as isize));
    //     let b = read32(bytes.offset((8 - 4) as isize)) << 32 |
    //         read32(bytes.offset((8 - 4 - ((8 >> 3) << 2)) as isize));
    //     mix(S1^8u64, mix(a^S1, b^DEFAULT_SEED))
    // }

    let v = v.to_le();
    unsafe { hash(&v as *const u64 as *const u8, 8, DEFAULT_SEED) }
    // let v = v.to_le();
    // mix(S1 ^ 8u64, mix(
    //     (v << 32u64 | (v >> 32u64 & 0xFFFFFFFFu64)) ^ S1,
    //     (v >> 32u64 | (v & 0xFFFFFFFFu64)) ^ DEFAULT_SEED))
}

#[inline(always)]
pub fn hash_i128(v: i128) -> u64 {
    hash_u128(unsafe { *(&v as *const i128 as *const u128) })
}

#[inline(always)]
pub fn hash_u128(v: u128) -> u64 {
    let mut v = v.to_le();
    unsafe { hash(&v as *const u128 as *const u8, 16, DEFAULT_SEED) }
    // unsafe {
    //     let p = &v as *const u128 as *const u8;
    //     let a = read32(p) << 32 | read32(p.offset(8));
    //     let b = read32(p.offset(12)) << 32 |
    //         read32(p.offset(4));
    //     mix(S1 ^ 16u64, mix(a ^ S1, b ^ DEFAULT_SEED))
    // }
}

pub trait Hasher: Eq {
    fn hash_wap(&self) -> u64;
}

impl Hasher for i8 {
    #[inline(always)]
    fn hash_wap(&self) -> u64 {
        hash_i8(*self)
    }
}

impl Hasher for i16 {
    #[inline(always)]
    fn hash_wap(&self) -> u64 {
        hash_i16(*self)
    }
}

impl Hasher for i32 {
    #[inline(always)]
    fn hash_wap(&self) -> u64 {
        hash_i32(*self)
    }
}

impl Hasher for i64 {
    #[inline(always)]
    fn hash_wap(&self) -> u64 {
        hash_i64(*self)
    }
}

impl Hasher for i128 {
    #[inline(always)]
    fn hash_wap(&self) -> u64 {
        hash_i128(*self)
    }
}

impl Hasher for u8 {
    #[inline(always)]
    fn hash_wap(&self) -> u64 {
        hash_u8(*self)
    }
}

impl Hasher for u16 {
    #[inline(always)]
    fn hash_wap(&self) -> u64 {
        hash_u16(*self)
    }
}

impl Hasher for u32 {
    #[inline(always)]
    fn hash_wap(&self) -> u64 {
        hash_u32(*self)
    }
}

impl Hasher for u64 {
    #[inline(always)]
    fn hash_wap(&self) -> u64 {
        hash_u64(*self)
    }
}

impl Hasher for u128 {
    #[inline(always)]
    fn hash_wap(&self) -> u64 {
        hash_u128(*self)
    }
}

impl Hasher for &str {
    #[inline(always)]
    fn hash_wap(&self) -> u64 {
        unsafe { hash(self.as_ptr(), self.len() as u64, DEFAULT_SEED) }
    }
}

impl Hasher for String {
    #[inline(always)]
    fn hash_wap(&self) -> u64 {
        unsafe { hash(self.as_ptr(), self.len() as u64, DEFAULT_SEED) }
    }
}

impl Hasher for &[u8] {
    #[inline(always)]
    fn hash_wap(&self) -> u64 {
        unsafe { hash(self.as_ptr(), self.len() as u64, DEFAULT_SEED) }
    }
}

impl Hasher for Vec<u8> {
    #[inline(always)]
    fn hash_wap(&self) -> u64 {
        unsafe { hash(self.as_ptr(), self.len() as u64, DEFAULT_SEED) }
    }
}

pub fn hash_default(data: *const u8, length: u64) -> u64 {
    unsafe { hash(data, length, DEFAULT_SEED) }
}

pub fn hash_sized<T: Sized>(data: &T) -> u64 {
    unsafe {
        hash(unsafe { data as *const T as *const u8 },
             mem::size_of::<T>() as u64, DEFAULT_SEED)
    }
}

// impl<T: Sized> Hasher for T {
//     fn hash_wap(self) -> u64 {
//         unsafe { hash(unsafe { &data as *const T as *const u8 },
//                       mem::size_of::<T>() as u64, DEFAULT_SEED) }
//     }
// }
//
// impl<T: Sized> Hasher for &T {
//     fn hash_wap(self) -> u64 {
//         unsafe { hash(unsafe { self as *const T as *const u8 },
//                       mem::size_of::<T>() as u64, DEFAULT_SEED) }
//     }
// }

pub unsafe fn hash(data: *const u8, length: u64, seed: u64) -> u64 {
    let mut a: u64 = 0;
    let mut b: u64 = 0;
    let mut bytes = data;
    let mut seed = seed;

    if length <= 16 {
        if length >= 4 {
            unsafe {
                a = read32(bytes) << 32 | read32(bytes.offset(((length >> 3) << 2) as isize));
                b = read32(bytes.offset((length - 4) as isize)) << 32 |
                    read32(bytes.offset((length - 4 - ((length >> 3) << 2)) as isize))
            }
        } else {
            unsafe {
                a = ((*bytes as u64) << 16) |
                    ((*bytes.offset((length >> 1) as isize) as u64) << 8) |
                    ((*bytes.offset((length - 1) as isize)) as u64);
            }
        }
    } else {
        let mut index = length;
        if length > 48 {
            let mut see1 = seed;
            let mut see2 = seed;
            while index > 48 {
                unsafe {
                    seed = mix(read64(bytes) ^ S1, read64(bytes.offset(8)) ^ seed);
                    see1 = mix(read64(bytes.offset(16)) ^ S2, read64(bytes.offset(24)) ^ see1);
                    see2 = mix(read64(bytes.offset(32)) ^ S3, read64(bytes.offset(40)) ^ see2);
                    bytes = bytes.offset(48);
                    index -= 48;
                }
            }
            seed ^= see1 ^ see2;
        }
        while index > 16 {
            unsafe {
                seed = mix(read64(bytes) ^ S1, read64(bytes.offset(8)) ^ seed);
                index -= 16;
                bytes = bytes.offset(16);
            }
        }

        unsafe {
            a = read64(bytes.offset((index - 16) as isize));
            b = read64(bytes.offset((index - 8) as isize));
        }
    }
    mix(S1 ^ length, mix(a ^ S1, b ^ seed))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    // use criterion::Bencher;
    use super::*;
    use test::{Bencher, black_box};

    fn print_hash(s: &str) {
        println!("{}: {}", s, unsafe { hash(s.as_ptr(), s.len() as u64, DEFAULT_SEED) });
    }

    #[inline(always)]
    pub fn hash_u8_slow(v: u8) -> u64 {
        unsafe {
            let p = &v as *const u8 as *const u8;
            hash(p, 1, DEFAULT_SEED)
        }
    }

    #[inline(always)]
    pub fn hash_u16_slow(v: u16) -> u64 {
        unsafe {
            let p = &v as *const u16 as *const u8;
            hash(p, 2, DEFAULT_SEED)
        }
    }

    #[inline(always)]
    pub fn hash_u32_slow(v: u32) -> u64 {
        unsafe {
            let p = &v as *const u32 as *const u8;
            hash(p, 4, DEFAULT_SEED)
        }
    }

    #[inline(always)]
    pub fn hash_u64_slow(v: u64) -> u64 {
        unsafe {
            let p = &v as *const u64 as *const u8;
            hash(p, 8, DEFAULT_SEED)
        }
    }

    #[inline(always)]
    pub fn hash_u128_slow(v: u128) -> u64 {
        unsafe {
            let p = &v as *const u128 as *const u8;
            hash(p, 16, DEFAULT_SEED)
        }
    }

    #[test]
    fn u64_slow_vs_fast() {
        println!("{:}", ((4 >> 3) << 2) as u64);
        println!("{:}", 2 >> 1 as u64);
        println!("{:}", 4 - 4 - ((4 >> 3) << 2));
        println!("slow 8:   {:}", hash_u8_slow(10));
        println!("fast 8:   {:}", hash_u8(10));
        println!("slow 16:  {:}", hash_u16_slow(10));
        println!("fast 16:  {:}", hash_u16(10));
        println!("slow 32:  {:}", hash_u32_slow(4200000000));
        println!("fast 32:  {:}", hash_u32(4200000000));
        println!("slow 64:  {:}", hash_u64_slow(10));
        println!("fast 64:  {:}", hash_u64(10));
        println!("slow 128: {:}", hash_u128_slow(10));
        println!("fast 128: {:}", hash_u128(10));
    }

    #[test]
    fn test_mix() {
        /*
        55000000000
5494308139885387899
5345491536366109532
h: 11372757148569077325
he: 3477181882470878621
hel: 205587233836623031
hell: 3508237342570683138
hello: 18063072054746964485
hellonow: 9731530904021028703

55000000000
5169300521685052069
16238366866855921188
h: 6780574694009273394
he: 8834066196113654930
hel: 12533521325614852376
hell: 12299660955645589691
hello: 17947492129529633476
hellonow: 5191492153603243908

55000000000
5494308139885387899
5345491536366109532
h: 11372757148569077325
he: 3477181882470878621
hel: 205587233836623031
hell: 3508237342570683138
hello: 18063072054746964485
hellonow: 9731530904021028703
         */
        println!("{:}", mix(5000000000, 11));
        println!("{:}", hash_u64(10));
        println!("{:}", hash_u64(11));
        print_hash("h");
        print_hash("he");
        print_hash("hel");
        print_hash("hell");
        print_hash("hello");
        print_hash("hellonow");
    }

    #[bench]
    fn bench_u64(b: &mut Bencher) {
        let mut rand = Rand::new(DEFAULT_SEED);

        // test::black_box(|| {
        //     b.iter(|| {
        //         "hello".wap_hash();
        //     });
        // });

        black_box(hash);
        black_box(hash_u8);
        black_box(hash_u16);
        black_box(hash_u32);
        // black_box(hash_u64);

        let xx = AtomicU64::new(0);

        b.iter(move || {
            let mut h = 0;
            for i in 1u64..1000u64 {
                h += std::hint::black_box(hash_u64(i * i));
                // h += hash_u64(i * i);
            }
        });
    }
}