package bop.asm;

import java.io.File;
import java.io.IOException;
import java.nio.file.FileSystems;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ForkJoinPool;
import java.util.jar.JarFile;
import org.agrona.UnsafeApi;

public class Compiler {
  private ArrayList<Directory> directories = new ArrayList<>();
  private ConcurrentHashMap<String, ClassFile> classes = new ConcurrentHashMap<>();
  private ForkJoinPool taskPool = ForkJoinPool.commonPool();

  public static void main(String[] args) throws Throwable {
    var fs = FileSystems.getDefault();
    var watcher = fs.newWatchService();
    for (var s : fs.getRootDirectories()) {
      System.out.println(s);
    }

    var bytes = new byte[257];
    bytes[0] = 34;
    var hashCode = System.identityHashCode(bytes);
    var header0 = UnsafeApi.getInt(bytes, 0);
    var header1 = UnsafeApi.getInt(bytes, 4);
    var header2 = UnsafeApi.getInt(bytes, 8);
    var header3 = UnsafeApi.getInt(bytes, 12);

    UnsafeApi.putIntUnaligned(bytes, 12, 16);
    UnsafeApi.putIntVolatile(bytes, 4, 2);

    for (int i = 0; i < 10000000; i++) {
      bytes = new byte[97];
      UnsafeApi.putIntVolatile(bytes, 0, 6);
      UnsafeApi.putIntUnaligned(bytes, 12, 16);
    }

    System.out.println(new File("./").toPath().toAbsolutePath());
    System.out.println(bytes.length);
    System.out.println(bytes.length);
    //    System.out.println(Factory.BYTE_ARRAY2.layout.instanceSize());
  }

  public class Directory {
    private Path path;
    private ArrayList<ClassFile> classFiles;
    HashMap<String, ClassFile> object2ObjectOpenHashMap;
  }

  public static class ClassFile {
    String name;
    public byte[] before;
    public byte[] after;
  }

  protected void readJar(Path path) throws IOException {
    Files.readAllBytes(path);
    var jarFile = new JarFile(path.toFile());
    jarFile.stream().forEach(entry -> {});
  }

  public static class Jar {
    HashMap<String, ClassFile> files;
  }
}
