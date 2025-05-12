package bop.c.lmdb;

public class DBI {
  final Env env;
  final String name;
  final int dbi;
  final int flags;

  public DBI(Env env, String name, int dbi, int flags) {
    this.env = env;
    this.name = name;
    this.dbi = dbi;
    this.flags = flags;
  }
}
