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
package org.agrona.collections;

/**
 * This is an (int, Object) primitive specialisation of a BiConsumer.
 *
 * @param <T> type of the value.
 */
@FunctionalInterface
public interface IntObjConsumer<T> {
  /**
   * Performs this operation on the given arguments.
   *
   * @param i for the tuple.
   * @param v for the tuple.
   */
  void accept(int i, T v);
}
