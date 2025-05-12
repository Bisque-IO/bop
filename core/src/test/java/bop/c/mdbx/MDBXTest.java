package bop.c.mdbx;

import bop.bench.Bench;
import bop.c.LongRef;
import bop.c.Memory;
import bop.unsafe.Danger;
import org.junit.jupiter.api.Assertions;
import org.junit.jupiter.api.Test;

public class MDBXTest {
  static final int DEFAULT_ENV_FLAGS = Env.Flags.NOMEMINIT
      | Env.Flags.MAPASYNC
      | Env.Flags.SAFE_NOSYNC
      | Env.Flags.LIFORECLAIM
      | Env.Flags.NOSTICKYTHREADS
      | Env.Flags.NOSUBDIR
      | Env.Flags.WRITEMAP;

  static final int MODE = 755;

  static final String DB_NAME = "mydb.db";

  @Test
  public void buildInfo() {
    System.out.println(BuildInfo.get());
  }

  @Test
  public void deleteEnv() {
    final var env = Env.create(DB_NAME);
    Assertions.assertEquals(Error.SUCCESS, env.open(DEFAULT_ENV_FLAGS, MODE));
    Assertions.assertEquals(Error.SUCCESS, env.doClose());
    Assertions.assertEquals(Error.SUCCESS, Env.delete(DB_NAME, Env.DeleteMode.JUST_DELETE));
  }

  @Test
  public void openExisting() {
    final var env = Env.create(DB_NAME);
    Assertions.assertEquals(
        Error.SUCCESS, env.geometry(1024 * 512, 1024 * 1024, 1024 * 1024, 1024 * 512, 0, 4096));
    Assertions.assertEquals(Error.SUCCESS, env.option(Env.Option.MAX_READERS, 120L));
    Assertions.assertEquals(Error.SUCCESS, env.option(Env.Option.MAX_DB, 120L));
    Assertions.assertEquals(Error.SUCCESS, env.option(Env.Option.PREFAULT_WRITE_ENABLE, 1));
    Assertions.assertEquals(Error.SUCCESS, env.open(DEFAULT_ENV_FLAGS, MODE));
    final var txn = env.begin(null, Txn.Flags.RDONLY);
    final var mainDb = Table.create("main");
    mainDb.flags = Table.Flags.INT_KEY;
    Assertions.assertEquals(Error.SUCCESS, txn.open(mainDb));
    Assertions.assertEquals(Error.SUCCESS, txn.get(mainDb, 9L));
    Assertions.assertEquals(Error.RESULT_TRUE, txn.getEqualOrGreater(mainDb, 0L));
    System.out.println(txn.key.asLong() + ": " + txn.data.asString());
    Assertions.assertEquals(Error.SUCCESS, txn.abort());

    Assertions.assertEquals(Error.SUCCESS, env.doClose());
  }

  @Test
  public void openEnv() {
    Env.delete(DB_NAME, Env.DeleteMode.JUST_DELETE);
    final var env = Env.create(DB_NAME);
    Assertions.assertEquals(
        Error.SUCCESS, env.geometry(1024 * 512, 1024 * 1024, 1024 * 1024, 1024 * 512, 0, 4096));
    Assertions.assertEquals(Error.SUCCESS, env.option(Env.Option.MAX_READERS, 120L));
    Assertions.assertEquals(Error.SUCCESS, env.option(Env.Option.MAX_DB, 120L));
    Assertions.assertEquals(Error.SUCCESS, env.option(Env.Option.PREFAULT_WRITE_ENABLE, 1));
    Assertions.assertEquals(Error.SUCCESS, env.open(DEFAULT_ENV_FLAGS, MODE));

    final var txn = env.begin(null, Txn.Flags.READWRITE);
    final var dbi = new Table();
    dbi.init("main");
    dbi.flags = Table.Flags.CREATE | Table.Flags.INT_KEY;
    Assertions.assertEquals(Error.SUCCESS, txn.open(dbi));

    txn.put(dbi, 9L, "hello there: " + System.nanoTime());
    Assertions.assertEquals(Error.SUCCESS, txn.commit());
    env.sync(true, false);

    final var info = env.info();

    final var stat = env.stat();
    System.out.println(stat);

    Assertions.assertEquals(Error.SUCCESS, env.doClose());
  }

  @Test
  public void sysRamInfo() {
    try (final var info = RAMInfo.create()) {
      System.out.println(info);
    }
  }

  @Test
  public void versionInfo() {
    System.out.println(VersionInfo.get());
  }

