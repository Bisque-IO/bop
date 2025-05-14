package bop.cluster.impl;

import bop.cluster.message.Event;
import java.io.IOException;
import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.nio.channels.FileChannel;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.Objects;
import java.util.concurrent.atomic.AtomicLong;

public class SegmentFile implements AutoCloseable {
  final FileChannel channel;
  final Arena arena;
  final MemorySegment segment;
  private final AtomicLong tail = new AtomicLong(0L);
  private final AtomicLong appendix = new AtomicLong(0L);
  private final long capacity;

  public SegmentFile(FileChannel channel, Arena arena, MemorySegment segment) {
    this.channel = channel;
    this.arena = arena;
    this.segment = segment;
    this.capacity = segment.byteSize();
  }

  public static SegmentFile open(Path path, long initialSize) throws IOException {
    Objects.requireNonNull(path);
    FileChannel channel = null;
    var created = false;
    long mapSize = Math.max(initialSize, 1024 * 1024 * 128);

    try {
      channel = FileChannel.open(
          path, StandardOpenOption.CREATE_NEW, StandardOpenOption.READ, StandardOpenOption.WRITE);
      created = true;
    } catch (IOException e) {
      try {
        channel = FileChannel.open(
            path, StandardOpenOption.CREATE, StandardOpenOption.READ, StandardOpenOption.WRITE);
        initialSize = channel.size();
      } catch (IOException e2) {
        throw e2;
      }
    }

    final Arena arena;
    try {
      arena = Arena.ofShared();
      Objects.requireNonNull(arena);
    } catch (Throwable e) {
      throw new RuntimeException(e);
    }

    final MemorySegment segment;
    final long size;
    if (created) {
      try {
        size = initialSize;
        channel.truncate(size);
        segment = channel.map(FileChannel.MapMode.READ_WRITE, 0, mapSize, arena);
      } catch (IOException e) {
        try {
          arena.close();
        } finally {
          channel.close();
        }
        throw e;
      }
    } else {
      try {
        size = channel.size();
        segment = channel.map(FileChannel.MapMode.READ_WRITE, 0, mapSize, arena);
      } catch (IOException e) {
        try {
          arena.close();
        } finally {
          channel.close();
        }
        throw e;
      }
    }

    final var file = new SegmentFile(channel, arena, segment);
    try {
      file.init(created);
    } catch (Throwable e) {
      try {
        file.close();
      } catch (Exception ex) {
        // Ignore.
      }
      throw new RuntimeException(e);
    }
    return file;
  }

  private void init(boolean created) {
    if (created) {}

    // Read file
    // File Header
    // i64   magic
    // i64   id
    // i32   records
    // i32   smallest
    // i32   largest
    // i32   appendixSize
    // i64   appendixOffset
    // i64   ackOffset
    // i32   ackSize
    // i32   reliableCount
    // i32   nacks
    // i32   acks
  }

  private void extend() {}

  public void append(Event msg) throws IOException {}

  public void append(long address, int length) throws IOException {}

  @Override
  public void close() throws Exception {
    try {
      channel.close();
    } finally {
      arena.close();
    }
  }

  interface Locator {
    long offsetOf(int sequence);
  }

  static class BackedLocator implements Locator {
    private final long address;
    private final long end;

    public BackedLocator(long address, long end) {
      this.address = address;
      this.end = end;
    }

    @Override
    public long offsetOf(int sequence) {
      final long address = this.address;
      final long end = this.end;
      final long offset = address + (sequence * 4L);
      if (offset + 4 > end) {
        return -1L;
      }
      return offset;
    }
  }
}
