#![allow(warnings)]

// #![feature(test)]
// #![feature(trait_alias)]
// #![feature(type_alias_impl_trait)]
#![feature(type_alias_impl_trait)]
#![feature(bigint_helper_methods)]
// #[allow(soft_unstable)]
#![feature(mem_copy_fn)]
// #![feature(core_intrinsics)]
#![feature(likely_unlikely)]

#![feature(test)]
extern crate test;

mod alloc;
pub mod hash;
mod block;
mod header;
mod message;
mod appender;
mod heap;
mod vector;
mod map;
mod string;
mod data;


#[cfg(test)]
mod tests {

}
