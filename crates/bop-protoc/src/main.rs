#![allow(warnings)]

// #![feature(test)]
// #![feature(bigint_helper_methods)]
#![allow(unused)]
#![allow(unused_imports)]
// extern crate test;

// #[cfg(all(target_arch = "wasm32", target_env = "p1", not(target_feature = "atomics")))]
// #[cfg(all(target_arch = "wasm32", target_env = "p1"))]
#[cfg(all(target_os = "wasi"))]
#[global_allocator]
static A: rlsf::SmallGlobalTlsf = rlsf::SmallGlobalTlsf::new();

#[cfg(all(not(target_os = "wasi")))]
#[global_allocator]
static A: snmalloc_rs::SnMalloc = snmalloc_rs::SnMalloc;

// extern crate wee_alloc;
// extern crate wapr;

// extern crate arrow;


// use arrow::array::{
//     Array, ArrayData, BooleanArray, Int32Array, Int32Builder, ListArray, PrimitiveArray,
//     StringArray, StructArray,
// };
// use arrow::buffer::Buffer;
// use arrow::datatypes::{DataType, Date64Type, Field, Time64NanosecondType, ToByteSlice};

mod go;
mod hash;
mod linker;
mod model;
mod parser;
mod rust;

use std::alloc::{alloc, dealloc, GlobalAlloc, Layout};
use std::future::Future;
use std::io::Read;
use std::path::Path;
// use wapr::hash;
use crate::parser::Parser;
use futures::executor::block_on;

// Use `wee_alloc` as the global allocator.
// #[global_allocator]
// static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

// `foo()` returns a type that implements `Future<Output = u8>`.
// `foo().await` will result in a value of type `u8`.
async fn foo() -> u8 {
    println!("foo()");
    5
}

fn bar() -> impl Future<Output = u8> {
    // This `async` block results in a type that implements
    // `Future<Output = u8>`.
    async {
        let x: u8 = foo().await;
        x + 5
    }
}

async fn count() {
    use futures::{future, select};

    let mut a_fut = future::ready(4);
    let mut b_fut = future::ready(6);
    let mut total = 0;

    loop {
        select! {
            a = a_fut => {
                total += a
            },
            b = b_fut => total += b,
            complete => break,
            default => unreachable!(), // never runs (futures are ready, then complete)
        }
    }
    println!("total: {:}", total);
    assert_eq!(total, 10);
}

fn print_hash(s: &str) {
    use hash::Hasher;
    println!("{}: {}", s, s.hash_wasp());
}

