/*
 * Copyright 2014-2025 Real Logic Limited.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * https://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
package org.agrona;

import static java.nio.channels.FileChannel.MapMode.READ_ONLY;
import static java.nio.channels.FileChannel.MapMode.READ_WRITE;
import static java.nio.file.StandardOpenOption.*;

import java.io.File;
import java.io.IOException;
import java.io.RandomAccessFile;
import java.nio.ByteBuffer;
import java.nio.MappedByteBuffer;
import java.nio.channels.FileChannel;
import java.nio.file.Files;
import java.util.Arrays;
import java.util.function.BiConsumer;

/** Collection of IO utilities for dealing with files, especially mapping and un-mapping. */
@SuppressWarnings({"deprecation", "removal"})
public final class IoUtil {
  /** Size in bytes of a file page. */
  public static final int BLOCK_SIZE = 4 * 1024;

  private static final byte[] FILLER = new byte[BLOCK_SIZE];
  private static final int MAP_READ_ONLY = 0;
  private static final int MAP_READ_WRITE = 1;
  private static final int MAP_PRIVATE = 2;

  private IoUtil() {}

  /**
   * Fill region of a file with a given byte value.
   *
   * @param fileChannel to fill.
   * @param position at which to start writing.
   * @param length of the region to write.
   * @param value to fill the region with.
   */
  public static void fill(
      final FileChannel fileChannel, final long position, final long length, final byte value) {
    try {
      final byte[] filler;
      if (0 != value) {
        filler = new byte[BLOCK_SIZE];
        Arrays.fill(filler, value);
      } else {
        filler = FILLER;
      }

      final ByteBuffer byteBuffer = ByteBuffer.wrap(filler);
      fileChannel.position(position);

      final int blocks = (int) (length / BLOCK_SIZE);
      final int blockRemainder = (int) (length % BLOCK_SIZE);

      for (int i = 0; i < blocks; i++) {
        byteBuffer.position(0);
        fileChannel.write(byteBuffer);
      }

      if (blockRemainder > 0) {
        byteBuffer.position(0);
        byteBuffer.limit(blockRemainder);
        fileChannel.write(byteBuffer);
      }
    } catch (final IOException ex) {
      LangUtil.rethrowUnchecked(ex);
    }
  }

  /**
   * Recursively delete a file or directory tree.
   *
   * @param file to be deleted.
   * @param ignoreFailures don't throw an exception if delete fails.
   */
  public static void delete(final File file, final boolean ignoreFailures) {
    if (file.exists()) {
      if (file.isDirectory()) {
        final File[] files = file.listFiles();
        if (null != files) {
          for (final File f : files) {
            delete(f, ignoreFailures);
          }
        }
      }

      if (!file.delete() && !ignoreFailures) {
        try {
          Files.delete(file.toPath());
        } catch (final IOException ex) {
          LangUtil.rethrowUnchecked(ex);
        }
      }
    }
  }

  /**
   * Recursively delete a file or directory tree.
   *
   * @param file to be deleted.
   * @param errorHandler to delegate errors to on exception.
   */
  public static void delete(final File file, final ErrorHandler errorHandler) {
    try {
      if (file.exists()) {
        if (file.isDirectory()) {
          final File[] files = file.listFiles();
          if (null != files) {
            for (final File f : files) {
              delete(f, errorHandler);
            }
          }
        }

        if (!file.delete()) {
          Files.delete(file.toPath());
        }
      }
    } catch (final Exception ex) {
      errorHandler.onError(ex);
    }
  }

  /**
   * Create a directory if it doesn't already exist.
   *
   * @param directory the directory which definitely exists after this method call.
   * @param descriptionLabel to associate with the directory for any exceptions.
   */
  public static void ensureDirectoryExists(final File directory, final String descriptionLabel) {
    if (!directory.exists()) {
      if (!directory.mkdirs()) {
        throw new IllegalArgumentException(
            "could not create " + descriptionLabel + " directory: " + directory);
      }
    }
  }

  /**
   * Create a directory, removing previous directory if it already exists.
   *
   * <p>Call callback if it does exist.
   *
   * @param directory the directory which definitely exists after this method call.
   * @param descriptionLabel to associate with the directory for any exceptions and callback.
   * @param callback to call if directory exists passing back absolute path and descriptionLabel.
   */
  public static void ensureDirectoryIsRecreated(
      final File directory,
      final String descriptionLabel,
      final BiConsumer<String, String> callback) {
    if (directory.exists()) {
      delete(directory, false);
      callback.accept(directory.getAbsolutePath(), descriptionLabel);
    }

    if (!directory.mkdirs()) {
      throw new IllegalArgumentException(
          "could not create " + descriptionLabel + " directory: " + directory);
    }
  }

