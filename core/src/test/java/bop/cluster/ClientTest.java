package bop.cluster;

import bop.cluster.message.Event;
import bop.cluster.message.EventImpl;
import java.net.Inet4Address;
import java.util.concurrent.atomic.AtomicLong;
import org.junit.jupiter.api.Test;

public class ClientTest {
  static void printSysProps() {
    System.getProperties().forEach((k, v) -> System.out.println(k + "=" + v));
  }

  public static void main(String[] args) throws Throwable {
    //    final var HOST = "172.30.192.1";
    final var HOST = "127.0.0.1";
    //    final var HOST = "192.168.8.131";
    System.setProperty("java.net.preferIPv4Stack", "true");
    System.out.println("IP Address: " + Inet4Address.getLoopbackAddress().getHostAddress());
    printSysProps();
    //    System.out.println(System.getProperty("vendor"))

    final int THREADS = 1;
    final AtomicLong counter = new AtomicLong();
    final Client[] clients = new Client[THREADS];
    Thread[] threads = new Thread[THREADS];
    for (int i = 0; i < clients.length; i++) {
      //      clients[i] = Client.create(Client.Config.forTest(Client.Config.cluster3Hosts(HOST)));
      clients[i] = Client.create(Client.Config.forTest());
      final var client = clients[i];
      threads[i] = Thread.ofPlatform().start(() -> {
        run(client, counter);
      });
    }

    final var timerThread = Thread.ofPlatform().start(() -> {
      long before = 0L;
      long after = 0L;
      while (true) {
        try {
          Thread.sleep(1000);
        } catch (InterruptedException e) {
          throw new RuntimeException(e);
        }
        after = counter.get();

        System.out.println(after - before + " ops / second");
        before = after;
      }
    });

    for (var thread : threads) {
      thread.join();
    }
    timerThread.join();
  }

  private static void run(Client client, AtomicLong counter) {
    client.start();
    System.out.println("connected");
    final var msg = EventImpl.allocate(8192, Event.Type.EVENT);
    final var total = 2048;

    for (int i = 0; i < 10000; i++) {
      final var started = System.currentTimeMillis();
      for (int x = 0; x < total; x++) {
        //        msg.reset(Event.Type.EVENT.code);
        msg.size(32);
        while (!client.offer(msg)) {
          client.cluster().pollEgress();
          //          Thread.yield();
        }
        //      msg.waitForLedger();
        //      buf.putInt(0, i);
        //      cluster.sendKeepAlive();
        //      cluster.offer(buf, 0, 4);
        //      cluster.pollEgress();
        //      Thread.sleep(1000);
        counter.incrementAndGet();
      }
      final var offerCount = client.offerCount.get();
      final var baseline = offerCount - total;
      //      while (client.receivedCount.get() != client.offerCount.get()) {
      //        //        while (client.cluster().pollEgress() > 0) ;
      //        //        total-(offerCount-client.receivedCount.get());
      //        Thread.yield();
      //      }
      //      counter.addAndGet(total);

      //      final var elapsed = System.currentTimeMillis() - started;
      //      System.out.println(elapsed + "ms");
      //      System.out.println(((double) (elapsed * 1000) / (double) total) + " micros / msg");
      //      System.out.println(
      //        "Min Latency: " + ((double) client.minLatency.get() / 1000.0) + " micros / msg");
      //      System.out.println(
      //        "Max Latency: " + ((double) client.maxLatency.get() / 1000.0) + " micros / msg");
    }

    client.close();
  }

  @Test
  public void clientTest() throws Throwable {
    main(new String[0]);
    //    final var config = Client.Config.forTest();
    //    final var client = Client.create(config);
    //    client.start();
    //    Thread.sleep(TimeUnit.HOURS.toMillis(24));
  }
}
