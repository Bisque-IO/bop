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
package org.agrona.concurrent.status;

/** Indicates how far through an abstract task a component has progressed as a counter value. */
public abstract class ReadablePosition implements AutoCloseable {
  /** Default constructor. */
  public ReadablePosition() {}

  /**
   * Identifier for this position.
   *
   * @return the identifier for this position.
   */
  public abstract int id();

  /**
   * Get the current position of a component with plain memory semantics.
   *
   * @return the current position of a component
   */
  public abstract long get();

  /**
   * Get the current position of a component with volatile memory semantics.
   *
   * @return the current position of a component.
   */
  public abstract long getVolatile();

  /**
   * Get the current position of a component with acquire memory semantics.
   *
   * @return the current position of a component.
   * @since 2.1.0
   */
  public abstract long getAcquire();

  /**
   * Get the current position of a component with opaque memory semantics.
   *
   * @return the current position of a component
   * @since 2.1.0
   */
  public abstract long getOpaque();

  /** {@inheritDoc} */
  public abstract void close();
}
