#[cfg(test)]
mod tests {
    // use std::ptr;
    // // use test::Bencher;

    // use crate::data::tests::DMaybe::Safe;
    // use crate::hash::{hash_i64, Rand};

    // trait Access {
    //     fn base() -> *const u8;
    // }

    // pub trait VariantVisitor<X, Y> {
    //     fn visit_x(x: &X);
    //     fn visit_y(y: &Y);
    // }

    // pub enum Variant<X, Y> {
    //     X(X),
    //     Y(Y),
    // }

    // impl<X, Y> Variant<X, Y> {
    //     #[inline(always)]
    //     pub fn visit<F1: FnOnce(&X), F2: FnOnce(&Y)>(&self, f1: F1, f2: F2) {
    //         match self {
    //             Self::X(x) => {
    //                 f1(x);
    //             }
    //             Self::Y(y) => {
    //                 f2(y);
    //             }
    //         }
    //     }

    //     pub fn visit0<T: VariantVisitor<X, Y>>(&self) {
    //         match self {
    //             Variant::X(x) => T::visit_x(x),
    //             Variant::Y(y) => T::visit_y(y)
    //         }
    //     }
    // }

    // // type AType = impl A;
    // pub type BType = impl B;
    // pub type CType = impl C;
    // pub type ImplD = impl D;

    // pub trait A {
    //     type B: B;
    //     fn b(&self) -> Self::B;

    //     fn print(&self);

    //     fn maybe_d(&self) -> DMaybe;
    // }

    // pub trait B {
    //     type C: C;

    //     fn c(&self) -> Self::C;

    //     fn print(&self);
    // }

    // pub trait C {
    //     type D: D;
    //     fn d(&self) -> Self::D;
    //     fn print(&self);
    // }

    // pub trait D where Self: Sized {
    //     fn x(&self) where Self: Sized;
    //     fn print(&self) where Self: Sized;
    // }

    // pub struct ASafe {
    //     p: *const u8,
    // }

    // impl ASafe {
    //     pub fn new_variant(&self) -> Variant<BSafe, BUnsafe> {
    //         Variant::X(BSafe::new())
    //     }
    // }

    // pub struct AUnsafe {
    //     p: *const u8,
    //     e: *const u8,
    // }

    // pub struct BSafe {
    //     p: *const u8,
    // }

    // pub struct BUnsafe {
    //     p: *const u8,
    //     e: *const u8,
    // }

    // pub struct CSafe {
    //     p: *const u8,
    // }

    // pub struct CUnsafe {
    //     p: *const u8,
    //     e: *const u8,
    // }

    // pub struct DSafe {
    //     p: *const u8,
    // }

    // pub struct DUnsafe {
    //     p: *const u8,
    //     e: *const u8,
    // }

    // pub enum DMaybe {
    //     Safe(DSafe),
    //     Unsafe(DUnsafe),
    // }

    // impl ASafe {
    //     fn new() -> Self {
    //         Self { p: ptr::null() }
    //     }

    //     fn new_b(&self) -> DMaybe {
    //         DMaybe::Safe(DSafe::new())
    //     }
    // }

    // impl AUnsafe {
    //     fn new() -> Self {
    //         Self { p: ptr::null(), e: ptr::null() }
    //     }
    // }

    // impl BSafe {
    //     fn new() -> Self {
    //         Self { p: ptr::null() }
    //     }
    // }

    // impl BUnsafe {
    //     fn new() -> Self {
    //         Self { p: ptr::null(), e: ptr::null() }
    //     }
    // }

    // impl CSafe {
    //     fn new() -> Self {
    //         Self { p: ptr::null() }
    //     }
    // }

    // impl CUnsafe {
    //     fn new() -> Self {
    //         Self { p: ptr::null(), e: ptr::null() }
    //     }
    // }

    // impl DSafe {
    //     fn new() -> Self {
    //         Self { p: ptr::null() }
    //     }
    // }

