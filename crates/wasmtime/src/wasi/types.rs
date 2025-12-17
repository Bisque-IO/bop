//! Common WASI types shared between Preview 1 and Preview 2.

use std::io;

/// WASI error codes (errno).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum Errno {
    Success = 0,
    TooBig = 1,
    Access = 2,
    AddrInUse = 3,
    AddrNotAvail = 4,
    AfNoSupport = 5,
    Again = 6,
    Already = 7,
    BadF = 8,
    BadMsg = 9,
    Busy = 10,
    Canceled = 11,
    Child = 12,
    ConnAborted = 13,
    ConnRefused = 14,
    ConnReset = 15,
    Deadlk = 16,
    DestAddrReq = 17,
    Dom = 18,
    DQuot = 19,
    Exist = 20,
    Fault = 21,
    FBig = 22,
    HostUnreach = 23,
    IdrM = 24,
    Ilseq = 25,
    InProgress = 26,
    Intr = 27,
    Inval = 28,
    Io = 29,
    IsConn = 30,
    IsDir = 31,
    Loop = 32,
    MFile = 33,
    MLink = 34,
    MsgSize = 35,
    Multihop = 36,
    NameTooLong = 37,
    NetDown = 38,
    NetReset = 39,
    NetUnreach = 40,
    NFile = 41,
    NoBufS = 42,
    NoDev = 43,
    NoEnt = 44,
    NoExec = 45,
    NoLck = 46,
    NoLink = 47,
    NoMem = 48,
    NoMsg = 49,
    NoProtoOpt = 50,
    NoSpc = 51,
    NoSys = 52,
    NotConn = 53,
    NotDir = 54,
    NotEmpty = 55,
    NotRecoverable = 56,
    NotSock = 57,
    NotSup = 58,
    NoTty = 59,
    NxIo = 60,
    Overflow = 61,
    OwnerDead = 62,
    Perm = 63,
    Pipe = 64,
    Proto = 65,
    ProtoNoSupport = 66,
    Prototype = 67,
    Range = 68,
    RoFs = 69,
    SPipe = 70,
    Srch = 71,
    Stale = 72,
    TimedOut = 73,
    TxtBsy = 74,
    XDev = 75,
    NotCapable = 76,
}

impl Errno {
    pub fn raw(self) -> u16 {
        self as u16
    }
}

impl From<io::ErrorKind> for Errno {
    fn from(kind: io::ErrorKind) -> Self {
        match kind {
            io::ErrorKind::NotFound => Errno::NoEnt,
            io::ErrorKind::PermissionDenied => Errno::Access,
            io::ErrorKind::ConnectionRefused => Errno::ConnRefused,
            io::ErrorKind::ConnectionReset => Errno::ConnReset,
            io::ErrorKind::ConnectionAborted => Errno::ConnAborted,
            io::ErrorKind::NotConnected => Errno::NotConn,
            io::ErrorKind::AddrInUse => Errno::AddrInUse,
            io::ErrorKind::AddrNotAvailable => Errno::AddrNotAvail,
            io::ErrorKind::BrokenPipe => Errno::Pipe,
            io::ErrorKind::AlreadyExists => Errno::Exist,
            io::ErrorKind::WouldBlock => Errno::Again,
            io::ErrorKind::InvalidInput => Errno::Inval,
            io::ErrorKind::InvalidData => Errno::Inval,
            io::ErrorKind::TimedOut => Errno::TimedOut,
            io::ErrorKind::WriteZero => Errno::Io,
            io::ErrorKind::Interrupted => Errno::Intr,
            io::ErrorKind::UnexpectedEof => Errno::Io,
            io::ErrorKind::OutOfMemory => Errno::NoMem,
            _ => Errno::Io,
        }
    }
}

impl From<io::Error> for Errno {
    fn from(err: io::Error) -> Self {
        #[cfg(unix)]
        if let Some(code) = err.raw_os_error() {
            return errno_from_unix(code);
        }
        err.kind().into()
    }
}

