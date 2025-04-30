package bop.cluster;

import io.aeron.archive.ArchiveToolX;
import io.aeron.archive.RecordingDescriptor;
import java.io.File;
import java.io.IOException;
import java.nio.MappedByteBuffer;
import java.nio.channels.FileChannel;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.ArrayList;
import java.util.Objects;
import lombok.extern.slf4j.Slf4j;
import org.agrona.collections.Long2ObjectHashMap;
import org.agrona.concurrent.UnsafeBuffer;

@Slf4j
public class Archives {
  private final File dir;
  private final Long2ObjectHashMap<Recording> recordings = new Long2ObjectHashMap<>();
  private final Long2ObjectHashMap<RecordingDescriptor> descriptors = new Long2ObjectHashMap<>();

  private Archives(File dir) {
    this.dir = dir;
  }

  public Recording get(long recordingId) {
    return recordings.get(recordingId);
  }

  private void init() {
    final var files = dir.listFiles();
    if (files == null || files.length == 0) {
      return;
    }

    log.info("loading archives from {} found {} files", dir, files.length);

    ArchiveToolX.loadRecordingDescriptors(dir, descriptors);

    for (var file : files) {
      var name = file.getName();
      if (!name.endsWith(".rec")) {
        continue;
      }
      name = name.substring(0, name.length() - 4);
      var index = name.lastIndexOf('-');
      if (index < 0) {
        continue;
      }
      final var recordingId = Long.parseLong(name.substring(0, index));
      final var startPosition = Long.parseLong(name.substring(index + 1));

      var recording = recordings.get(recordingId);
      if (recording == null) {
        recording = new Recording(this, recordingId, descriptors.get(recordingId));
        recordings.put(recordingId, recording);
      }

      recording.files.add(new Segment(
          recording,
          file.getAbsolutePath(),
          recordingId,
          startPosition,
          startPosition + file.length()));
    }

    for (var store : recordings.values()) {
      store.files.sort((a, b) -> {
        if (a.startPosition < b.startPosition) {
          return -1;
        }
        if (a.startPosition > b.startPosition) {
          return 1;
        }
        return 0;
      });
    }

    for (var store : recordings.values()) {
      final var descriptor = descriptors.get(store.recordingId);
      if (descriptor == null) {}
    }
  }

  public static Archives create(File dir) {
    Objects.requireNonNull(dir);
    if (!dir.exists()) {
      dir.mkdirs();
    }
    if (!dir.isDirectory()) {
      throw new IllegalArgumentException(dir + " is not a directory");
    }
    final var store = new Archives(dir);
    store.init();
    return store;
  }

  public static class Recording {
    private final Archives store;
    private final long recordingId;
    private final RecordingDescriptor descriptor;
    private long segmentSize;
    private long termSize;
    private final ArrayList<Segment> files = new ArrayList<>();
    private final Long2ObjectHashMap<Segment> filesByStartPosition = new Long2ObjectHashMap<>();

    public Recording(Archives store, long recordingId, RecordingDescriptor descriptor) {
      this.store = store;
      this.recordingId = recordingId;
      this.descriptor = descriptor;
      if (descriptor != null) {
        this.segmentSize = descriptor.segmentFileLength();
        this.termSize = descriptor.termBufferLength();
      } else {

      }
    }

    public void reload() {}
  }

  public static class Segment {
    private final Recording recording;
    private final Path path;
    private final long recordingId;
    private final long startPosition;
    private final long endPosition;
    private long refCount = 0L;
    private UnsafeBuffer buffer;
    private FileChannel channel;
    private MappedByteBuffer mmap;

    Segment(
        Recording recording, String path, long recordingId, long startPosition, long endPosition) {
      this.recording = recording;
      this.path = Path.of(path);
      this.recordingId = recordingId;
      this.startPosition = startPosition;
      this.endPosition = endPosition;
    }

    public UnsafeBuffer buffer() throws IOException {
      final var buffer = this.buffer;
      if (buffer != null) {
        return buffer;
      }
      try {
        this.channel = FileChannel.open(path, StandardOpenOption.READ);
        this.mmap = this.channel.map(FileChannel.MapMode.READ_ONLY, 0, channel.size());
        this.buffer = new UnsafeBuffer(mmap);
      } catch (IOException e) {
        if (this.channel != null) {
          try {
            this.channel.close();
          } catch (Throwable e2) {
            // Ignore
          }
        }
        this.mmap = null;
        throw e;
      }
      return this.buffer;
    }

    void incrementRefCount() {
      this.refCount++;
    }

    void decrementRefCount() {
      if (--this.refCount <= 0) {
        this.refCount = 0;
        close();
      }
    }

    void compress() {}

    public void close() {
      final var channel = this.channel;
      if (channel == null) {
        return;
      }
      try {
        channel.close();
      } catch (Throwable e) {
      } finally {
        this.channel = null;
        this.mmap = null;
        this.buffer = null;
      }
    }
  }
}