    // impl DUnsafe {
    //     fn new() -> Self {
    //         Self { p: ptr::null(), e: ptr::null() }
    //     }
    // }

    // impl A for ASafe {
    //     type B = BSafe;

    //     fn b(&self) -> Self::B {
    //         Self::B::new()
    //     }

    //     fn print(&self) {
    //         println!("A Safe");
    //     }

    //     fn maybe_d(&self) -> DMaybe {
    //         if self.p == ptr::null() {
    //             let s = unsafe { &mut *(self as *const Self as *mut Self) };
    //             s.p = unsafe { self as *const Self as *const u8 };
    //             DMaybe::Unsafe(DUnsafe::new())
    //         } else {
    //             let s = unsafe { &mut *(self as *const Self as *mut Self) };
    //             s.p = ptr::null();
    //             DMaybe::Safe(DSafe::new())
    //         }
    //     }
    // }

    // impl A for AUnsafe {
    //     type B = BType;

    //     fn b(&self) -> Self::B {
    //         BUnsafe::new()
    //         // if self.p == ptr::null() {
    //         //     BUnsafe::new()
    //         // } else {
    //         //     BSafe::new()
    //         // }
    //     }

    //     fn print(&self) {
    //         println!("A Unsafe");
    //     }

    //     fn maybe_d(&self) -> DMaybe {
    //         DMaybe::Safe(DSafe::new())
    //     }
    // }

    // impl B for BSafe {
    //     type C = CSafe;

    //     fn c(&self) -> Self::C {
    //         Self::C::new()
    //     }

    //     fn print(&self) {
    //         println!("B Safe");
    //     }
    // }

    // impl B for BUnsafe {
    //     type C = CType;

    //     fn c(&self) -> Self::C {
    //         CUnsafe::new()
    //     }

    //     fn print(&self) {
    //         println!("B Unsafe");
    //     }
    // }

    // impl C for CSafe {
    //     type D = DSafe;

    //     fn d(&self) -> Self::D {
    //         Self::D::new()
    //     }

    //     fn print(&self) {
    //         println!("C Safe");
    //     }
    // }

    // impl C for CUnsafe {
    //     type D = ImplD;

    //     fn d(&self) -> Self::D {
    //         DUnsafe::new()
    //     }

    //     fn print(&self) {
    //         println!("C Unsafe");
    //     }
    // }


    // impl D for DSafe {
    //     fn x(&self) where Self: Sized {
    //         hash_i64(12);
    //     }

    //     fn print(&self) {
    //         println!("D Safe");
    //     }
    // }

    // impl D for DUnsafe {
    //     fn x(&self) where Self: Sized {
    //         hash_i64(13);
    //     }

    //     fn print(&self) {
    //         println!("D Unsafe");
    //     }
    // }

    // pub trait Visitor {
    //     fn visit_0(v: &DSafe);
    //     fn visit_1(v: &DUnsafe);
    // }

    // impl DMaybe {
    //     pub fn visit<T: Visitor>(&self, t: T) {
    //         match self {
    //             Self::Safe(s) => {
    //                 T::visit_0(s);
    //             }
    //             Self::Unsafe(s) => {
    //                 T::visit_1(s);
    //             }
    //         }
    //     }

    //     #[inline(always)]
    //     pub fn visit0<F1: FnOnce(&DSafe), F2: FnOnce(&DUnsafe)>(&self, f1: F1, f2: F2) {
    //         match self {
    //             Self::Safe(s) => {
    //                 f1(s);
    //             }
    //             Self::Unsafe(s) => {
    //                 f2(s);
    //             }
    //         }
    //     }
    // }

    // fn visit(p: &impl D) {
    //     p.print();
    // }

    // fn my_func(cmp: fn(&ImplD), p: &ImplD) {
    //     cmp(p);
    // }

    // fn my_func3<T: D>(f: fn(&ImplD), d: &T) {}

    // // type Func<T: D> = impl FnOnce(T);

    // struct Builder;

