package bop.c;

import static java.lang.System.getProperty;
import static java.lang.Thread.currentThread;
import static java.util.Locale.ENGLISH;
import static java.util.Objects.requireNonNull;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.lang.foreign.Arena;
import java.lang.foreign.Linker;
import java.lang.foreign.SymbolLookup;
import java.nio.file.Files;

public class Loader {
  public static final Linker LINKER = Linker.nativeLinker();
  public static final SymbolLookup LOOKUP;
  public static final Arena ARENA = Arena.global();
  public static final String LIB_NAME;

  static {
    LIB_NAME = extract(TargetName.RESOLVED_FILENAME);
    //    System.out.println("LIB NAME: " + LIB_NAME);
    LOOKUP = SymbolLookup.libraryLookup(new File(LIB_NAME).getAbsolutePath(), ARENA);
  }

  public static void load() {}

  private static String extract(final String name) {
    final var lastSlash = name.lastIndexOf('/');
    final var fileName = lastSlash > -1 ? name.substring(lastSlash + 1) : name;
    final var fileNameWithoutExtension = fileName.substring(0, fileName.lastIndexOf('.'));
    final String suffix = name.substring(fileName.lastIndexOf('.'));
    final File file;
    try {
      final File dir = new File("./");
      if (!dir.exists() || !dir.isDirectory()) {
        throw new IllegalStateException("Invalid extraction directory " + dir);
      }
      file = new File(dir, fileName);
      //      file = createTempFile(fileNameWithoutExtension, suffix, dir);
      file.deleteOnExit();
      final ClassLoader cl = currentThread().getContextClassLoader();
      try (InputStream in = cl.getResourceAsStream(name);
          OutputStream out = Files.newOutputStream(file.toPath())) {
        requireNonNull(in, "Classpath resource not found");
        int bytes;
        final byte[] buffer = new byte[8192];
        while (-1 != (bytes = in.read(buffer))) {
          out.write(buffer, 0, bytes);
        }
      }
      return fileName;
      //      return file.getAbsolutePath();
    } catch (final IOException e) {
      throw new RuntimeException("Failed to extract " + name, e);
    }
  }

  /// Determines the name of the target libbop native library.
  public static final class TargetName {

    /// Resolved target native filename or fully-qualified classpath location.
    public static final String RESOLVED_FILENAME;

    private static final String ARCH = getProperty("os.arch");
    private static final String OS = getProperty("os.name");

    static {
      RESOLVED_FILENAME = resolveFilename(ARCH, OS);
    }

    private TargetName() {}

    public static String resolveExtension(final String os) {
      if (check(os, "Windows", "windows", "win")) {
        return "dll";
      }
      if (check(os, "mac os x", "mac os", "ios", "macos", "macosx", "mac")) {
        return "dylib";
      }
      return "so";
    }

    static String resolveFilename(final String arch, final String os) {
      final String pkg = TargetName.class.getPackage().getName().replace('.', '/');
      final var resolvedArch = resolveArch(arch);
      final var resolvedOs = resolveOs(os);
      final var resolvedExtension = resolveExtension(os);
      if (resolvedOs.equals("windows")) {
        return pkg + "/bop-windows-" + resolvedArch + "." + resolvedExtension;
      } else {
        return pkg + "/libbop-" + resolvedOs + "-" + resolvedArch + "." + resolvedExtension;
      }
    }

    /// Case insensitively checks whether the passed string starts with any of the candidate
    // strings.
    ///
    /// @param string the string being checked
    /// @param candidates one or more candidate strings
    /// @return true if the string starts with any of the candidates
    private static boolean check(final String string, final String... candidates) {
      if (string == null) {
        return false;
      }

      final String strLower = string.toLowerCase(ENGLISH);
      for (final String c : candidates) {
        if (strLower.startsWith(c.toLowerCase(ENGLISH))) {
          return true;
        }
      }
      return false;
    }

    private static String resolveArch(final String arch) {
      if (check(arch, "aarch64", "arm64")) {
        return "arm64";
      } else if (check(arch, "x86_64", "amd64", "x64")) {
        return "x86_64";
      } else if (check(arch, "riscv64", "riscv")) {
        return "riscv64";
      }
      throw new UnsupportedOperationException("Unsupported os.arch: " + arch);
    }

    private static String resolveOs(final String os) {
      if (check(os, "linux")) {
        return "linux";
      } else if (check(
          os, "macos", "macosx", "mac osx", "darwin")) {
        return "macos";
      } else if (check(os, "win", "windows", "windows nt")) {
        return "windows";
      }
      throw new UnsupportedOperationException("Unsupported os.name: " + os);
    }

    private static String resolveToolchain(final String os) {
      return check(os, "Mac OS") ? "none" : "gnu";
    }
  }
}
