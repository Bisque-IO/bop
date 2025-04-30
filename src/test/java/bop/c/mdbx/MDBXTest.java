package bop.c.mdbx;

import bop.c.LongRef;
import bop.c.Memory;
import bop.unsafe.Danger;
import org.junit.jupiter.api.Assertions;
import org.junit.jupiter.api.Test;

public class MDBXTest {
  static final int DEFAULT_ENV_FLAGS = Env.Flags.NOMEMINIT
      | Env.Flags.MAPASYNC
      | Env.Flags.SYNC_DURABLE
      | Env.Flags.LIFORECLAIM
      | Env.Flags.NOSTICKYTHREADS
      | Env.Flags.NOSUBDIR
      | Env.Flags.WRITEMAP;

  static final int MODE = 644;

  static final String DB_NAME = "mydb.db";

  @Test
  public void deleteEnv() {
    final var env = Env.create(DB_NAME);
    Assertions.assertEquals(Error.SUCCESS, env.open(DEFAULT_ENV_FLAGS, MODE));
    Assertions.assertEquals(Error.SUCCESS, env.close());
    Assertions.assertEquals(Error.SUCCESS, Env.delete(DB_NAME, Env.DeleteMode.JUST_DELETE));
  }

  @Test
  public void openEnv() {
    Env.delete(DB_NAME, Env.DeleteMode.JUST_DELETE);
    final var env = Env.create(DB_NAME);
    Assertions.assertEquals(
        Error.SUCCESS, env.setGeometry(1024 * 512, 1024 * 1024, 1024 * 1024, 1024 * 512, 0, 4096));
    Assertions.assertEquals(Error.SUCCESS, env.setOption(Env.Option.MAX_READERS, 120L));
    Assertions.assertEquals(Error.SUCCESS, env.open(DEFAULT_ENV_FLAGS, MODE));

    env.sync(true, false);

    final var info = env.info();

    final var stat = env.stat();
    System.out.println(stat);

    Assertions.assertEquals(Error.SUCCESS, env.close());
  }

  @Test
  public void beginTxn() {
    Env.delete(DB_NAME, Env.DeleteMode.JUST_DELETE);
    final var env = Env.create(DB_NAME);
    Assertions.assertEquals(
        Error.SUCCESS, env.setGeometry(1024 * 512, 1024 * 1024, 1024 * 1024, 1024 * 512, 0, 4096));
    Assertions.assertEquals(Error.SUCCESS, env.setOption(Env.Option.MAX_READERS, 100L));
    Assertions.assertEquals(Error.SUCCESS, env.setOption(Env.Option.MAX_DB, 128L));
    Assertions.assertEquals(Error.SUCCESS, env.open(DEFAULT_ENV_FLAGS, MODE));
    final var txn = env.begin(null, Txn.Flags.READWRITE, 0L);
    Assertions.assertTrue(txn.txnPointer != 0L);

    final var dbi = new Dbi();
    dbi.init("main");
    dbi.flags = Dbi.Flags.CREATE | Dbi.Flags.INTEGERKEY;
    int err = txn.open(dbi);
    Assertions.assertEquals(Error.SUCCESS, err);

    final var stat = txn.stat();
    final var info = txn.info(true);
    System.out.println(stat);
    System.out.println(info);

    final var keyRef = LongRef.allocate();
    final var key = Val.allocate();
    final var data = Val.allocate();

    keyRef.value(9L);
    key.set(keyRef);

    final var ptr = Memory.allocZeroed(64L);
    final var value = "hello world";
    final var valueBytes = Danger.getBytes(value);
    Danger.UNSAFE.copyMemory(valueBytes, Danger.BYTE_BASE, null, ptr, valueBytes.length);

    data.base(ptr);
    data.len(valueBytes.length);

    err = txn.put(dbi, key, data, PutFlags.APPEND);
    Assertions.assertEquals(Error.SUCCESS, err);

    key.set(keyRef);
    data.clear();

    err = txn.get(dbi, key, data);
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

    Assertions.assertEquals(value, dataStr);

    Assertions.assertEquals(Error.SUCCESS, txn.commit(true));
    System.out.println(txn.commitLatency());
    final var syncErr = env.sync();
    Assertions.assertTrue(syncErr == Error.SUCCESS || syncErr == Error.RESULT_TRUE);
    System.out.println("EnvInfo: " + env.info());
    System.out.println("EnvStat: " + env.stat());
  }

  @Test
  public void setOptions() {
    Env.delete(DB_NAME, Env.DeleteMode.JUST_DELETE);
    final var env = Env.create(DB_NAME);
    Assertions.assertEquals(Error.SUCCESS, env.setOption(Env.Option.MAX_DB, 128));
    Assertions.assertEquals(128L, env.getOption(Env.Option.MAX_DB));
    Assertions.assertEquals(Error.SUCCESS, env.setOption(Env.Option.MAX_READERS, 256));
    Assertions.assertEquals(256L, env.getOption(Env.Option.MAX_READERS));
    Assertions.assertEquals(Error.SUCCESS, env.close());
  }
}