    // impl Builder {
    //     pub fn build() {
    //         let a = ASafe::new();
    //         // {
    //         //     fn visit(d: &impl D) {
    //         //         d.print();
    //         //     }
    //         //     Self::check_d(&a, visit);
    //         // }

    //         Self::check_a(&a);

    //         let f: fn(&ImplD) = visit;

    //         let b = a.new_b();

    //         let x = {
    //             fn vis(d: &impl D) {
    //                 d.print();
    //             }
    //             match b {
    //                 Safe(d) => vis(&d),
    //                 DMaybe::Unsafe(d) => vis(&d),
    //             }
    //             0
    //         };
    //     }

    //     fn check_a(a: &impl A) {
    //         match a.maybe_d() {
    //             Safe(d) => Self::visit_d(&d),
    //             DMaybe::Unsafe(d) => Self::visit_d(&d),
    //         }
    //     }

    //     fn visit_d(d: &impl D) {
    //         d.print();
    //     }
    // }

    // macro_rules! visit_me {
    //     ($a:expr, $closure:tt) => {
    //         {
    //             fn visit_mee(d: &impl D) {
    //                 d.print();
    //             }

    //             // let b = $a.new_b();
    //             // let $a;
    //             struct Y;
    //             impl Visitor for Y {
    //                 #[inline(always)]
    //                 fn visit_0(v: &DSafe) {
    //                     // Self::visit(v);
    //                     $closure(v);
    //                 }

    //                 #[inline(always)]
    //                 fn visit_1(v: &DUnsafe) {
    //                     // Self::visit(v);
    //                     $closure(v);
    //                 }
    //             }
    //             impl Y {
    //                 #[inline(always)]
    //                 fn visit(v: &impl D) {
    //                     // v.print();
    //                     // println!("visit!!!");
    //                     $closure(v);
    //                 }
    //             }

    //             $a.visit(Y);
    //         }
    //     };
    // }

    // macro_rules! visit_me2 {
    //     ($a:expr, $closure:tt) => {
    //         {
    //             $a.visit0(|v| $closure(v), |v| $closure(v));
    //         }
    //     };
    // }

    // macro_rules! variant_visit {
    //     ($a:expr, $closure:tt) => {
    //         {
    //             match &$a {
    //                 Variant::X(xx) => $closure(xx),
    //                 Variant::Y(yy) => $closure(yy)
    //             }
    //         }
    //     };
    // }

    // macro_rules! variant_visit2 {
    //     ($a:expr, $b:ident, $closure:tt) => {
    //         {
    //             match &$a {
    //                 Variant::X(xx) => $closure(xx),
    //                 Variant::Y(yy) => $closure(yy)
    //             }
    //         }
    //     };
    // }

    // #[test]
    // fn graph() {
    //     let x = Variant::<DSafe, DUnsafe>::X(DSafe::new());

    //     let a = ASafe::new();

    //     fn visit_it(v: &impl D) {
    //         v.print();
    //     }

    //     let mut rand = Rand::new(13);
    //     variant_visit!(x, ({
    //         fn v0(v: &impl D, r: u64) {
    //             v.print();
    //         }
    //         |d| {
    //             v0(d, 0);
    //         }
    //     }));

    //     // variant_visit2!(x, &impl D, visit_it);
    // }

    // extern crate test;

    // #[bench]
    // fn bench_visit(b: &mut Bencher) {
    //     let x = Variant::<DSafe, DUnsafe>::X(DSafe::new());
    //     let mut rand = Rand::new(11);
    //     unsafe { test::black_box(Rand::next) };

    //     #[inline(always)]
    //     fn visit(v: &impl D, r: u64) {
    //         // v.print();
    //         v.x();
    //     }

    //     b.iter(move || {
    //         variant_visit!(x, (|d| {
    //             let vv = rand.next();
    //             for i in 1u64..10000u64 {
    //                 let vv = rand.next();
    //                 visit(d, vv);
    //             }
    //         }));
    //     });
    // }
}