  /**
   * Delete file only if it already exists.
   *
   * @param file to delete.
   */
  public static void deleteIfExists(final File file) {
    try {
      Files.deleteIfExists(file.toPath());
    } catch (final IOException ex) {
      LangUtil.rethrowUnchecked(ex);
    }
  }

  /**
   * Delete file only if it already exists.
   *
   * @param file to delete.
   * @param errorHandler to delegate error to on exception.
   */
  public static void deleteIfExists(final File file, final ErrorHandler errorHandler) {
    try {
      Files.deleteIfExists(file.toPath());
    } catch (final Exception ex) {
      errorHandler.onError(ex);
    }
  }

  /**
   * Create an empty file, fill with 0s, and return the {@link FileChannel}.
   *
   * @param file to create.
   * @param length of the file to create.
   * @return {@link FileChannel} for the file.
   */
  public static FileChannel createEmptyFile(final File file, final long length) {
    return createEmptyFile(file, length, true);
  }

  /**
   * Create an empty file, and optionally fill with 0s, and return the {@link FileChannel}.
   *
   * @param file to create.
   * @param length of the file to create.
   * @param fillWithZeros to the length of the file to force allocation.
   * @return {@link FileChannel} for the file.
   */
  public static FileChannel createEmptyFile(
      final File file, final long length, final boolean fillWithZeros) {
    ensureDirectoryExists(file.getParentFile(), file.getParent());

    FileChannel templateFile = null;
    try {
      final RandomAccessFile randomAccessFile = new RandomAccessFile(file, "rw");
      randomAccessFile.setLength(length);
      templateFile = randomAccessFile.getChannel();

      if (fillWithZeros) {
        fill(templateFile, 0, length, (byte) 0);
      }
    } catch (final IOException ex) {
      LangUtil.rethrowUnchecked(ex);
    }

    return templateFile;
  }

  /**
   * Check that file exists, open file, and return MappedByteBuffer for entire file as
   * {@link FileChannel.MapMode#READ_WRITE}.
   *
   * <p>The file itself will be closed, but the mapping will persist.
   *
   * @param location of the file to map.
   * @param descriptionLabel to be associated for any exceptions.
   * @return {@link MappedByteBuffer} for the file.
   */
  public static MappedByteBuffer mapExistingFile(
      final File location, final String descriptionLabel) {
    return mapExistingFile(location, READ_WRITE, descriptionLabel);
  }

  /**
   * Check that file exists, open file, and return MappedByteBuffer for only region specified as
   * {@link FileChannel.MapMode#READ_WRITE}.
   *
   * <p>The file itself will be closed, but the mapping will persist.
   *
   * @param location of the file to map.
   * @param descriptionLabel to be associated for an exceptions.
   * @param offset offset to start mapping at.
   * @param length length to map region.
   * @return {@link MappedByteBuffer} for the file.
   */
  public static MappedByteBuffer mapExistingFile(
      final File location, final String descriptionLabel, final long offset, final long length) {
    return mapExistingFile(location, READ_WRITE, descriptionLabel, offset, length);
  }

  /**
   * Check that file exists, open file, and return MappedByteBuffer for entire file for a given
   * {@link FileChannel.MapMode}.
   *
   * <p>The file itself will be closed, but the mapping will persist.
   *
   * @param location of the file to map.
   * @param mapMode for the mapping.
   * @param descriptionLabel to be associated for any exceptions.
   * @return {@link MappedByteBuffer} for the file.
   */
  public static MappedByteBuffer mapExistingFile(
      final File location, final FileChannel.MapMode mapMode, final String descriptionLabel) {
    checkFileExists(location, descriptionLabel);

    MappedByteBuffer mappedByteBuffer = null;
    try (RandomAccessFile file = new RandomAccessFile(location, getFileMode(mapMode));
        FileChannel channel = file.getChannel()) {
      mappedByteBuffer = channel.map(mapMode, 0, channel.size());
    } catch (final IOException ex) {
      LangUtil.rethrowUnchecked(ex);
    }

    return mappedByteBuffer;
  }