pub fn main() -> anyhow::Result<()> {
    // pub fn main() -> Result<(), ()> {
    // print_hash("h");
    // print_hash("he");
    // print_hash("hel");
    // print_hash("hell");
    // print_hash("hello");
    print_hash("hellonow");
    // print_hash("hellonowhellonow");
    // print_hash("hellonowhellonowhellonowhellonow");
    // print_hash("hellonowhellonowhellonowhellonowhellonowhellonowhellonowhellonow");
    // //
    // // let p = unsafe { alloc(Layout::from_size_align_unchecked(64, 8)) };
    // // println!("alloc pointer: {:}", p as usize);
    // // unsafe { dealloc(p, Layout::from_size_align_unchecked(64, 8)); }
    // //
    // // println!("hash_u64(10): {:}", hash::hash_u64(10));
    //
    // // println!("hi");
    //
    block_on(async {
        count().await;

        let _br = bar().await;
    });
    //
    // let file = std::env::current_dir().unwrap().to_str().unwrap();
    // println!("{}", std::env::current_dir().unwrap().to_str().unwrap());

    // let input_fname = "./src/testdata/s.wap";
    //
    // let mut input_file =
    //     std::fs::File::open(Path::new(input_fname))?;
    //     // std::fs::File::open(Path::new("/Users/cmo/repos/moontrade/wap/wapc/src/testdata/s.wap"))?;
    //         // .map_err(|err| format!("error opening input {}: {}", input_fname, err))?;
    // let mut contents = Vec::new();
    // input_file.read_to_end(&mut contents)?;
    // input_file
    //     .read_to_end(&mut contents)
    //     .map_err(|err| format!("read error: {}", err))?;

    // let contents = std::fs::read_to_string(Path::new("/Users/cmo/repos/moontrade/wap/waspc/src/testdata/s.wasp"))?;
    // // let mut contents = String::from_utf8(contents)?;
    // match Parser::parse(&contents) {
    //     Ok(_module) => {}
    //     Err(reason) => {
    //         let err = reason.to_string();
    //         println!("{}", reason.to_string());
    //     }
    // }

    // Primitive Arrays
    //
    // Primitive arrays are arrays of fixed-width primitive types (bool, u8, u16, u32,
    // u64, i8, i16, i32, i64, f32, f64)

    // // Create a new builder with a capacity of 100
    // let mut primitive_array_builder = Int32Builder::with_capacity(100);
    //
    // // Append an individual primitive value
    // primitive_array_builder.append_value(55);
    //
    // // Append a null value
    // primitive_array_builder.append_null();
    //
    // // Append a slice of primitive values
    // primitive_array_builder.append_slice(&[39, 89, 12]);
    //
    // // Append lots of values
    // primitive_array_builder.append_null();
    // primitive_array_builder.append_slice(&(25..50).collect::<Vec<i32>>());
    //
    // // Build the `PrimitiveArray`
    // let primitive_array = primitive_array_builder.finish();
    // // Long arrays will have an ellipsis printed in the middle
    // println!("{primitive_array:?}");
    //
    // // Arrays can also be built from `Vec<Option<T>>`. `None`
    // // represents a null value in the array.
    // let date_array: PrimitiveArray<Date64Type> =
    //     vec![Some(1550902545147), None, Some(1550902545147)].into();
    // println!("{date_array:?}");
    //
    // let time_array: PrimitiveArray<Time64NanosecondType> =
    //     (0..100).collect::<Vec<i64>>().into();
    // println!("{time_array:?}");
    //
    // // We can build arrays directly from the underlying buffers.
    //
    // // BinaryArrays are arrays of byte arrays, where each byte array
    // // is a slice of an underlying buffer.
    //
    // // Array data: ["hello", null, "parquet"]
    // let values: [u8; 12] = [
    //     b'h', b'e', b'l', b'l', b'o', b'p', b'a', b'r', b'q', b'u', b'e', b't',
    // ];
    // let offsets: [i32; 4] = [0, 5, 5, 12];
    //
    // let array_data = ArrayData::builder(DataType::Utf8)
    //     .len(3)
    //     .add_buffer(Buffer::from(offsets.to_byte_slice()))
    //     .add_buffer(Buffer::from(&values[..]))
    //     .null_bit_buffer(Some(Buffer::from([0b00000101])))
    //     .build()
    //     .unwrap();
    // let binary_array = StringArray::from(array_data);
    // println!("{binary_array:?}");
    //
    // // ListArrays are similar to ByteArrays: they are arrays of other
    // // arrays, where each child array is a slice of the underlying
    // // buffer.
    // let value_data = ArrayData::builder(DataType::Int32)
    //     .len(8)
    //     .add_buffer(Buffer::from(&[0, 1, 2, 3, 4, 5, 6, 7].to_byte_slice()))
    //     .build()
    //     .unwrap();
    //
    // // Construct a buffer for value offsets, for the nested array:
    // //  [[0, 1, 2], [3, 4, 5], [6, 7]]
    // let value_offsets = Buffer::from(&[0, 3, 6, 8].to_byte_slice());
    //
    // // Construct a list array from the above two
    // let list_data_type =
    //     DataType::List(Arc::new(Field::new("item", DataType::Int32, false)));
    // let list_data = ArrayData::builder(list_data_type)
    //     .len(3)
    //     .add_buffer(value_offsets)
    //     .add_child_data(value_data)
    //     .build()
    //     .unwrap();
    // let list_array = ListArray::from(list_data);
    //
    // println!("{list_array:?}");

    // StructArrays are arrays of tuples, where each tuple element is
    // from a child array. (In other words, they're like zipping
    // multiple columns into one and giving each subcolumn a label.)

    // StructArrays can be constructed using the StructArray::from
    // helper, which takes the underlying arrays and field types.
    // let struct_array = StructArray::from(vec![
    //     (
    //         Arc::new(Field::new("b", DataType::Boolean, false)),
    //         Arc::new(BooleanArray::from(vec![false, false, true, true]))
    //             as Arc<dyn Array>,
    //     ),
    //     (
    //         Arc::new(Field::new("c", DataType::Int32, false)),
    //         Arc::new(Int32Array::from(vec![42, 28, 19, 31])),
    //     ),
    // ]);
    // println!("{struct_array:?}");

    Ok(())

    // match std::fs::read_to_string(Path::new("src/testdata/s.wap")) {
    // match std::fs::read_to_string(Path::new("/Users/cmo/repos/moontrade/wap/wapc/src/testdata/s.wap")) {
    //     Ok(s) => match Parser::parse(&s) {
    //         Ok(_module) => {}
    //         Err(reason) => {
    //             let err = reason.to_string();
    //             println!("{}", reason.to_string());
    //         }
    //     },
    //     Err(reason) => {
    //         println!("{}", reason.to_string());
    //     }
    // }
}