#[cfg(unix)]
fn errno_from_unix(code: i32) -> Errno {
    match code {
        libc::EPERM => Errno::Perm,
        libc::ENOENT => Errno::NoEnt,
        libc::ESRCH => Errno::Srch,
        libc::EINTR => Errno::Intr,
        libc::EIO => Errno::Io,
        libc::ENXIO => Errno::NxIo,
        libc::E2BIG => Errno::TooBig,
        libc::ENOEXEC => Errno::NoExec,
        libc::EBADF => Errno::BadF,
        libc::ECHILD => Errno::Child,
        libc::EAGAIN => Errno::Again,
        libc::ENOMEM => Errno::NoMem,
        libc::EACCES => Errno::Access,
        libc::EFAULT => Errno::Fault,
        libc::EBUSY => Errno::Busy,
        libc::EEXIST => Errno::Exist,
        libc::EXDEV => Errno::XDev,
        libc::ENODEV => Errno::NoDev,
        libc::ENOTDIR => Errno::NotDir,
        libc::EISDIR => Errno::IsDir,
        libc::EINVAL => Errno::Inval,
        libc::ENFILE => Errno::NFile,
        libc::EMFILE => Errno::MFile,
        libc::ENOTTY => Errno::NoTty,
        libc::ETXTBSY => Errno::TxtBsy,
        libc::EFBIG => Errno::FBig,
        libc::ENOSPC => Errno::NoSpc,
        libc::ESPIPE => Errno::SPipe,
        libc::EROFS => Errno::RoFs,
        libc::EMLINK => Errno::MLink,
        libc::EPIPE => Errno::Pipe,
        libc::EDOM => Errno::Dom,
        libc::ERANGE => Errno::Range,
        libc::EDEADLK => Errno::Deadlk,
        libc::ENAMETOOLONG => Errno::NameTooLong,
        libc::ENOLCK => Errno::NoLck,
        libc::ENOSYS => Errno::NoSys,
        libc::ENOTEMPTY => Errno::NotEmpty,
        libc::ELOOP => Errno::Loop,
        libc::ENOMSG => Errno::NoMsg,
        libc::EOVERFLOW => Errno::Overflow,
        libc::EILSEQ => Errno::Ilseq,
        libc::ENOTSOCK => Errno::NotSock,
        libc::EDESTADDRREQ => Errno::DestAddrReq,
        libc::EMSGSIZE => Errno::MsgSize,
        libc::EPROTOTYPE => Errno::Prototype,
        libc::ENOPROTOOPT => Errno::NoProtoOpt,
        libc::EPROTONOSUPPORT => Errno::ProtoNoSupport,
        libc::ENOTSUP => Errno::NotSup,
        libc::EAFNOSUPPORT => Errno::AfNoSupport,
        libc::EADDRINUSE => Errno::AddrInUse,
        libc::EADDRNOTAVAIL => Errno::AddrNotAvail,
        libc::ENETDOWN => Errno::NetDown,
        libc::ENETUNREACH => Errno::NetUnreach,
        libc::ENETRESET => Errno::NetReset,
        libc::ECONNABORTED => Errno::ConnAborted,
        libc::ECONNRESET => Errno::ConnReset,
        libc::ENOBUFS => Errno::NoBufS,
        libc::EISCONN => Errno::IsConn,
        libc::ENOTCONN => Errno::NotConn,
        libc::ETIMEDOUT => Errno::TimedOut,
        libc::ECONNREFUSED => Errno::ConnRefused,
        libc::EHOSTUNREACH => Errno::HostUnreach,
        libc::EALREADY => Errno::Already,
        libc::EINPROGRESS => Errno::InProgress,
        libc::ESTALE => Errno::Stale,
        libc::ECANCELED => Errno::Canceled,
        #[cfg(target_os = "linux")]
        libc::EDQUOT => Errno::DQuot,
        #[cfg(target_os = "linux")]
        libc::EMULTIHOP => Errno::Multihop,
        #[cfg(target_os = "linux")]
        libc::ENOLINK => Errno::NoLink,
        _ => Errno::Io,
    }
}

/// File type (used in directory entries and stats).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Filetype {
    Unknown = 0,
    BlockDevice = 1,
    CharacterDevice = 2,
    Directory = 3,
    RegularFile = 4,
    SocketDgram = 5,
    SocketStream = 6,
    SymbolicLink = 7,
}

/// Rights bitmap for file descriptor capabilities.
#[derive(Debug, Clone, Copy, Default)]
pub struct Rights(pub u64);

