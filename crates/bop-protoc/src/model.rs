use std::borrow::ToOwned;
use std::cell::{RefCell, RefMut};
use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::ptr::{null, null_mut};
use std::rc::{Rc, Weak};
use std::result::Result;
use std::sync::PoisonError;

use thiserror::Error;

type ModelResult<T> = Result<T, ModelError>;

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

// impl fmt::Display for AddFieldError {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         match self {
//             AddFieldError::MaxFieldsReached => {
//                 write!(f, "max fields reached")
//             }
//             AddFieldError::DuplicateName(_) => {
//                 write!(f, "name already used")
//             }
//         }
//     }
// }

// impl fmt::Display for ModelError {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         write!(f, "invalid first item to double")
//     }
// }

#[derive(Clone, Debug)]
pub struct Module {
    this: Option<Weak<RefCell<Module>>>,
    full_name: Option<String>,
    schema: Option<String>,
    imports: Imports,
    children: Children,
}

impl Module {
    pub fn new(schema: Option<String>) -> Ref<Self> {
        let m = Rc::new(RefCell::new(Self {
            this: None,
            full_name: None,
            schema,
            imports: Imports::new(),
            children: Children::new(ParentKind::None),
        }));
        m.borrow_mut().this = Some(Rc::downgrade(&m));
        m.borrow_mut().children.parent = ParentKind::Module(Ref::Weak(Rc::downgrade(&m)));
        Ref::Strong(m)
    }

    #[inline]
    pub fn children(&self) -> &Children {
        &self.children
    }

    #[inline]
    pub fn add_alias(&mut self, name: String, kind: Kind) -> ModelResult<Ref<Alias>> {
        self.children.add_alias(name, kind)
    }

    #[inline]
    pub fn add_struct(
        &mut self,
        name: Option<String>,
        kind: StructKind,
    ) -> ModelResult<Ref<Struct>> {
        self.children.add_struct(name, kind)
    }

    #[inline]
    pub fn add_enum(&mut self, name: Option<String>) -> ModelResult<Ref<Enum>> {
        self.children.add_enum(name)
    }

    #[inline]
    pub fn add_const(&mut self, name: String) -> ModelResult<Ref<Const>> {
        self.children.add_const(Some(name))
    }

    pub fn full_name(&self) -> &Option<String> {
        &self.full_name
    }

    pub fn schema(&self) -> &Option<String> {
        &self.schema
    }

    pub fn set_full_name(&mut self, full_name: Option<String>) {
        self.full_name = full_name;
    }

    pub fn set_schema(&mut self, schema: Option<String>) {
        self.schema = schema;
    }
}

#[derive(Clone, Debug)]
pub struct Imports {
    list: Vec<Ref<Import>>,
    map: HashMap<String, Ref<Import>>,
}

