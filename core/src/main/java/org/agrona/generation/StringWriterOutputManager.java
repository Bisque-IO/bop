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
package org.agrona.generation;

import java.io.FilterWriter;
import java.io.IOException;
import java.io.StringWriter;
import java.io.Writer;
import java.util.HashMap;
import java.util.Map;

/**
 * An {@link org.agrona.generation.OutputManager} which can store source files as
 * {@link StringWriter} buy source file name.
 */
public class StringWriterOutputManager implements DynamicPackageOutputManager {
  private String packageName;
  private String initialPackageName;
  private final HashMap<String, StringWriter> sourceFileByName = new HashMap<>();

  /** Create a new instance. */
  public StringWriterOutputManager() {}

  /** {@inheritDoc} */
  public Writer createOutput(final String name) {
    final StringWriter stringWriter = new StringWriter();
    sourceFileByName.put(packageName + "." + name, stringWriter);

    return new FilterWriter(stringWriter) {
      public void close() throws IOException {
        super.close();

        if (null != initialPackageName) {
          packageName = initialPackageName;
        }
      }
    };
  }

  /**
   * Set the package name to be used for source files.
   *
   * @param packageName to be used for source files.
   */
  public void setPackageName(final String packageName) {
    this.packageName = packageName;
    if (null == initialPackageName) {
      initialPackageName = packageName;
    }
  }

  /**
   * Get a {@link CharSequence} which represents the source file.
   *
   * @param name of the source file.
   * @return {@link CharSequence} which represents the source file.
   */
  public CharSequence getSource(final String name) {
    final StringWriter stringWriter = sourceFileByName.get(name);
    if (null == stringWriter) {
      throw new IllegalArgumentException("unknown source file name: " + name);
    }

    return stringWriter.toString();
  }

  /**
   * Get a {@link Map} of all source files.
   *
   * @return a {@link Map} of all source files.
   */
  public Map<String, CharSequence> getSources() {
    final HashMap<String, CharSequence> sources = new HashMap<>();
    for (final Map.Entry<String, StringWriter> entry : sourceFileByName.entrySet()) {
      sources.put(entry.getKey(), entry.getValue().toString());
    }

    return sources;
  }

  /** Clear all source files in this {@link OutputManager} and reset the initial package name. */
  public void clear() {
    initialPackageName = null;
    packageName = "";
    sourceFileByName.clear();
  }
}