impl Rights {
    pub const FD_DATASYNC: Rights = Rights(1 << 0);
    pub const FD_READ: Rights = Rights(1 << 1);
    pub const FD_SEEK: Rights = Rights(1 << 2);
    pub const FD_FDSTAT_SET_FLAGS: Rights = Rights(1 << 3);
    pub const FD_SYNC: Rights = Rights(1 << 4);
    pub const FD_TELL: Rights = Rights(1 << 5);
    pub const FD_WRITE: Rights = Rights(1 << 6);
    pub const FD_ADVISE: Rights = Rights(1 << 7);
    pub const FD_ALLOCATE: Rights = Rights(1 << 8);
    pub const PATH_CREATE_DIRECTORY: Rights = Rights(1 << 9);
    pub const PATH_CREATE_FILE: Rights = Rights(1 << 10);
    pub const PATH_LINK_SOURCE: Rights = Rights(1 << 11);
    pub const PATH_LINK_TARGET: Rights = Rights(1 << 12);
    pub const PATH_OPEN: Rights = Rights(1 << 13);
    pub const FD_READDIR: Rights = Rights(1 << 14);
    pub const PATH_READLINK: Rights = Rights(1 << 15);
    pub const PATH_RENAME_SOURCE: Rights = Rights(1 << 16);
    pub const PATH_RENAME_TARGET: Rights = Rights(1 << 17);
    pub const PATH_FILESTAT_GET: Rights = Rights(1 << 18);
    pub const PATH_FILESTAT_SET_SIZE: Rights = Rights(1 << 19);
    pub const PATH_FILESTAT_SET_TIMES: Rights = Rights(1 << 20);
    pub const FD_FILESTAT_GET: Rights = Rights(1 << 21);
    pub const FD_FILESTAT_SET_SIZE: Rights = Rights(1 << 22);
    pub const FD_FILESTAT_SET_TIMES: Rights = Rights(1 << 23);
    pub const PATH_SYMLINK: Rights = Rights(1 << 24);
    pub const PATH_REMOVE_DIRECTORY: Rights = Rights(1 << 25);
    pub const PATH_UNLINK_FILE: Rights = Rights(1 << 26);
    pub const POLL_FD_READWRITE: Rights = Rights(1 << 27);
    pub const SOCK_SHUTDOWN: Rights = Rights(1 << 28);
    pub const SOCK_ACCEPT: Rights = Rights(1 << 29);

    // Networking Aliases/Extensions
    // Reuse FD_READ/FD_WRITE for receive/send as `sock_recv` checks FD_READ
    pub const SOCK_RECV: Rights = Rights::FD_READ;
    pub const SOCK_SEND: Rights = Rights::FD_WRITE;
    // SOCK_LISTEN: Not standard, but useful for FdTable tracking.
    // We can use a free bit or alias. Using a free bit to distinguish.
    pub const SOCK_LISTEN: Rights = Rights(1 << 30);

    pub const ALL: Rights = Rights(u64::MAX);

    pub const DIR_BASE: Rights = Rights(
        Rights::FD_FDSTAT_SET_FLAGS.0
            | Rights::FD_SYNC.0
            | Rights::FD_ADVISE.0
            | Rights::PATH_CREATE_DIRECTORY.0
            | Rights::PATH_CREATE_FILE.0
            | Rights::PATH_LINK_SOURCE.0
            | Rights::PATH_LINK_TARGET.0
            | Rights::PATH_OPEN.0
            | Rights::FD_READDIR.0
            | Rights::PATH_READLINK.0
            | Rights::PATH_RENAME_SOURCE.0
            | Rights::PATH_RENAME_TARGET.0
            | Rights::PATH_FILESTAT_GET.0
            | Rights::PATH_FILESTAT_SET_SIZE.0
            | Rights::PATH_FILESTAT_SET_TIMES.0
            | Rights::FD_FILESTAT_GET.0
            | Rights::FD_FILESTAT_SET_TIMES.0
            | Rights::PATH_SYMLINK.0
            | Rights::PATH_REMOVE_DIRECTORY.0
            | Rights::PATH_UNLINK_FILE.0,
    );

    pub const DIR_INHERITING: Rights = Rights(Rights::ALL.0);

