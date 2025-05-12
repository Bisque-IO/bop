package bop.c.mdbx

import bop.bench.Bench
import bop.c.LongRef
import bop.c.Memory
import bop.unsafe.Danger
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.Assertions
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test

object MDBXAssert {
  @JvmStatic
  fun <T> eq(expected: T, actual: T) {
    Assertions.assertEquals(expected, actual)
  }

  @JvmStatic
  fun yes(actual: Boolean) {
    Assertions.assertTrue(actual)
  }

  @JvmStatic
  fun no(actual: Boolean) {
    Assertions.assertFalse(actual)
  }

  @JvmStatic
  fun no(actual: () -> Boolean) {
    Assertions.assertFalse(actual)
  }

  @JvmStatic
  fun success(code: Int) {
    Assertions.assertEquals(Error.SUCCESS, code)
  }

  @JvmStatic
  fun successOrTrue(code: Int) {
    Assertions.assertTrue(code == Error.SUCCESS || code == Error.RESULT_TRUE)
  }
}

class MDBXTesting {
  @BeforeEach
  fun setUp() {
    Env.delete(DB_NAME, Env.DeleteMode.JUST_DELETE)
  }

  @AfterEach
  fun tearDown() {
    Env.delete(DB_NAME, Env.DeleteMode.JUST_DELETE)
  }

  @Test
  fun deleteEnv() {
    val env = Env.create(DB_NAME)
    MDBXAssert.success(env.open(DEFAULT_ENV_FLAGS, MODE))
    MDBXAssert.success(env.doClose())
    MDBXAssert.success(Env.delete(DB_NAME, Env.DeleteMode.JUST_DELETE))
  }


  @Test
  fun printJVM() {
    System.getProperties().forEach {
      println("${it.key}: ${it.value}")
    }
  }

  @Test
  fun openExisting() {
    Env.create(DB_NAME).use { env ->
      MDBXAssert.eq(
        Error.SUCCESS,
        env.geometry(
          (1024 * 512).toLong(),
          (1024 * 1024).toLong(),
          (1024 * 1024).toLong(),
          (1024 * 512).toLong(),
          0,
          4096,
        ),
      )
      MDBXAssert.success(env.option(Env.Option.MAX_READERS, 120L))
      MDBXAssert.success(env.option(Env.Option.MAX_DB, 120L))
      MDBXAssert.success(env.option(Env.Option.PREFAULT_WRITE_ENABLE, 1))
      MDBXAssert.success(env.open(DEFAULT_ENV_FLAGS, MODE))

      val txn = env.begin(null, Txn.Flags.READWRITE)
      val dbi = Table()
      dbi.init("main")
      dbi.flags = Table.Flags.CREATE or Table.Flags.INT_KEY
      MDBXAssert.success(txn.open(dbi))

      txn.put(dbi, 9L, "hello there: " + System.nanoTime())
      MDBXAssert.success(txn.commit())
      env.sync(true, false)

      env.info()

      val stat = env.stat()
      println(stat)
    }

    Env.create(DB_NAME).use { env ->
      MDBXAssert.eq(
        Error.SUCCESS,
        env.geometry(
          (1024 * 512).toLong(),
          (1024 * 1024).toLong(),
          (1024 * 1024).toLong(),
          (1024 * 512).toLong(),
          0,
          4096,
        ),
      )

      MDBXAssert.success(env.option(Env.Option.MAX_READERS, 120L))
      MDBXAssert.success(env.option(Env.Option.MAX_DB, 120L))
      MDBXAssert.success(env.option(Env.Option.PREFAULT_WRITE_ENABLE, 1))
      MDBXAssert.success(env.open(DEFAULT_ENV_FLAGS, MODE))
      val txn = env.begin(null, Txn.Flags.RDONLY)
      val mainDb = Table.create("main")
      mainDb.flags = Table.Flags.INT_KEY
      MDBXAssert.success(txn.open(mainDb))
      MDBXAssert.success(txn[mainDb, 9L])
      MDBXAssert.eq(Error.RESULT_TRUE, txn.getEqualOrGreater(mainDb, 0L))
      println(txn.key.asLong().toString() + ": " + txn.data.asString())
      MDBXAssert.success(txn.abort())
    }
  }

