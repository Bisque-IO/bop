
// #[derive(Debug)]
// pub struct Struct<'a> {
//     fields: BumpVec<'a, &'a mut Field<'a>>,
// }
//
// #[derive(Debug)]
// pub struct Field<'a> {
//     parent: &'a mut Struct<'a>,
// }

use bumpalo::{Bump, boxed::Box as BumpBox, collections::Vec as BumpVec};
use std::borrow::ToOwned;
use std::cell::{RefCell, RefMut};
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::ptr::{null, null_mut};
use std::rc::{Rc, Weak};
use std::result::Result;
use std::sync::PoisonError;
use ghost_cell::{GhostCell, GhostToken};
use thiserror::Error;

type ModelResult<T> = Result<T, crate::model::ModelError>;

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Position {
    pub line: usize,
    pub col: usize,
    pub index: usize,
}

impl Position {
    pub fn new(line: usize, col: usize, index: usize) -> Self {
        Self { line, col, index }
    }
}

#[derive(Error, Clone, Debug)]
pub enum ModelError {
    #[error("max fields reached")]
    MaxFieldsReached,
    #[error("duplicate struct name: {0}")]
    DuplicateName(String),
    #[error("duplicate field name: {0}::{1}")]
    DuplicateFieldName(String, String),
    #[error("field kind not set for: {0}::{1}")]
    FieldKindNotSet(String, String),
    #[error("enums must be an integer type: i8, i16, i32, i64, i128, u8, u16, u32, u64, u128")]
    EnumMustBeIntegerType,
    #[error(
        "Cyclic dependency between: {0} and {1}. Cannot embed. Must use Pointer (*) of the types."
    )]
    CyclicDependency(String, String),
    #[error("invalid parent kind: {0} must be Struct, Module, Const, Alias, Field")]
    InvalidParentKind(String),
    #[error("type alias: {0} must be nested either in a module or struct")]
    InvalidAliasParent(String),
}

#[derive(Debug)]
pub enum Node<'a> {
    Struct(&'a mut Struct<'a>),
    Field(&'a mut Field<'a>),
}

#[derive(PartialEq, Debug)]
pub struct Ref<'a, T> {
    reference: *mut T,
    _phantom: PhantomData<&'a T>,
}

impl<'a, T> Ref<'a, T> {
    pub fn new(p: &'a mut T) -> Ref<'a, T> {
        Self { reference: p, _phantom: PhantomData }
    }
    pub fn as_mut(&mut self) -> &'a mut T {
        unsafe { self.reference.as_mut().unwrap() }
    }
}

impl<'a, T> Clone for Ref<'a, T> {
    fn clone(&self) -> Self {
        Self { reference: self.reference, _phantom: PhantomData }
    }
}

impl<'a, T> Copy for Ref<'a, T> {}

#[derive(Copy, Clone, Debug)]
pub enum Parent<'a> {
    None,
    Package(Ref<'a, Package<'a>>),
    // Package(&'a Package<'a>),
    Struct(Ref<'a, Struct<'a>>),
    // Const(Ref<Const>),
    // Alias(Ref<Alias>),
    Field(Ref<'a, Field<'a>>),
}

#[derive(Debug)]
pub struct Packages<'a> {
    bump: &'a Bump,
    packages: BTreeMap<String, &'a mut Package<'a>>,
}

impl<'a> Packages<'a> {
    pub fn new(bump: &'a Bump) -> &'a mut Self {
        bump.alloc(Self {
            bump,
            packages: BTreeMap::new(),
        })
    }

    pub fn add_package(&'a mut self, name: &'a str) -> &'a mut Package<'a> {
        let ptr: *mut Package<'a> = BumpBox::into_raw(BumpBox::new_in(Package {
            parent: unsafe { &mut *(self as *mut Self) },
            full_name: Some(name),
            name: None,
            schema: None,
            files: BumpVec::new_in(&self.bump),
        }, &self.bump));

        self.packages.insert(name.to_string(), BumpBox::leak(unsafe { BumpBox::from_raw(ptr) }));

        BumpBox::leak(unsafe { BumpBox::from_raw(ptr) })
    }
}

#[derive(Debug)]
pub struct Package<'a> {
    parent: &'a mut Packages<'a>,
    full_name: Option<&'a str>,
    name: Option<&'a str>,
    schema: Option<&'a str>,
    files: BumpVec<'a, &'a mut File<'a>>,
}