    pub const FILE_BASE: Rights = Rights(
        Rights::FD_DATASYNC.0
            | Rights::FD_READ.0
            | Rights::FD_SEEK.0
            | Rights::FD_FDSTAT_SET_FLAGS.0
            | Rights::FD_SYNC.0
            | Rights::FD_TELL.0
            | Rights::FD_WRITE.0
            | Rights::FD_ADVISE.0
            | Rights::FD_ALLOCATE.0
            | Rights::FD_FILESTAT_GET.0
            | Rights::FD_FILESTAT_SET_SIZE.0
            | Rights::FD_FILESTAT_SET_TIMES.0
            | Rights::POLL_FD_READWRITE.0,
    );

    pub fn contains(self, other: Rights) -> bool {
        (self.0 & other.0) == other.0
    }

    pub fn raw(self) -> u64 {
        self.0
    }
}

impl std::ops::BitOr for Rights {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Rights(self.0 | rhs.0)
    }
}

impl std::ops::BitAnd for Rights {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Rights(self.0 & rhs.0)
    }
}

/// File descriptor flags.
#[derive(Debug, Clone, Copy, Default)]
pub struct FdFlags(pub u16);

impl FdFlags {
    pub const APPEND: FdFlags = FdFlags(1 << 0);
    pub const DSYNC: FdFlags = FdFlags(1 << 1);
    pub const NONBLOCK: FdFlags = FdFlags(1 << 2);
    pub const RSYNC: FdFlags = FdFlags(1 << 3);
    pub const SYNC: FdFlags = FdFlags(1 << 4);

    pub fn contains(self, other: FdFlags) -> bool {
        (self.0 & other.0) == other.0
    }

    pub fn raw(self) -> u16 {
        self.0
    }
}

impl std::ops::BitOr for FdFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        FdFlags(self.0 | rhs.0)
    }
}

/// Open flags for path_open.
#[derive(Debug, Clone, Copy, Default)]
pub struct OFlags(pub u16);

impl OFlags {
    pub const CREATE: OFlags = OFlags(1 << 0);
    pub const DIRECTORY: OFlags = OFlags(1 << 1);
    pub const EXCL: OFlags = OFlags(1 << 2);
    pub const TRUNC: OFlags = OFlags(1 << 3);

    pub fn contains(self, other: OFlags) -> bool {
        (self.0 & other.0) == other.0
    }

    pub fn raw(self) -> u16 {
        self.0
    }
}

/// Lookup flags for path operations.
#[derive(Debug, Clone, Copy, Default)]
pub struct LookupFlags(pub u32);

impl LookupFlags {
    pub const SYMLINK_FOLLOW: LookupFlags = LookupFlags(1 << 0);

    pub fn contains(self, other: LookupFlags) -> bool {
        (self.0 & other.0) == other.0
    }
}

/// Clock identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ClockId {
    Realtime = 0,
    Monotonic = 1,
    ProcessCpuTimeId = 2,
    ThreadCpuTimeId = 3,
}

impl ClockId {
    pub fn from_raw(v: u32) -> Option<Self> {
        match v {
            0 => Some(ClockId::Realtime),
            1 => Some(ClockId::Monotonic),
            2 => Some(ClockId::ProcessCpuTimeId),
            3 => Some(ClockId::ThreadCpuTimeId),
            _ => None,
        }
    }
}

/// Whence for seeking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Whence {
    Set = 0,
    Cur = 1,
    End = 2,
}

impl Whence {
    pub fn from_raw(v: u8) -> Option<Self> {
        match v {
            0 => Some(Whence::Set),
            1 => Some(Whence::Cur),
            2 => Some(Whence::End),
            _ => None,
        }
    }
}

/// Advice for fd_advise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Advice {
    Normal = 0,
    Sequential = 1,
    Random = 2,
    WillNeed = 3,
    DontNeed = 4,
    NoReuse = 5,
}

impl Advice {
    pub fn from_raw(v: u8) -> Option<Self> {
        match v {
            0 => Some(Advice::Normal),
            1 => Some(Advice::Sequential),
            2 => Some(Advice::Random),
            3 => Some(Advice::WillNeed),
            4 => Some(Advice::DontNeed),
            5 => Some(Advice::NoReuse),
            _ => None,
        }
    }
}

