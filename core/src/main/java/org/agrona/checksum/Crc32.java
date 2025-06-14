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
package org.agrona.checksum;

import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.lang.reflect.Method;
import java.util.zip.CRC32;
import org.agrona.LangUtil;

/**
 * Implementation of the {@link org.agrona.checksum.Checksum} interface that computes CRC-32
 * checksum.
 */
public final class Crc32 implements Checksum {
  /** Singleton instance to compute CRC-32 checksum. */
  public static final Crc32 INSTANCE = new Crc32();

  private static final MethodHandle UPDATE_BYTE_BUFFER;

  static {
    try {
      final Method method = CRC32.class.getDeclaredMethod(
          "updateByteBuffer0", int.class, long.class, int.class, int.class);
      method.setAccessible(true);
      MethodHandle methodHandle = MethodHandles.lookup().unreflect(method);
      methodHandle = MethodHandles.insertArguments(methodHandle, 0, 0);
      UPDATE_BYTE_BUFFER = methodHandle;
    } catch (final Exception ex) {
      throw new Error(ex);
    }
  }

  private Crc32() {}

  /** {@inheritDoc} */
  public int compute(final long address, final int offset, final int length) {
    try {
      return (int) UPDATE_BYTE_BUFFER.invokeExact(address, offset, length);
    } catch (final Throwable t) {
      LangUtil.rethrowUnchecked(t);
      return -1;
    }
  }
}
