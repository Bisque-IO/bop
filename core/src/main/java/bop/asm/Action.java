package bop.asm;

import java.util.HashMap;

public class Action {
  public void run() {
    final var map = new HashMap<String, String>();
    final var obj = new Object();
    var str = new String("Hello World");
    map.put("a", "b");
    map.put("b", str);
    print(map);
  }

  public static void print(Object o) {
    System.out.println(o);
  }
}