  @Test
  fun openEnv() {
    Env.create(DB_NAME).use { env ->
      MDBXAssert.eq(
        Error.SUCCESS,
        env.geometry(
          (1024 * 512).toLong(),
          (1024 * 1024).toLong(),
          (1024 * 1024).toLong(),
          (1024 * 512).toLong(),
          0,
          4096,
        ),
      )
      MDBXAssert.success(env.option(Env.Option.MAX_READERS, 120L))
      MDBXAssert.success(env.option(Env.Option.MAX_DB, 120L))
      MDBXAssert.success(env.option(Env.Option.PREFAULT_WRITE_ENABLE, 1))
      MDBXAssert.success(env.open(DEFAULT_ENV_FLAGS, MODE))

      val txn = env.begin(null, Txn.Flags.READWRITE)
      val dbi = Table()
      dbi.init("main")
      dbi.flags = Table.Flags.CREATE or Table.Flags.INT_KEY
      MDBXAssert.success(txn.open(dbi))

      txn.put(dbi, 9L, "hello there: " + System.nanoTime())
      MDBXAssert.success(txn.commit())
      env.sync(true, false)

      env.info()

      val stat = env.stat()
      println(stat)
    }
  }

  @Test
  fun sysRamInfo() {
    RAMInfo.create().use { info ->
      println(info)
      println("total = ${info.totalPages*info.pageSize}")
      println("avail = ${info.availPages*info.pageSize}")
    }
  }

  @Test
  fun versionInfo() {
    println(VersionInfo.get())
  }

