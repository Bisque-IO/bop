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
 * This is an (Object, int) primitive specialisation of a BiPredicate.
 *
 * @param <T> the type of the input to the predicate.
 */
@FunctionalInterface
public interface ObjIntPredicate<T> {
  /**
   * Evaluates this predicate on the given arguments.
   *
   * @param valueOne for the tuple.
   * @param valueTwo for the tuple.
   * @return {@code true} if the input arguments match the predicate, otherwise {@code false}.
   */
  boolean test(T valueOne, int valueTwo);
}
