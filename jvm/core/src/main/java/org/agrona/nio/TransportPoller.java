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
package org.agrona.nio;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.channels.Selector;
import org.agrona.LangUtil;

/** Implements the common functionality for a transport poller. */
public class TransportPoller implements AutoCloseable {
  /**
   * System property name for the threshold beyond which individual channel/socket polling will swap
   * to using a {@link Selector}.
   */
  public static final String ITERATION_THRESHOLD_PROP_NAME =
      "org.agrona.transport.iteration.threshold";

  /**
   * Default threshold beyond which individual channel/socket polling will swap to using a
   * {@link Selector}.
   */
  public static final int ITERATION_THRESHOLD_DEFAULT = 5;

  /**
   * Threshold beyond which individual channel/socket polling will swap to using a {@link Selector}.
   *
   * @see #ITERATION_THRESHOLD_PROP_NAME
   * @see #ITERATION_THRESHOLD_DEFAULT
   */
  public static final int ITERATION_THRESHOLD =
      Integer.getInteger(ITERATION_THRESHOLD_PROP_NAME, ITERATION_THRESHOLD_DEFAULT);

  /** Reference to the {@link Selector} for the transport. */
  protected final Selector selector;

  /** Default constructor. */
  public TransportPoller() {
    try {
      selector = Selector.open(); // yes, SelectorProvider, blah, blah
    } catch (final IOException ex) {
      throw new UncheckedIOException(ex);
    }
  }

  /** Close NioSelector down. Returns immediately. */
  public void close() {
    selector.wakeup();
    try {
      selector.close();
    } catch (final IOException ex) {
      LangUtil.rethrowUnchecked(ex);
    }
  }

  /**
   * Explicit call to {@link Selector#selectNow()} followed by clearing out the set without
   * processing.
   */
  public void selectNowWithoutProcessing() {
    try {
      selector.selectNow();
    } catch (final IOException ex) {
      LangUtil.rethrowUnchecked(ex);
    }
  }
}