  @Test
  @Throws(Throwable::class)
  fun beginTxn() {
    Env.create(DB_NAME).use { env ->
      MDBXAssert.eq(
        Error.SUCCESS,
        env.geometry(
          (1024 * 1024).toLong(),
          (1024 * 1024).toLong(),
          (1024 * 1024 * 64).toLong(),
          (1024 * 1024).toLong(),
          0,
          4096,
        ),
      )
      MDBXAssert.success(env.option(Env.Option.MAX_READERS, 100L))
      MDBXAssert.success(env.option(Env.Option.MAX_DB, 128L))
      MDBXAssert.success(env.open(DEFAULT_ENV_FLAGS, MODE))
      val txn = env.begin(null, Txn.Flags.READWRITE, 0L)
      MDBXAssert.yes(txn.txnPointer != 0L)

      val dbi = Table()
      dbi.init("main")
      dbi.flags = Table.Flags.CREATE or Table.Flags.INT_KEY
      var err = txn.open(dbi)
      MDBXAssert.success(err)

      err = txn.put(dbi, 7L, "hello")
      MDBXAssert.success(err)

      val stat = txn.stat()
      val info = txn.info(true)
      println(stat)
      println(info)

      val keyRef = LongRef.allocate()
      val key = Val.allocate()
      val data = Val.allocate()

      keyRef.value(9L)
      key.set(keyRef)

      val ptr = Memory.zalloc(64L)
      val value = "hello world"
      val valueBytes = Danger.getBytes(value)
      Danger.copyMemory(valueBytes, Danger.BYTE_BASE.toLong(), null, ptr, valueBytes.size.toLong())

      data.base(ptr)
      data.len(valueBytes.size.toLong())

      err = txn.put(dbi, key, data, PutFlags.APPEND)
      MDBXAssert.success(err)

      key.set(keyRef)
      data.clear()

      err = txn.get(dbi, key, data)
      MDBXAssert.success(err)

      data.clear()
      err = txn.get(dbi, 9L, data)
      MDBXAssert.success(err)

      key.base()
      key.len()
      data.base()
      data.len()
      data.asBytes()
      var dataStr = data.asString()

      MDBXAssert.eq(value, dataStr)

      val cursor = Cursor.create()
      MDBXAssert.success(cursor.bind(txn, dbi))
      MDBXAssert.success(cursor.put(3L, "bye"))
      MDBXAssert.success(cursor.put(3L, "bye"))
      MDBXAssert.success(cursor.put(1L, "bye bye"))
      MDBXAssert.success(cursor.put(2L, "hi bye"))

      MDBXAssert.success(txn.put(dbi, 10L, "hello"))
      MDBXAssert.success(txn.delete(dbi, 10L))
      MDBXAssert.success(cursor.put(10L, "hi hi", PutFlags.UPSERT))
      MDBXAssert.success(cursor.get(10L, Cursor.Op.SET_KEY))
      MDBXAssert.success(cursor.delete(PutFlags.CURRENT))
      MDBXAssert.success(cursor.get(2L, Cursor.Op.SET_KEY))

      val cycles = 5
      val iters = 1000000

      Bench.printHeader()
      Bench.threaded("txn.get", 1, cycles, iters) { threadId: Int, cycle: Int, iteration: Int ->
        MDBXAssert.success(txn.get(dbi, if (iteration % 2 == 0) 3L else 9L))
      }
      Bench.threaded("txn.put", 1, cycles, iters) { threadId: Int, cycle: Int, iteration: Int ->
        MDBXAssert.success(txn.put(dbi, if (iteration % 2 == 0) 3L else 9L, "hello"))
      }
      Bench.threaded("cursor.get FIRST/LAST", 1, cycles, iters) { threadId, cycle, iteration ->
        MDBXAssert.success(cursor.get(if (iteration % 2 == 0) Cursor.Op.FIRST else Cursor.Op.LAST))
      }
      Bench.threaded("cursor.get", 1, cycles, iters) { threadId: Int, cycle: Int, iteration: Int ->
        MDBXAssert.success(cursor.get(if (iteration % 2 == 0) 3L else 9L, Cursor.Op.SET_KEY))
      }
      Bench.threaded("cursor.put", 1, cycles, iters) { threadId: Int, cycle: Int, iteration: Int ->
        MDBXAssert.success(cursor.put(if (iteration % 2 == 0) 3L else 9L, "hello", PutFlags.UPSERT))
      }
      Bench.printFooter()

      //    Bench.printHeader();
      //    Bench.threaded("cursor.get", 1, 10, 10000000, (threadId, cycle, iteration) -> {
      //      cursor.get();
      //    });
      //    Bench.printFooter();
      err = txn[dbi, 3L]
      dataStr = txn.data.asString()
      MDBXAssert.success(err)

      //    err = cursor.get(0L, Cursor.Op.SET_LOWERBOUND);
      err = cursor.get(Cursor.Op.FIRST)
      MDBXAssert.success(err)
      cursor.key.asLong()
      cursor.data.asString()
      err = cursor.get(Cursor.Op.NEXT)
      MDBXAssert.success(err)
      cursor.key.asLong()
      cursor.data.asString()
      err = cursor.get(Cursor.Op.NEXT)
      MDBXAssert.success(err)
      cursor.key.asLong()
      cursor.data.asString()
      err = cursor.get(Cursor.Op.NEXT)
      MDBXAssert.success(err)
      cursor.key.asLong()
      cursor.data.asString()
      err = cursor.get(Cursor.Op.NEXT)
      MDBXAssert.success(err)
      err = cursor.get(Cursor.Op.NEXT)
      MDBXAssert.eq(Error.NOTFOUND, err)

      MDBXAssert.success(txn.commit(true))
      println(txn.commitLatency())
      MDBXAssert.successOrTrue(env.sync())
      println("EnvInfo: " + env.info())
      println("EnvStat: " + env.stat())

      MDBXAssert.success(env.doClose())
    }
  }

  @Test
  fun setOptions() {
    Env.create(DB_NAME).use { env ->
      MDBXAssert.success(env.option(Env.Option.MAX_DB, 128))
      MDBXAssert.eq(128L, env.option(Env.Option.MAX_DB))
      MDBXAssert.success(env.option(Env.Option.MAX_READERS, 256))
      MDBXAssert.eq(256L, env.option(Env.Option.MAX_READERS))
      MDBXAssert.success(env.doClose())
    }
  }

  companion object {
    const val DEFAULT_ENV_FLAGS: Int =
      (Env.Flags.NOMEMINIT or
        Env.Flags.MAPASYNC or
        Env.Flags.SAFE_NOSYNC or
        Env.Flags.LIFORECLAIM or
        Env.Flags.NOSTICKYTHREADS or
        Env.Flags.NOSUBDIR or
        Env.Flags.WRITEMAP)

    const val MODE: Int = 755

    const val DB_NAME: String = "mydb.db"
  }
}
