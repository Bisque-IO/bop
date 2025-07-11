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
package org.agrona.concurrent;

/**
 * Low-latency idle strategy to be employed in loops that do significant work on each iteration such
 * that any work in the idle strategy would be wasteful.
 *
 * <p>The {@link NoOpIdleStrategy} should be used with care:
 *
 * <ol>
 *   <li>It could increase power consumption.
 *   <li>The increased power consumption could lead to thermal throttling causing an overall
 *       performance drop.
 *   <li>It could consume resources that otherwise would be used by the hyper-sibling.
 *   <li>It could lead to a memory order violation at the end of the loop causing a pipeline reset.
 * </ol>
 *
 * The {@link BusySpinIdleStrategy} might be a better alternative in some scenarios. That being
 * said, using {@link NoOpIdleStrategy} is perfectly fine if the underlying duty cycle loops is
 * performing syscalls (e.g. in sender and receiver threads in Aeron).
 */
public final class NoOpIdleStrategy implements IdleStrategy {
  /** Name to be returned from {@link #alias()}. */
  public static final String ALIAS = "noop";

  /** As there is no instance state then this object can be used to save on allocation. */
  public static final NoOpIdleStrategy INSTANCE = new NoOpIdleStrategy();

  /** Create a new instance. */
  public NoOpIdleStrategy() {}

  /**
   * <b>Note</b>: this implementation will result in no safepoint poll once inlined.
   *
   * <p>{@inheritDoc}
   */
  public void idle(final int workCount) {}

  /**
   * <b>Note</b>: this implementation will result in no safepoint poll once inlined.
   *
   * <p>{@inheritDoc}
   */
  public void idle() {}

  /** {@inheritDoc} */
  public void reset() {}

  /** {@inheritDoc} */
  public String alias() {
    return ALIAS;
  }

  /** {@inheritDoc} */
  public String toString() {
    return "NoOpIdleStrategy{alias=" + ALIAS + "}";
  }
}
