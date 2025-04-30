package bop.c.mdbx;

public class MDBXError extends RuntimeException {
  public final int code;

  public MDBXError(int code) {
    this(code, "");
  }

  public MDBXError(int code, String message) {
    super("mdbx error: " + code + " (" + codeToName(code) + ") " + message);
    this.code = code;
  }

  public String name() {
    return codeToName(code);
  }

  public String getMessage() {
    return "mdbx error: " + code + " (" + codeToName(code) + ") " + super.getMessage();
  }

  static String codeToName(int code) {
    return switch (code) {
      case Error.SUCCESS -> "SUCCESS";
      case Error.RESULT_TRUE -> "RESULT_TRUE";
      case Error.KEYEXIST -> "KEYEXIST";
      case Error.NOTFOUND -> "NOTFOUND";
      case Error.PAGE_NOTFOUND -> "PAGE_NOTFOUND";
      case Error.CORRUPTED -> "CORRUPTED";
      case Error.PANIC -> "PANIC";
      case Error.VERSION_MISMATCH -> "VERSION_MISMATCH";
      case Error.INVALID -> "INVALID";
      case Error.MAP_FULL -> "MAP_FULL";
      case Error.DBS_FULL -> "DBS_FULL";
      case Error.READERS_FULL -> "READERS_FULL";
      case Error.TXN_FULL -> "TXN_FULL";
      case Error.CURSOR_FULL -> "CURSOR_FULL";
      case Error.PAGE_FULL -> "PAGE_FULL";
      case Error.UNABLE_EXTEND_MAPSIZE -> "UNABLE_EXTEND_MAPSIZE";
      case Error.INCOMPATIBLE -> "INCOMPATIBLE";
      case Error.BAD_RSLOT -> "BAD_RSLOT";
      case Error.BAD_TXN -> "BAD_TXN";
      case Error.BAD_VALSIZE -> "BAD_VALSIZE";
      case Error.BAD_DBI -> "BAD_DBI";
      case Error.PROBLEM -> "PROBLEM";
      case Error.BUSY -> "BUSY";
      case Error.EMULTIVAL -> "EMULTIVAL";
      case Error.EBADSIGN -> "EBADSIGN";
      case Error.WANNA_RECOVERY -> "WANNA_RECOVERY";
      case Error.EKEYMISMATCH -> "EKEYMISMATCH";
      case Error.TOO_LARGE -> "TOO_LARGE";
      case Error.THREAD_MISMATCH -> "THREAD_MISMATCH";
      case Error.TXN_OVERLAPPING -> "TXN_OVERLAPPING";
      case Error.BACKLOG_DEPLETED -> "BACKLOG_DEPLETED";
      case Error.DUPLICATED_CLK -> "DUPLICATED_CLK";
      case Error.DANGLING_DBI -> "DANGLING_DBI";
      case Error.OUSTED -> "OUSTED";
      case Error.MVCC_RETARDED -> "MVCC_REARDED";
      default -> {
        if (code == Error.ENODATA) yield "ENODATA";
        else if (code == Error.EINVAL) yield "EINVAL";
        else if (code == Error.EACCESS) yield "EACCESS";
        else if (code == Error.ENOMEM) yield "ENOMEM";
        else if (code == Error.EROFS) yield "EROFS";
        else if (code == Error.ENOSYS) yield "ENOSYS";
        else if (code == Error.EIO) yield "EIO";
        else if (code == Error.EPERM) yield "EPERM";
        else if (code == Error.EINTR) yield "EINTR";
        else if (code == Error.ENOFILE) yield "ENOFILE";
        else if (code == Error.EREMOTE) yield "EREMOTE";
        else if (code == Error.EDEADLK) yield "EDEADLK";
        else yield "UNKNOWN ERROR";
      }
    };
  }
}