  @Test
  public void beginTxn() throws Throwable {
    Env.delete(DB_NAME, Env.DeleteMode.JUST_DELETE);
    final var env = Env.create(DB_NAME);
    Assertions.assertEquals(
        Error.SUCCESS,
        env.geometry(1024 * 1024, 1024 * 1024, 1024 * 1024 * 64, 1024 * 1024, 0, 4096));
    Assertions.assertEquals(Error.SUCCESS, env.option(Env.Option.MAX_READERS, 100L));
    Assertions.assertEquals(Error.SUCCESS, env.option(Env.Option.MAX_DB, 128L));
    Assertions.assertEquals(Error.SUCCESS, env.open(DEFAULT_ENV_FLAGS, MODE));
    final var txn = env.begin(null, Txn.Flags.READWRITE, 0L);
    Assertions.assertTrue(txn.txnPointer != 0L);

    final var dbi = new Table();
    dbi.init("main");
    dbi.flags = Table.Flags.CREATE | Table.Flags.INT_KEY;
    int err = txn.open(dbi);
    Assertions.assertEquals(Error.SUCCESS, err);

    err = txn.put(dbi, 7L, "hello");
    Assertions.assertEquals(Error.SUCCESS, err);

    final var stat = txn.stat();
    final var info = txn.info(true);
    System.out.println(stat);
    System.out.println(info);

    final var key = Val.allocate();
    final var data = Val.allocate();


    err = txn.put(dbi, 9L, "hello world", PutFlags.APPEND);
    Assertions.assertEquals(Error.SUCCESS, err);

    data.clear();
    err = txn.get(dbi, 9L, data);
    Assertions.assertEquals(Error.SUCCESS, err);

    var keyBase = key.base();
    var keyLen = key.len();
    var dataBase = data.base();
    var dataLen = data.len();
    var dataBytes = data.asBytes();
    var dataStr = data.asString();

//    Assertions.assertEquals("hello world", dataStr);

    final var cursor = Cursor.create();
    err = cursor.bind(txn, dbi);
    Assertions.assertEquals(Error.SUCCESS, err);
    err = cursor.put(3L, "bye");
    Assertions.assertEquals(Error.SUCCESS, err);
    err = cursor.put(1L, "bye bye");
    Assertions.assertEquals(Error.SUCCESS, err);
    err = cursor.put(2L, "hi bye");
    Assertions.assertEquals(Error.SUCCESS, err);

    err = txn.put(dbi, 10L, "hello");
    err = txn.delete(dbi, 10L);
    err = cursor.put(10L, "hi hi", PutFlags.UPSERT);
    err = cursor.get(10L, Cursor.Op.SET_KEY);
    err = cursor.delete(PutFlags.CURRENT);
    err = cursor.get(2L, Cursor.Op.SET_KEY);

    final var CYCLES = 5;
    final var ITERS = 10_000_000;

    Bench.printHeader();
    Bench.threaded("txn.get", 1, CYCLES, ITERS, (threadId, cycle, iteration) -> {
      Assertions.assertEquals(Error.SUCCESS, txn.get(dbi, iteration % 2 == 0 ? 3L : 9L));
    });
    Bench.threaded("txn.put", 1, CYCLES, ITERS, (threadId, cycle, iteration) -> {
      Assertions.assertEquals(Error.SUCCESS, txn.put(dbi, iteration % 2 == 0 ? 3L : 9L, "hello"));
    });
    Bench.threaded("cursor.get FIRST/LAST", 1, CYCLES, ITERS, (threadId, cycle, iteration) -> {
      Assertions.assertEquals(
          Error.SUCCESS, cursor.get(iteration % 2 == 0 ? Cursor.Op.FIRST : Cursor.Op.LAST));
    });
    Bench.threaded("cursor.get", 1, CYCLES, ITERS, (threadId, cycle, iteration) -> {
      Assertions.assertEquals(
          Error.SUCCESS, cursor.get(iteration % 2 == 0 ? 3L : 9L, Cursor.Op.SET_KEY));
    });
    Bench.threaded("cursor.put", 1, CYCLES, ITERS, (threadId, cycle, iteration) -> {
      Assertions.assertEquals(
          Error.SUCCESS, cursor.put(iteration % 2 == 0 ? 3L : 9L, "hello", PutFlags.UPSERT));
    });
    Bench.printFooter();

    //    Bench.printHeader();
    //    Bench.threaded("cursor.get", 1, 10, 10000000, (threadId, cycle, iteration) -> {
    //      cursor.get();
    //    });
    //    Bench.printFooter();

    err = txn.get(dbi, 3L);
    dataStr = txn.data.asString();
    Assertions.assertEquals(Error.SUCCESS, err);

    //    err = cursor.get(0L, Cursor.Op.SET_LOWERBOUND);
    err = cursor.get(Cursor.Op.FIRST);
    Assertions.assertEquals(Error.SUCCESS, err);
    var firstValue = cursor.key.asLong();
    var firstData = cursor.data.asString();
    err = cursor.get(Cursor.Op.NEXT);
    Assertions.assertEquals(Error.SUCCESS, err);
    var secondValue = cursor.key.asLong();
    var secondData = cursor.data.asString();
    err = cursor.get(Cursor.Op.NEXT);
    Assertions.assertEquals(Error.SUCCESS, err);
    var thirdValue = cursor.key.asLong();
    var thirdData = cursor.data.asString();
    err = cursor.get(Cursor.Op.NEXT);
    Assertions.assertEquals(Error.SUCCESS, err);
    var fourthValue = cursor.key.asLong();
    var fourthData = cursor.data.asString();
    err = cursor.get(Cursor.Op.NEXT);
    Assertions.assertEquals(Error.SUCCESS, err);
    err = cursor.get(Cursor.Op.NEXT);
    Assertions.assertEquals(Error.NOTFOUND, err);

    Assertions.assertEquals(Error.SUCCESS, txn.commit(true));
    System.out.println(txn.commitLatency());
    final var syncErr = env.sync();
    Assertions.assertTrue(syncErr == Error.SUCCESS || syncErr == Error.RESULT_TRUE);
    System.out.println("EnvInfo: " + env.info());
    System.out.println("EnvStat: " + env.stat());

    Assertions.assertEquals(Error.SUCCESS, env.doClose());
  }

  @Test
  public void setOptions() {
    Env.delete(DB_NAME, Env.DeleteMode.JUST_DELETE);
    final var env = Env.create(DB_NAME);
    Assertions.assertEquals(Error.SUCCESS, env.option(Env.Option.MAX_DB, 128));
    Assertions.assertEquals(128L, env.option(Env.Option.MAX_DB));
    Assertions.assertEquals(Error.SUCCESS, env.option(Env.Option.MAX_READERS, 256));
    Assertions.assertEquals(256L, env.option(Env.Option.MAX_READERS));
    Assertions.assertEquals(Error.SUCCESS, env.doClose());
  }
}