impl Imports {
    pub fn new() -> Self {
        Self {
            list: Vec::new(),
            map: HashMap::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Children {
    parent: ParentKind,
    list: Vec<Kind>,
    map: HashMap<String, Kind>,
}

impl Children {
    pub fn new(parent: ParentKind) -> Self {
        Self {
            parent,
            list: Vec::new(),
            map: HashMap::new(),
        }
    }

    fn push(&mut self, kind: Kind) {
        self.list.push(kind);
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn add_alias(&mut self, name: String, kind: Kind) -> ModelResult<Ref<Alias>> {
        let mut alias = Rc::new(RefCell::new(Alias::new(
            self.parent.clone_weak(),
            name.clone(),
            kind,
        )));
        alias.borrow_mut().this = Some(Rc::downgrade(&alias));
        self.list.push(Kind::Alias(Ref::Strong(Rc::clone(&alias))));
        self.map
            .insert(name.clone(), Kind::Alias(Ref::Weak(Rc::downgrade(&alias))));
        Ok(Ref::Strong(alias))
    }

    pub fn add_struct(
        &mut self,
        name: Option<String>,
        flavor: StructKind,
    ) -> ModelResult<Ref<Struct>> {
        let mut s = Rc::new(RefCell::new(Struct::new(self.parent.clone_weak(), flavor)));
        s.borrow_mut().this = Some(Rc::downgrade(&s));
        s.borrow_mut()
            .set_name(name.clone().unwrap_or_else(|| "".to_owned()));
        s.borrow_mut().kind = flavor;
        self.list.push(Kind::Struct(Ref::Strong(Rc::clone(&s))));
        let n = name.unwrap_or_else(|| "".to_owned());
        self.map
            .insert(n, Kind::Struct(Ref::Weak(Rc::downgrade(&s))));
        Ok(Ref::Strong(s))
    }

    pub fn add_enum(&mut self, name: Option<String>) -> ModelResult<Ref<Enum>> {
        let mut e = Rc::new(RefCell::new(Enum::new(self.parent.clone_weak())));
        e.borrow_mut().this = Some(Rc::downgrade(&e));
        e.borrow_mut().name = name.clone();
        self.list.push(Kind::Enum(Ref::Strong(Rc::clone(&e))));
        self.map.insert(
            name.unwrap_or_else(|| "".to_owned()),
            Kind::Enum(Ref::Weak(Rc::downgrade(&e))),
        );
        Ok(Ref::Strong(e))
    }

    pub fn add_const(&mut self, name: Option<String>) -> ModelResult<Ref<Const>> {
        let mut c = Rc::new(RefCell::new(Const::new(self.parent.clone_weak())));
        c.borrow_mut().this = Some(Rc::downgrade(&c));
        c.borrow_mut().name = name.clone();
        self.list.push(Kind::Const(Ref::Strong(Rc::clone(&c))));
        self.map.insert(
            name.unwrap_or_else(|| "".to_owned()),
            Kind::Const(Ref::Weak(Rc::downgrade(&c))),
        );
        Ok(Ref::Strong(c))
    }

    pub fn structs(&self) -> impl Iterator<Item = &Struct> {
        self.list
            .iter()
            .filter(|it| matches!(it, Kind::Struct(_)))
            .map(|it| match it {
                Kind::Struct(s) => s.borrow().unwrap(),
                _ => unreachable!(),
            })
    }

    pub fn structs_mut(&mut self) -> impl Iterator<Item = &mut Ref<Struct>> {
        self.list
            .iter_mut()
            .filter(|it| matches!(it, Kind::Struct(_)))
            .map(|it| match it {
                Kind::Struct(s) => s,
                _ => unreachable!(),
            })
    }

    pub fn structs_len(&self) -> usize {
        self.structs().count()
    }

    pub fn enums(&self) -> impl Iterator<Item = &Ref<Enum>> {
        self.list
            .iter()
            .filter(|it| matches!(it, Kind::Enum(_)))
            .map(|it| match it {
                Kind::Enum(s) => s,
                _ => unreachable!(),
            })
    }

    pub fn enums_mut(&mut self) -> impl Iterator<Item = &mut Ref<Enum>> {
        self.list
            .iter_mut()
            .filter(|it| matches!(it, Kind::Enum(_)))
            .map(|it| match it {
                Kind::Enum(e) => e,
                _ => unreachable!(),
            })
    }

    pub fn enums_len(&self) -> usize {
        self.enums().count()
    }
}

#[derive(Clone, Debug)]
pub enum ParentKind {
    None,
    Module(Ref<Module>),
    Struct(Ref<Struct>),
    Const(Ref<Const>),
    Alias(Ref<Alias>),
    Field(Ref<Field>),
}

impl ParentKind {
    pub fn kind(&self) -> Kind {
        match self {
            ParentKind::None => Kind::None,
            ParentKind::Module(m) => Kind::Module(m.clone()),
            ParentKind::Struct(s) => Kind::Struct(s.clone()),
            ParentKind::Const(c) => Kind::Const(c.clone()),
            ParentKind::Alias(a) => Kind::Alias(a.clone()),
            ParentKind::Field(f) => Kind::Field(f.clone()),
        }
    }

    pub fn clone_weak(&self) -> Self {
        match self {
            ParentKind::None => ParentKind::None,
            ParentKind::Module(m) => ParentKind::Module(Ref::Weak(m.downgrade())),
            ParentKind::Struct(s) => ParentKind::Struct(Ref::Weak(s.downgrade())),
            ParentKind::Const(c) => ParentKind::Const(Ref::Weak(c.downgrade())),
            ParentKind::Alias(a) => ParentKind::Alias(Ref::Weak(a.downgrade())),
            ParentKind::Field(f) => ParentKind::Field(Ref::Weak(f.downgrade())),
        }
    }

    pub fn clone_strong(&self) -> Self {
        match self {
            ParentKind::None => ParentKind::None,
            ParentKind::Module(m) => ParentKind::Module(Ref::Strong(m.upgrade().unwrap())),
            ParentKind::Struct(s) => ParentKind::Struct(Ref::Strong(s.upgrade().unwrap())),
            ParentKind::Const(c) => ParentKind::Const(Ref::Strong(c.upgrade().unwrap())),
            ParentKind::Alias(a) => ParentKind::Alias(Ref::Strong(a.upgrade().unwrap())),
            ParentKind::Field(f) => ParentKind::Field(Ref::Strong(f.upgrade().unwrap())),
        }
    }

    pub fn is_module(&self) -> bool {
        matches!(self, ParentKind::Module(_))
    }

    pub fn is_struct(&self) -> bool {
        matches!(self, ParentKind::Struct(_))
    }

    pub fn is_const(&self) -> bool {
        matches!(self, ParentKind::Const(_))
    }

    pub fn is_alias(&self) -> bool {
        matches!(self, ParentKind::Alias(_))
    }

    pub fn is_field(&self) -> bool {
        matches!(self, ParentKind::Field(_))
    }

    // pub fn children_mut(&mut self) -> Option<&mut Children> {
    //     match self {
    //         ParentKind::None => None,
    //         ParentKind::Module(m) => Some(&mut m.as_mut().children),
    //         ParentKind::Struct(s) => s.as_mut().ensure_children(),
    //     }
    // }
}

#[derive(Clone, Debug)]
pub enum Ref<T> {
    /// Strong reference
    Strong(Rc<RefCell<T>>),
    /// Weak reference
    Weak(Weak<RefCell<T>>),
}

impl<T> Ref<T> {
    pub fn new(value: T) -> Ref<T> {
        Ref::Strong(Rc::new(RefCell::new(value)))
    }

    pub fn clone_strong(&self) -> Ref<T> {
        Ref::Strong(self.upgrade().unwrap())
    }

    pub fn clone_weak(&self) -> Ref<T> {
        Ref::Weak(self.downgrade())
    }

    pub fn downgrade(&self) -> Weak<RefCell<T>> {
        match self {
            Ref::Strong(r) => Rc::downgrade(r),
            Ref::Weak(r) => r.clone(),
        }
    }

    pub fn upgrade(&self) -> Option<Rc<RefCell<T>>> {
        match self {
            Ref::Strong(r) => Some(r.clone()),
            Ref::Weak(r) => r.upgrade(),
        }
    }

    pub fn borrow(&self) -> Option<&T> {
        match self {
            Ref::Strong(r) => {
                let raw = r.as_ptr();
                if raw.is_null() {
                    return None;
                }
                Some(unsafe { (&*raw) })
            }
            Ref::Weak(w) => {
                let raw = w.as_ptr();
                if raw.is_null() {
                    return None;
                }
                Some(unsafe { (&*(raw as *mut T)) })
            }
        }
    }

    pub fn as_ref(&self) -> &T {
        self.borrow().unwrap()
    }

    // pub fn as_mut(&mut self) -> RefMut<'_, T> {
    //     self.borrow_mut().unwrap()
    // }
    //
    // pub fn borrow_mut(&mut self) -> Option<RefMut<'_, T>> {
    //     match self {
    //         Ref::Strong(r) => {
    //             let raw = r.as_ptr();
    //             if raw.is_null() {
    //                 return None;
    //             }
    //             Some(r.borrow_mut())
    //             // Some(unsafe { (&mut *raw) })
    //         }
    //         Ref::Weak(w) => {
    //             let raw = w.as_ptr();
    //             if raw.is_null() {
    //                 return None;
    //             }
    //             w.upgrade().as_mut()
    //             None
    //             // Some(w.upgrade().unwrap().borrow_mut())
    //             // Some(w.borrow_mut())
    //             // Some(unsafe { (&mut *(raw as *mut T)) })
    //         }
    //     }
    // }

    pub fn borrow_mut(&mut self) -> Option<&mut T> {
        match self {
            Ref::Strong(r) => {
                let raw = r.as_ptr();
                if raw.is_null() {
                    return None;
                }
                Some(unsafe { (&mut *raw) })
            }
            Ref::Weak(w) => {
                let raw = w.as_ptr();
                if raw.is_null() {
                    return None;
                }
                // Some(w.upgrade().unwrap().borrow_mut())
                // Some(w.borrow_mut())
                Some(unsafe { (&mut *(raw as *mut T)) })
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum Kind {
    None,
    Alias(Ref<Alias>),
    Module(Ref<Module>),
    Import(Ref<Import>),
    Const(Ref<Const>),
    Bool,
    Int(Int),
    Float(Float),
    String(Str),
    Pad(i32),
    Struct(Ref<Struct>),
    Field(Ref<Field>),
    Enum(Ref<Enum>),
    Array(Ref<Array>),
    Map(Ref<Map>),
    Optional(Ref<Optional>),
    Pointer(Ref<Pointer>),
    Unknown(String),
}

pub const I8: Kind = Kind::Int(Int::new(Endian::Little, 8, true));
pub const I16: Kind = Kind::Int(Int::new(Endian::Little, 16, true));
pub const I32: Kind = Kind::Int(Int::new(Endian::Little, 32, true));
pub const I64: Kind = Kind::Int(Int::new(Endian::Little, 64, true));
pub const I128: Kind = Kind::Int(Int::new(Endian::Little, 128, true));
pub const I16BE: Kind = Kind::Int(Int::new(Endian::Big, 16, true));
pub const I32BE: Kind = Kind::Int(Int::new(Endian::Big, 32, true));
pub const I64BE: Kind = Kind::Int(Int::new(Endian::Big, 64, true));
pub const I128BE: Kind = Kind::Int(Int::new(Endian::Big, 128, true));
pub const U8: Kind = Kind::Int(Int::new(Endian::Little, 8, false));
pub const U16: Kind = Kind::Int(Int::new(Endian::Little, 16, false));
pub const U32: Kind = Kind::Int(Int::new(Endian::Little, 32, false));
pub const U64: Kind = Kind::Int(Int::new(Endian::Little, 64, false));
pub const U128: Kind = Kind::Int(Int::new(Endian::Little, 128, false));
pub const U16BE: Kind = Kind::Int(Int::new(Endian::Big, 16, false));
pub const U32BE: Kind = Kind::Int(Int::new(Endian::Big, 32, false));
pub const U64BE: Kind = Kind::Int(Int::new(Endian::Big, 64, false));
pub const U128BE: Kind = Kind::Int(Int::new(Endian::Big, 128, false));
pub const F32: Kind = Kind::Float(Float::new(Endian::Little, 32));
pub const F32BE: Kind = Kind::Float(Float::new(Endian::Big, 32));
pub const F64: Kind = Kind::Float(Float::new(Endian::Little, 64));
pub const F64BE: Kind = Kind::Float(Float::new(Endian::Big, 64));

impl Kind {
    pub fn size(&self) -> i32 {
        match self {
            Kind::None => 0,
            Kind::Alias(a) => a.borrow().unwrap().kind.size(),
            Kind::Const(c) => c.borrow().unwrap().kind.size(),
            Kind::Module(_) => 0,
            Kind::Import(im) => im.borrow().unwrap().kind.size(),
            Kind::Int(v) => {
                if v.bits <= 8 {
                    1
                } else if v.bits % 8 > 0 {
                    (v.bits / 8) as i32 + 1
                } else {
                    (v.bits / 8) as i32
                }
            }
            Kind::Float(v) => {
                if v.bits <= 8 {
                    1
                } else if v.bits % 8 > 0 {
                    (v.bits / 8) as i32 + 1
                } else {
                    (v.bits / 8) as i32
                }
            }
            Kind::Pad(p) => *p,
            Kind::Struct(s) => s.borrow().unwrap().size,
            Kind::Field(f) => f.as_ref().size,
            Kind::Enum(e) => e.borrow().unwrap().of.size(),
            Kind::Array(_) => 0,
            Kind::Map(_) => 0,
            Kind::Bool => 1,
            Kind::Optional(o) => 0,
            Kind::Unknown(_) => 0,
            Kind::String(_) => 0,
            Kind::Pointer(_) => 0,
        }
    }

    pub fn is_module(&self) -> bool {
        matches!(self, Kind::Module(_))
    }

    pub fn is_struct(&self) -> bool {
        matches!(self, Kind::Struct(_))
    }

    pub fn is_const(&self) -> bool {
        matches!(self, Kind::Const(_))
    }

    pub fn is_alias(&self) -> bool {
        matches!(self, Kind::Alias(_))
    }

    pub fn is_field(&self) -> bool {
        matches!(self, Kind::Field(_))
    }

    pub fn is_optional(&self) -> bool {
        matches!(self, Kind::Optional(_))
    }

    pub fn is_pointer(&self) -> bool {
        matches!(self, Kind::Pointer(_))
    }

    pub fn as_parent_kind(&self) -> ModelResult<ParentKind> {
        match self {
            Kind::Module(m) => Ok(ParentKind::Module(m.clone_weak())),
            Kind::Struct(s) => Ok(ParentKind::Struct(s.clone_weak())),
            Kind::Alias(a) => Ok(ParentKind::Alias(a.clone_weak())),
            Kind::Const(c) => Ok(ParentKind::Const(c.clone_weak())),
            Kind::Field(f) => Ok(ParentKind::Field(f.clone_weak())),
            _ => Err(ModelError::InvalidParentKind(format!("{:?}", self))),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Import {
    module: Option<Ref<Module>>,
    this: Option<Weak<RefCell<Import>>>,
    alias: Option<String>,
    position: Position,
    kind: Kind,
}

impl Import {
    pub fn new() -> Self {
        Self {
            module: None,
            this: None,
            alias: None,
            position: Position::new(0, 0, 0),
            kind: Kind::None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Optional {
    kind: Option<Kind>,
}

impl Optional {
    pub fn new(kind: Option<Kind>) -> Self {
        Self { kind }
    }
    pub fn kind(&self) -> &Option<Kind> {
        &self.kind
    }
    pub fn set_kind(&mut self, kind: Option<Kind>) {
        self.kind = kind;
    }
}

#[derive(Clone, Debug)]
pub struct Pointer {
    parent: ParentKind,
    this: Option<Weak<RefCell<Pointer>>>,
    kind: Kind,
}

impl Pointer {
    pub fn new(parent: ParentKind, kind: Kind) -> Ref<Pointer> {
        Ref::new(Self {
            parent,
            this: None,
            kind,
        })
    }

    pub fn parent(&self) -> &ParentKind {
        &self.parent
    }
    pub fn kind(&self) -> &Kind {
        &self.kind
    }

    pub fn set_parent(&mut self, parent: ParentKind) {
        self.parent = parent;
    }
    pub fn set_kind(&mut self, kind: Kind) {
        self.kind = kind;
    }
}

#[derive(Clone, Debug)]
pub struct Alias {
    parent: ParentKind,
    this: Option<Weak<RefCell<Alias>>>,
    name: String,
    kind: Kind,
}

impl Alias {
    pub fn new(parent: ParentKind, name: String, kind: Kind) -> Self {
        Self {
            parent,
            this: None,
            name,
            kind,
        }
    }

    pub fn parent(&self) -> &ParentKind {
        &self.parent
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn kind(&self) -> &Kind {
        &self.kind
    }

    pub fn set_parent(&mut self, parent: ParentKind) {
        self.parent = parent;
    }
    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }
    pub fn set_kind(&mut self, kind: Kind) {
        self.kind = kind;
    }
}

#[derive(Clone, Debug)]
pub struct Str {
    inline_size: i32,
    inline: bool,
    heap: bool,
}

impl Str {
    pub fn new(inline_size: i32, inline: bool, flex: bool) -> Self {
        Self {
            inline_size,
            inline,
            heap: flex,
        }
    }

    pub fn new_heap() -> Self {
        Self {
            inline_size: 0,
            inline: false,
            heap: true,
        }
    }

    pub fn new_inline(size: i32) -> Self {
        Self {
            inline_size: size,
            inline: true,
            heap: false,
        }
    }

    pub fn new_inline_heap(size: i32) -> Self {
        Self {
            inline_size: size,
            inline: true,
            heap: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Array {
    element: Kind,
    inline_size: i32,
    inline: bool,
    heap: bool,
    fixed: bool,
}

#[derive(Clone, Debug)]
pub enum MapKind {
    FlatMap,
    BTree,
}

#[derive(Clone, Debug)]
pub struct Map {
    name: String,
    kind: MapKind,
    key: Kind,
    value: Kind,
}

#[derive(Copy, Clone, Debug)]
pub enum Endian {
    Little,
    Big,
}

#[derive(Clone, Debug)]
pub struct Int {
    endian: Endian,
    bits: u16,
    signed: bool,
}

impl Int {
    pub const fn new(endian: Endian, bits: u16, signed: bool) -> Self {
        Self {
            endian,
            bits,
            signed,
        }
    }

    pub fn is_std(&self) -> bool {
        matches!(self.bits, 8 | 16 | 32 | 64 | 128)
    }
}

#[derive(Clone, Debug)]
pub struct Float {
    endian: Endian,
    bits: u16,
}

impl Float {
    pub const fn new(endian: Endian, bits: u16) -> Self {
        Self { endian, bits }
    }

    pub fn is_std(&self) -> bool {
        matches!(self.bits, 8 | 16 | 32 | 64)
    }
}

#[derive(Clone, Debug)]
pub struct Enum {
    parent: ParentKind,
    comment: Option<Comment>,
    name: Option<String>,
    this: Option<Weak<RefCell<Enum>>>,
    of: Kind,
    options: Vec<Rc<RefCell<EnumOption>>>,
}

impl Enum {
    fn new(parent: ParentKind) -> Self {
        Self {
            parent,
            comment: None,
            name: None,
            this: None,
            of: I32,
            options: Vec::new(),
        }
    }

    pub fn add_option(
        &mut self,
        comment: Option<Comment>,
        name: String,
        value: Value,
    ) -> Rc<RefCell<EnumOption>> {
        let option = Rc::new(RefCell::new(EnumOption {
            parent: self.this.clone().unwrap(),
            comment,
            line_comment: None,
            name,
            value,
        }));
        self.options.push(option.clone());
        option
    }

    pub fn parent(&self) -> &ParentKind {
        &self.parent
    }
    pub fn comment(&self) -> &Option<Comment> {
        &self.comment
    }
    pub fn name(&self) -> &Option<String> {
        &self.name
    }
    pub fn this(&self) -> &Option<Weak<RefCell<Enum>>> {
        &self.this
    }
    pub fn of(&self) -> &Kind {
        &self.of
    }
    pub fn kind(&self) -> Kind {
        Kind::Enum(Ref::Weak(self.this.clone().unwrap()))
    }
    pub fn options(&self) -> &Vec<Rc<RefCell<EnumOption>>> {
        &self.options
    }

    pub fn set_comment(&mut self, comment: Option<Comment>) {
        self.comment = comment;
    }
    pub fn set_name(&mut self, name: Option<String>) {
        self.name = name;
    }
    pub fn set_this(&mut self, this: Option<Weak<RefCell<Enum>>>) {
        self.this = this;
    }
    pub fn set_of(&mut self, kind: Kind) {
        self.of = kind;
    }
    pub fn set_options(&mut self, options: Vec<Rc<RefCell<EnumOption>>>) {
        self.options = options;
    }
}

#[derive(Clone, Debug)]
pub struct EnumOption {
    parent: Weak<RefCell<Enum>>,
    comment: Option<Comment>,
    line_comment: Option<Comment>,
    name: String,
    value: Value,
}

impl EnumOption {
    pub fn parent(&self) -> &Weak<RefCell<Enum>> {
        &self.parent
    }
    pub fn comment(&self) -> &Option<Comment> {
        &self.comment
    }
    pub fn line_comment(&self) -> &Option<Comment> {
        &self.line_comment
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn value(&self) -> &Value {
        &self.value
    }

    pub fn set_parent(&mut self, parent: Weak<RefCell<Enum>>) {
        self.parent = parent;
    }
    pub fn set_comment(&mut self, comment: Option<Comment>) {
        self.comment = comment;
    }
    pub fn set_line_comment(&mut self, line_comment: Option<Comment>) {
        self.line_comment = line_comment;
    }
    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }
    pub fn set_value(&mut self, value: Value) {
        self.value = value;
    }
}

#[derive(Copy, Clone, Debug)]
pub enum StructKind {
    Struct,
    Union,
    Variant,
}

#[derive(Clone, Debug)]
pub struct Struct {
    parent: ParentKind,
    this: Option<Weak<RefCell<Struct>>>,
    kind: StructKind,
    name: Option<String>,
    comment: Option<Comment>,
    fields: Vec<Ref<Field>>,
    position: Position,
    children: Option<Children>,
    size: i32,
    packed: bool,
    needs_heap: bool,
}

impl Struct {
    fn new(parent: ParentKind, kind: StructKind) -> Self {
        Self {
            parent,
            this: None,
            kind,
            name: None,
            comment: None,
            fields: Vec::new(),
            position: Position::new(0, 0, 0),
            children: None,
            size: 0,
            packed: false,
            needs_heap: false,
        }
    }

    pub fn name(&self) -> &str {
        match &self.name {
            Some(s) => s.as_str(),
            None => "",
        }
    }

    pub fn parent(&self) -> &ParentKind {
        &self.parent
    }
    pub fn this(&self) -> &Option<Weak<RefCell<Struct>>> {
        &self.this
    }
    pub fn kind(&self) -> StructKind {
        self.kind
    }
    pub fn comment(&self) -> &Option<Comment> {
        &self.comment
    }
    pub fn fields(&self) -> &Vec<Ref<Field>> {
        &self.fields
    }
    pub fn position(&self) -> Position {
        self.position
    }
    pub fn size(&self) -> i32 {
        self.size
    }
    pub fn packed(&self) -> bool {
        self.packed
    }
    pub fn is_flex(&self) -> bool {
        self.needs_heap
    }

    pub fn set_name(&mut self, name: impl Into<String>) -> &mut Self {
        self.name = Some(name.into());
        self
    }

    pub fn clear_name(&mut self) -> &mut Self {
        self.name = None;
        self
    }

    pub fn module(&self) -> Option<Rc<RefCell<Module>>> {
        let mut parent = self.parent.clone();
        let mut i = 0;
        while i < 128 {
            match parent {
                ParentKind::Module(ref m) => {
                    return m.upgrade();
                }
                ParentKind::Struct(s) => parent = s.borrow().unwrap().parent.clone(),
                ParentKind::Const(c) => parent = c.borrow().unwrap().parent.clone(),
                ParentKind::Alias(a) => parent = a.borrow().unwrap().parent.clone(),
                ParentKind::Field(f) => {
                    parent = ParentKind::Struct(Ref::Weak(f.borrow().unwrap().parent.clone()))
                }
                ParentKind::None => return None,
            }
            i += 1;
        }
        None
    }

    pub fn children(&self) -> Option<&Children> {
        self.children.as_ref()
    }

    pub fn children_mut(&mut self) -> Option<&mut Children> {
        self.children.as_mut()
    }

    pub fn add_field(
        &mut self,
        mut f: impl FnMut(&mut Field) -> ModelResult<()>,
    ) -> ModelResult<Ref<Field>> {
        let mut field = Ref::Strong(Rc::new(RefCell::new(Field::new(
            self.this.clone().unwrap(),
            None,
        ))));
        f(field.upgrade().unwrap().borrow_mut().deref_mut())?;
        for field in self.fields.iter() {
            if field
                .as_ref()
                .name()
                .eq_ignore_ascii_case(field.as_ref().name())
            {
                return Err(ModelError::DuplicateFieldName(
                    self.name.clone().unwrap(),
                    field.as_ref().name().to_owned(),
                ));
            }
        }
        field.upgrade().unwrap().borrow_mut().deref_mut().size = field.as_ref().kind.size();
        self.fields.push(field.clone());
        Ok(field)
    }

    fn ensure_children(&mut self) -> &mut Children {
        if self.children.is_none() {
            self.children = Some(Children::new(ParentKind::Struct(Ref::Weak(
                self.this.as_ref().unwrap().clone(),
            ))))
        }
        self.children.as_mut().unwrap()
    }

    #[inline]
    pub fn add_alias(&mut self, name: String, kind: Kind) -> ModelResult<Ref<Alias>> {
        self.ensure_children().add_alias(name, kind)
    }

    pub fn add_struct(
        &mut self,
        name: Option<String>,
        kind: StructKind,
    ) -> ModelResult<Ref<Struct>> {
        self.ensure_children().add_struct(name, kind)
    }

    pub fn add_enum(&mut self, name: Option<String>) -> ModelResult<Ref<Enum>> {
        self.ensure_children().add_enum(name)
    }

    pub fn add_const(&mut self, name: Option<String>) -> ModelResult<Ref<Const>> {
        self.ensure_children().add_const(name)
    }
}

#[derive(Clone, Debug)]
pub struct Field {
    parent: Weak<RefCell<Struct>>,
    name: Option<String>,
    kind: Kind,
    comment: Option<Comment>,
    line_comment: Option<Comment>,
    value: Option<Value>,
    position: Position,
    align: i32,
    offset: i32,
    size: i32,
}

impl Field {
    fn new(parent: Weak<RefCell<Struct>>, name: Option<String>) -> Self {
        Self {
            parent,
            name,
            kind: Kind::None,
            comment: None,
            line_comment: None,
            value: None,
            position: Position::new(0, 0, 0),
            align: 0,
            offset: 0,
            size: 0,
        }
    }

    pub fn name(&self) -> &str {
        match &self.name {
            None => "",
            Some(n) => n.as_str(),
        }
    }
    pub fn parent(&self) -> &Weak<RefCell<Struct>> {
        &self.parent
    }
    pub fn kind(&self) -> &Kind {
        &self.kind
    }
    pub fn comment(&self) -> &Option<Comment> {
        &self.comment
    }
    pub fn line_comment(&self) -> &Option<Comment> {
        &self.line_comment
    }
    pub fn value(&self) -> &Option<Value> {
        &self.value
    }
    pub fn position(&self) -> Position {
        self.position
    }
    pub fn align(&self) -> i32 {
        self.align
    }
    pub fn offset(&self) -> i32 {
        self.offset
    }
    pub fn size(&self) -> i32 {
        self.size
    }

    pub fn set_name(&mut self, name: impl Into<String>) -> &mut Self {
        self.name = Some(name.into());
        self
    }
    pub fn set_kind(&mut self, kind: Kind) -> &mut Self {
        self.kind = kind;
        self
    }
    pub fn set_comment(&mut self, comment: Option<Comment>) -> &mut Self {
        self.comment = comment;
        self
    }
    pub fn set_line_comment(&mut self, line_comment: Option<Comment>) {
        self.line_comment = line_comment;
    }
    pub fn set_value(&mut self, value: Option<Value>) -> &mut Self {
        self.value = value;
        self
    }
    pub fn set_position(&mut self, position: Position) -> &mut Self {
        self.position = position;
        self
    }
    pub fn set_align(&mut self, align: i32) -> &mut Self {
        self.align = align;
        self
    }
    pub fn set_offset(&mut self, offset: i32) -> &mut Self {
        self.offset = offset;
        self
    }
    pub fn set_size(&mut self, size: i32) -> &mut Self {
        self.size = size;
        self
    }
    pub fn finish(&self) -> ModelResult<()> {
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Const {
    parent: ParentKind,
    this: Option<Weak<RefCell<Const>>>,
    comment: Option<Comment>,
    name: Option<String>,
    kind: Kind,
    value: Value,
}

impl Const {
    fn new(parent: ParentKind) -> Self {
        Self {
            parent,
            this: None,
            comment: None,
            name: None,
            kind: Kind::None,
            value: Value::Nil,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Value {
    Nil,
    Bool(bool),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    F32(f32),
    F64(f64),
    String(Ref<String>),
    Struct(Ref<Object>),
    Union(Ref<Object>),
    Variant(Ref<Object>),
    Enum(Ref<EnumOption>),
    Const(Ref<Const>),
    Object(Ref<Object>),
}

#[derive(Clone, Debug)]
pub struct Object {
    pub props: Vec<ObjectProperty>,
}

impl Object {
    pub fn new() -> Self {
        Self { props: Vec::new() }
    }
}

#[derive(Clone, Debug)]
pub struct ObjectProperty {
    pub name: Option<String>,
    pub field: Option<Ref<Field>>,
    pub value: Value,
}

impl ObjectProperty {
    pub fn new(name: Option<String>, value: Value) -> Self {
        Self {
            name,
            field: None,
            value,
        }
    }
}

pub struct CommentBuf {
    orphans: Vec<Comment>,
    active: Option<Comment>,
}

impl CommentBuf {
    pub fn new() -> Self {
        Self {
            orphans: Vec::new(),
            active: None,
        }
    }

    pub fn push(&mut self, multiline: bool, begin: Position, end: Position, text: &str) {
        let c = self.active.take();
        if c.is_none() {
            if multiline {
                self.active = Some(Comment::new(CommentLine::new(begin, end, text), true));
            } else {
                self.active = Some(Comment::new(CommentLine::new(begin, end, text), false));
            }
            return;
        }
        let mut active = c.unwrap();
        if active.is_single() {
            if multiline {
                self.orphans.push(active);
                self.active = Some(Comment::new(CommentLine::new(begin, end, text), true));
            } else if active.line.begin.line == begin.line - 1 {
                active.lines = Some(vec![
                    active.line.to_owned(),
                    CommentLine::new(begin, end, text),
                ]);
                self.active = Some(active);
            } else {
                self.orphans.push(active);
                self.active = Some(Comment::new(CommentLine::new(begin, end, text), false));
            }
        } else if active.is_group() {
            let mut lines = active.lines.as_mut().unwrap();
            if multiline {
                self.orphans.push(active);
                self.active = Some(Comment::new(CommentLine::new(begin, end, text), true));
            } else if !lines.is_empty() && lines.last().unwrap().begin.line == begin.line - 1 {
                lines.push(CommentLine::new(begin, end, text));
                self.active = Some(active);
            } else {
                self.orphans.push(active);
                self.active = Some(Comment::new(CommentLine::new(begin, end, text), false));
            }
        } else {
            let mut lines = active.lines.as_mut().unwrap();
            if multiline {
                if !lines.is_empty() && lines.last().unwrap().begin.line == begin.line - 1 {
                    lines.push(CommentLine::new(begin, end, text));
                    // active.lines = Some(lines);
                    self.active = Some(active);
                } else {
                    self.orphans.push(active);
                    self.active = Some(Comment::new(CommentLine::new(begin, end, text), true));
                }
            } else {
                self.orphans.push(active);
                self.active = Some(Comment::new(CommentLine::new(begin, end, text), false));
            }
        }
    }

    pub fn take(&mut self) -> Option<Comment> {
        self.active.take()
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Comment {
    line: CommentLine,
    lines: Option<Vec<CommentLine>>,
    multi: bool,
}

impl Comment {
    pub fn new(line: CommentLine, multi: bool) -> Self {
        if multi {
            Self {
                line: line.clone(),
                lines: Some(vec![line]),
                multi,
            }
        } else {
            Self {
                line,
                lines: None,
                multi,
            }
        }
    }

    pub fn is_single(&self) -> bool {
        !self.multi && self.lines.is_none()
    }

    pub fn is_group(&self) -> bool {
        !self.multi && self.lines.is_some()
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct CommentLine {
    pub begin: Position,
    pub end: Position,
    pub value: String,
}

impl CommentLine {
    pub fn new(begin: Position, end: Position, value: &str) -> Self {
        Self {
            begin,
            end,
            value: value.to_owned(),
        }
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        println!("dropped module");
    }
}

impl Drop for Struct {
    fn drop(&mut self) {
        println!("dropped Struct");
    }
}

impl Drop for Field {
    fn drop(&mut self) {
        eprintln!("dropped Field: {}", self.name());
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Error;

    use super::*;

    #[test]
    fn test_type() -> anyhow::Result<()> {
        let mut m = Module::new(Some("".to_owned()));

        {
            let children = &m.borrow().unwrap().children;
            println!("module children: {}", children.len());
        }

        let mut order_struct: Ref<Struct> = m
            .upgrade().unwrap().borrow_mut().deref_mut()
            .add_struct(Some("Order".to_owned()), StructKind::Struct)?;

        order_struct
            .borrow_mut().unwrap()
            .add_field(|f| f.set_name("id").set_kind(U64).finish())?;

        order_struct
            .borrow_mut().unwrap()
            .add_struct(Some("Line".to_owned()), StructKind::Struct)?;
        let order_struct_ref = order_struct.upgrade().unwrap().borrow_mut().deref_mut();

        {
            let children = &m.borrow().unwrap().children;
            println!("module children: {}", children.len());
        }

        {
            // let module = order_struct.borrow_mut().unwrap().module().unwrap();
            // let children = &order_struct.borrow_mut().unwrap().module().unwrap().borrow_mut().children;
            // println!("module children: {}", children.len());
        }

        let module = m.borrow().unwrap();

        for s in module.children().structs() {
            println!("struct: {}", s.name());
        }
        eprintln!("done");
        Ok(())
    }
}