impl<'a> Package<'a> {
    pub fn bump(&self) -> &'a Bump {
        self.parent.bump
    }

    pub fn add_file(&'a mut self, path: &'a str) -> &'a mut File<'a> {
        let ptr: *mut File<'a> = BumpBox::into_raw(BumpBox::new_in(File {
            parent: unsafe { &mut *(self as *mut Self) },
            path,
            children: Children::new(Parent::None, self.parent.bump),
        }, self.parent.bump));

        self.files.push(BumpBox::leak(unsafe { BumpBox::from_raw(ptr) }));

        let r = BumpBox::leak(unsafe { BumpBox::from_raw(ptr) });
        r.children.parent = Parent::Package(Ref::new(self));
        r
    }
}

#[derive(Debug)]
pub struct File<'a> {
    parent: &'a mut Package<'a>,
    path: &'a str,
    children: Children<'a>,
}

#[derive(Debug)]
pub struct Children<'a> {
    parent: Parent<'a>,
    values: BumpVec<'a, Node<'a>>,
}

impl<'a> Children<'a> {
    pub fn new(parent: Parent<'a>, bump: &'a Bump) -> Self {
        Self {
            parent,
            values: BumpVec::new_in(bump)
        }
    }

    pub fn add_struct(&mut self, bump: &'a Bump) -> &'a mut Struct<'a> {
        let struct_ptr: *mut Struct<'a> = BumpBox::into_raw(BumpBox::new_in(Struct {
            name: None,
            fields: BumpVec::new_in(bump),
            children: Children::new(self.parent, bump),
        }, bump));

        self.values.push(Node::Struct(BumpBox::leak(
            unsafe { BumpBox::from_raw(struct_ptr) }
        )));

        BumpBox::leak(
            unsafe { BumpBox::from_raw(struct_ptr) }
        )
    }
}

#[derive(Debug)]
pub struct Struct<'a> {
    name: Option<&'a str>,
    fields: BumpVec<'a, &'a mut Field<'a>>,
    children: Children<'a>,
}

impl<'a> Struct<'a> {
    pub fn add_field(&mut self, bump: &'a Bump) -> &'a mut Field<'a> {
        let ptr: *mut Field<'a> = BumpBox::into_raw(BumpBox::new_in(Field {
            name: None,
            parent: unsafe { &mut *(self as *mut Self) },
        }, bump));

        self.fields.push(BumpBox::leak(unsafe { BumpBox::from_raw(ptr) }));

        BumpBox::leak(unsafe { BumpBox::from_raw(ptr) })
    }
}

#[derive(Debug)]
pub struct Field<'a> {
    name: Option<&'a str>,
    parent: &'a mut Struct<'a>,
}

#[test]
fn test_struct() {
    let mut bump = Bump::new();
    let mut packages = Packages::new(&bump);
    let package = packages.add_package(packages.bump.alloc_str("model"));
    let file = package.add_file(bump.alloc_str("model.bop"));

    // let mut bump = Bump::new();
    // let mut module = BumpBox::new_in(Package {
    //     name: None,
    //     children: Children {
    //         values: BumpVec::new_in(&bump),
    //     },
    // }, &bump);
    //
    // let st = module.children.add_struct(&bump);
    // st.name = Some(bump.alloc_str("Price"));

    println!("{:?}", file);

    // let mut structs = BumpVec::new_in(&bump);
    //
    // let mut st = BumpBox::new_in(Struct {
    //     fields: BumpVec::new_in(&bump),
    // }, &bump);
    //
    // let st_ptr = BumpBox::into_raw(st);
    // let mut st = unsafe { BumpBox::from_raw(st_ptr) };
    //
    // structs.push(st);
    //
    // let mut st = unsafe { BumpBox::from_raw(st_ptr) };
    // // let mut st = unsafe { BumpBox::leak(st) };
    //
    // st.fields.push(BumpBox::new_in(Field {
    //     parent: unsafe { BumpBox::leak(unsafe { BumpBox::from_raw(st_ptr) }) },
    // }, &bump));
    //
    // println!("{:?}", unsafe { BumpBox::from_raw(st_ptr) });

    // let mut fields = BumpVec::new_in(&bump);
    
    // let mut s = bump.alloc(Struct {
    //     fields: fields,
    // });

    // let mut fields = &mut s.fields;

    // fields.push(bump.alloc(Field {
    //     parent: s,
    // }));
}