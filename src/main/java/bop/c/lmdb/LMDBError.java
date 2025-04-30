package bop.c.lmdb;

public class LMDBError extends RuntimeException {
  public LMDBError(int code) {
    this(code, "");
  }

  public LMDBError(int code, String message) {
    super("LMDB error code: " + code + " message: " + Code.message(code) + " " + message);
  }
}