  /**
   * Check that file exists, open file, and return MappedByteBuffer for only region specified for a
   * given {@link FileChannel.MapMode}.
   *
   * <p>The file itself will be closed, but the mapping will persist.
   *
   * @param location of the file to map.
   * @param mapMode for the mapping.
   * @param descriptionLabel to be associated for an exceptions.
   * @param offset offset to start mapping at.
   * @param length length to map region.
   * @return {@link MappedByteBuffer} for the file.
   */
  public static MappedByteBuffer mapExistingFile(
      final File location,
      final FileChannel.MapMode mapMode,
      final String descriptionLabel,
      final long offset,
      final long length) {
    checkFileExists(location, descriptionLabel);

    MappedByteBuffer mappedByteBuffer = null;
    try (RandomAccessFile file = new RandomAccessFile(location, getFileMode(mapMode));
        FileChannel channel = file.getChannel()) {
      mappedByteBuffer = channel.map(mapMode, offset, length);
    } catch (final IOException ex) {
      LangUtil.rethrowUnchecked(ex);
    }

    return mappedByteBuffer;
  }

  /**
   * Create a new file, fill with 0s, and return a {@link MappedByteBuffer} for the file.
   *
   * <p>The file itself will be closed, but the mapping will persist.
   *
   * @param location of the file to create and map.
   * @param length of the file to create and map.
   * @return {@link MappedByteBuffer} for the file.
   */
  public static MappedByteBuffer mapNewFile(final File location, final long length) {
    return mapNewFile(location, length, true);
  }

  /**
   * Create a new file, and optionally fill with 0s, and return a {@link MappedByteBuffer} for the
   * file.
   *
   * <p>The file itself will be closed, but the mapping will persist.
   *
   * @param location of the file to create and map.
   * @param length of the file to create and map.
   * @param fillWithZeros to force allocation.
   * @return {@link MappedByteBuffer} for the file.
   */
  public static MappedByteBuffer mapNewFile(
      final File location, final long length, final boolean fillWithZeros) {
    MappedByteBuffer mappedByteBuffer = null;
    try (FileChannel channel = FileChannel.open(location.toPath(), CREATE_NEW, READ, WRITE)) {
      mappedByteBuffer = channel.map(READ_WRITE, 0, length);
    } catch (final IOException ex) {
      LangUtil.rethrowUnchecked(ex);
    }

    if (fillWithZeros) {
      int pos = 0;
      final int capacity = mappedByteBuffer.capacity();
      while (pos < capacity) {
        mappedByteBuffer.put(pos, (byte) 0);
        pos += BLOCK_SIZE;
      }
    }

    return mappedByteBuffer;
  }

  /**
   * Check that a file exists and throw an exception if not.
   *
   * @param file to check existence of.
   * @param name to associate for the exception.
   */
  public static void checkFileExists(final File file, final String name) {
    if (!file.exists()) {
      final String msg = "missing file for " + name + " : " + file.getAbsolutePath();
      throw new IllegalStateException(msg);
    }
  }

  /**
   * Unmap a {@link MappedByteBuffer} without waiting for the next GC cycle.
   *
   * @param buffer to be unmapped.
   * @see BufferUtil#free(ByteBuffer)
   */
  public static void unmap(final MappedByteBuffer buffer) {
    BufferUtil.free(buffer);
  }

  /**
   * Unmap a {@link ByteBuffer} without waiting for the next GC cycle if its memory mapped.
   *
   * @param buffer to be unmapped.
   */
  public static void unmap(final ByteBuffer buffer) {
    if (buffer instanceof MappedByteBuffer) {
      unmap((MappedByteBuffer) buffer);
    }
  }

  /**
   * Return the system property for java.io.tmpdir ensuring a {@link File#separator} is at the end.
   *
   * @return tmp directory for the runtime.
   */
  public static String tmpDirName() {
    String tmpDirName = System.getProperty("java.io.tmpdir");
    if (!tmpDirName.endsWith(File.separator)) {
      tmpDirName += File.separator;
    }

    return tmpDirName;
  }

  /**
   * Remove trailing slash characters from a builder leaving the remaining characters.
   *
   * @param builder to remove trailing slash characters from.
   */
  public static void removeTrailingSlashes(final StringBuilder builder) {
    while (builder.length() > 1) {
      final int lastCharIndex = builder.length() - 1;
      final char c = builder.charAt(lastCharIndex);
      if ('/' == c || '\\' == c) {
        builder.setLength(lastCharIndex);
      } else {
        break;
      }
    }
  }

  private static String getFileMode(final FileChannel.MapMode mode) {
    return mode == READ_ONLY ? "r" : "rw";
  }
}