/// File stat structure.
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct Filestat {
    pub dev: u64,
    pub ino: u64,
    pub filetype: u8,
    pub nlink: u64,
    pub size: u64,
    pub atim: u64,
    pub mtim: u64,
    pub ctim: u64,
}

/// Prestat (for fd_prestat_get).
#[derive(Debug, Clone, Copy)]
pub enum Prestat {
    Dir { pr_name_len: u32 },
}

/// Subscription type for poll_oneoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EventType {
    Clock = 0,
    FdRead = 1,
    FdWrite = 2,
}

impl EventType {
    pub fn from_raw(v: u8) -> Option<Self> {
        match v {
            0 => Some(EventType::Clock),
            1 => Some(EventType::FdRead),
            2 => Some(EventType::FdWrite),
            _ => None,
        }
    }
}

/// Subscription clock flags.
#[derive(Debug, Clone, Copy, Default)]
pub struct SubclockFlags(pub u16);

impl SubclockFlags {
    pub const ABSTIME: SubclockFlags = SubclockFlags(1 << 0);

    pub fn contains(self, other: SubclockFlags) -> bool {
        (self.0 & other.0) == other.0
    }
}

/// Event read/write flags.
#[derive(Debug, Clone, Copy, Default)]
pub struct EventRwFlags(pub u16);

impl EventRwFlags {
    pub const HANGUP: EventRwFlags = EventRwFlags(1 << 0);
}

/// Signal numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Signal {
    None = 0,
    Hup = 1,
    Int = 2,
    Quit = 3,
    Ill = 4,
    Trap = 5,
    Abrt = 6,
    Bus = 7,
    Fpe = 8,
    Kill = 9,
    Usr1 = 10,
    Segv = 11,
    Usr2 = 12,
    Pipe = 13,
    Alrm = 14,
    Term = 15,
}

impl Signal {
    pub fn from_raw(v: u8) -> Option<Self> {
        match v {
            0 => Some(Signal::None),
            1 => Some(Signal::Hup),
            2 => Some(Signal::Int),
            3 => Some(Signal::Quit),
            4 => Some(Signal::Ill),
            5 => Some(Signal::Trap),
            6 => Some(Signal::Abrt),
            7 => Some(Signal::Bus),
            8 => Some(Signal::Fpe),
            9 => Some(Signal::Kill),
            10 => Some(Signal::Usr1),
            11 => Some(Signal::Segv),
            12 => Some(Signal::Usr2),
            13 => Some(Signal::Pipe),
            14 => Some(Signal::Alrm),
            15 => Some(Signal::Term),
            _ => None,
        }
    }
}

/// Socket shutdown flags.
#[derive(Debug, Clone, Copy, Default)]
pub struct SdFlags(pub u8);

impl SdFlags {
    pub const RD: SdFlags = SdFlags(1 << 0);
    pub const WR: SdFlags = SdFlags(1 << 1);

    pub fn contains(self, other: SdFlags) -> bool {
        (self.0 & other.0) == other.0
    }
}

/// Receive flags for sock_recv.
#[derive(Debug, Clone, Copy, Default)]
pub struct RiFlags(pub u16);

impl RiFlags {
    pub const PEEK: RiFlags = RiFlags(1 << 0);
    pub const WAITALL: RiFlags = RiFlags(1 << 1);

    pub fn contains(self, other: RiFlags) -> bool {
        (self.0 & other.0) == other.0
    }
}

/// Return flags for sock_recv.
#[derive(Debug, Clone, Copy, Default)]
pub struct RoFlags(pub u16);

impl RoFlags {
    pub const DATA_TRUNCATED: RoFlags = RoFlags(1 << 0);
}

/// Send flags for sock_send.
#[derive(Debug, Clone, Copy, Default)]
pub struct SiFlags(pub u16);

/// Timestamps to set.
#[derive(Debug, Clone, Copy, Default)]
pub struct FstFlags(pub u16);

impl FstFlags {
    pub const ATIM: FstFlags = FstFlags(1 << 0);
    pub const ATIM_NOW: FstFlags = FstFlags(1 << 1);
    pub const MTIM: FstFlags = FstFlags(1 << 2);
    pub const MTIM_NOW: FstFlags = FstFlags(1 << 3);

    pub fn contains(self, other: FstFlags) -> bool {
        (self.0 & other.0) == other.0
    }
}
