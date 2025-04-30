package bop.cluster;

import static org.agrona.concurrent.ringbuffer.RingBufferDescriptor.TRAILER_LENGTH;

import bop.cluster.message.EventImpl;
import bop.cluster.message.Message;
import io.aeron.Publication;
import io.aeron.cluster.client.AeronCluster;
import io.aeron.cluster.client.EgressListener;
import io.aeron.cluster.codecs.AdminRequestType;
import io.aeron.cluster.codecs.AdminResponseCode;
import io.aeron.cluster.codecs.EventCode;
import io.aeron.driver.MediaDriver;
import io.aeron.driver.ThreadingMode;
import io.aeron.logbuffer.Header;
import java.io.File;
import java.net.Inet4Address;
import java.util.List;
import java.util.Objects;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.locks.ReentrantLock;
import lombok.Builder;
import lombok.experimental.Accessors;
import lombok.extern.slf4j.Slf4j;
import org.agrona.DirectBuffer;
import org.agrona.collections.Long2ObjectHashMap;
import org.agrona.concurrent.*;

@Slf4j
public class Client {
  private final Egress egress;
  private final Config config;
  private final ReentrantLock lock = new ReentrantLock();
  private final IdleStrategy idleStrategy;
  private final ManyToOneConcurrentArrayQueue<Message> sendQueue =
      new ManyToOneConcurrentArrayQueue<>(8192);
  //  private final ConcurrentLong2ObjectMap<Message> offerMap = new
  // ConcurrentLong2ObjectMap<Message>();
  private final ConcurrentHashMap<Long, Message> offerMap = new ConcurrentHashMap<>();
  private final Long2ObjectHashMap<EventSubscriptions> eventSubs = new Long2ObjectHashMap<>();
  private final IdGenerator idGenerator;
  private final EpochNanoClock clock = new SystemEpochNanoClock();
  AtomicLong offerCount = new AtomicLong(0L);
  AtomicLong receivedCount = new AtomicLong(0L);
  AtomicLong offerLatency = new AtomicLong(0L);
  AtomicLong minLatency = new AtomicLong(0L);
  AtomicLong maxLatency = new AtomicLong(0L);
  private MediaDriver mediaDriver;
  private AeronCluster cluster;
  private Thread loopThread;
  private volatile boolean running;
  //  private AppRegistry registry;
  private long counter;
  private volatile int connectCount;

  private Client(Config config) {
    Objects.requireNonNull(config);
    this.config = config;
    this.egress = new Egress();
    this.idGenerator = new SnowflakeIdGenerator(config.nodeId);
    this.idleStrategy =
        config.idleStrategy == null ? new BackoffIdleStrategy() : config.idleStrategy;
  }

  public static Client create(Config config) {
    return new Client(config);
  }

  //  public void setRegistry(AppRegistry registry) {
  //    this.registry = registry;
  //  }

  public AeronCluster cluster() {
    return this.cluster;
  }

  public void start() {
    lock.lock();
    try {
      if (cluster != null) {
        doClose();
      }
      this.mediaDriver = MediaDriver.launch(new MediaDriver.Context()
          .aeronDirectoryName(config.aeronDirectoryName)
          .threadingMode(config.threadingMode)
          .cachedNanoClock(new CachedNanoClock())
          .nanoClock(SystemNanoClock.INSTANCE)
          .channelReceiveTimestampClock(new SystemEpochNanoClock())
          .channelSendTimestampClock(new SystemEpochNanoClock())
          .receiverCachedNanoClock(new CachedNanoClock())
          .senderCachedNanoClock(new CachedNanoClock())
          .socketRcvbufLength(config.socketRcvbufLength)
          .socketSndbufLength(config.socketSndbufLength)
          .conductorBufferLength(config.conductorBufferLength)
          .ipcTermBufferLength(config.ipcTermBufferLength)
          .dirDeleteOnStart(true)
          .dirDeleteOnShutdown(true));
      this.cluster = AeronCluster.connect(new AeronCluster.Context()
          .egressListener(egress)
          .egressChannel(config.egressChannel)
          .aeronDirectoryName(mediaDriver.aeronDirectoryName())
          .ingressChannel(config.ingressChannel)
          .ingressEndpoints(config.toAeronIngressEndpoints()));

      this.running = true;
      this.loopThread = Thread.ofPlatform().name("bus-client").start(this::runLoop);
    } finally {
      connectCount++;
      lock.unlock();
    }
  }

  public void close() {
    lock.lock();
    try {
      doClose();
    } finally {
      lock.unlock();
    }
  }

  private void doClose() {
    final var mediaDriver = this.mediaDriver;
    final var cluster = this.cluster;
    final var loopThread = this.loopThread;
    if (loopThread != null) {
      running = false;
      try {
        loopThread.interrupt();
      } catch (Throwable e) {
        // Ignore
      }
      try {
        loopThread.join();
      } catch (InterruptedException e) {
        // Ignore
      }
      this.loopThread = null;
    }
    this.mediaDriver = null;
    this.cluster = null;

    if (cluster != null) {
      try {
        cluster.close();
      } catch (final Exception e) {
        log.warn("AeronCluster.close threw exception", e);
      }
    }
    if (mediaDriver != null) {
      try {
        mediaDriver.close();
      } catch (final Exception e) {
        log.warn("MediaDriver.close threw exception", e);
      }
    }
  }

  public boolean offer(Message message) {
    final var buf = message.buffer();
    Objects.requireNonNull(buf);
    var cluster = this.cluster;
    if (cluster == null) {
      throw new IllegalStateException("not connected");
    }
    if (message.time() == 0L) {
      message.time(System.currentTimeMillis());
    }

    final long id = idGenerator.nextId();

    // Assign a new id
    message.id(id);
    message.sentNs(System.nanoTime());
    while (message.sentNs() == 0) {
      message.sentNs(System.nanoTime());
    }

    // Put into offer map to handle egress messages.
    offerMap.put(id, message);

    offerCount.incrementAndGet();

    long result = 0L;
    while ((result = cluster.offer(buf, 0, message.size())) < 0L) {
      if (result == Publication.NOT_CONNECTED) {

      } else if (result == Publication.BACK_PRESSURED || result == Publication.ADMIN_ACTION) {

      } else if (result == Publication.CLOSED) {
        return false;
      } else if (result == Publication.MAX_POSITION_EXCEEDED) {

      } else {
        idleStrategy.idle();
      }
    }

    message.sentNs(System.nanoTime());
    //    int count = 0;
    //    while (cluster.pollEgress() > 0) {
    //      count++;
    //      if (count >= 10) {
    //        return true;
    //      }
    //    }
    return true;
  }

  private void runLoop() {
    final var cluster = this.cluster;
    long timestamp = System.currentTimeMillis();
    long now = timestamp;
    while (running) {
      while (cluster.pollEgress() > 0) {}
      now = System.currentTimeMillis();
      if (now - timestamp >= 5_000) {
        cluster.sendKeepAlive();
        timestamp = now;
      }
      //      idleStrategy.reset();
      while (cluster.pollEgress() == 0) {
        //        idleStrategy.idle();
        Thread.yield();
        //        Thread.yield();
      }
    }
  }

  public enum State {
    INIT,
    CONNECTING,
    CONNECTED,
    TERMINATING,
    TERMINATED,
  }

  @Accessors(fluent = true)
  @Builder(toBuilder = true)
  public record Config(
      String aeronDirectoryName,
      int nodeId,
      int socketSndbufLength,
      int socketRcvbufLength,
      int conductorBufferLength,
      int ipcTermBufferLength,
      IdleStrategy conductorIdleStrategy,
      IdleStrategy receiverIdleStrategy,
      IdleStrategy senderIdleStrategy,
      IdleStrategy replicationIdleStrategy,
      ThreadingMode threadingMode,
      IdleStrategy idleStrategy,
      String egressChannel,
      String ingressChannel,
      List<ServerHost> ingressEndpoints) {
    public static List<ServerHost> cluster3Hosts(String host) {
      return List.of(
          ServerHost.of(host, 9100), ServerHost.of(host, 9200), ServerHost.of(host, 9300));
    }

    public static Config forTest() {
      var host = Inet4Address.getLoopbackAddress().getHostAddress();
      return forTest(List.of(ServerHost.of(host, 9100)));
    }

    public static Config forTest(List<ServerHost> serverHosts) {
      final var aeronDir =
          new File(".bus/client/" + UUID.randomUUID().toString().replace("-", ""));
      //      final var aeronDir = new File(".bus/aeron");

      //      String host = Inet4Address.getLoopbackAddress().getHostAddress();

      return Config.builder()
          .aeronDirectoryName(aeronDir.getAbsolutePath())
          .nodeId(0)
          .socketSndbufLength(1024 * 1024 * 8)
          .socketRcvbufLength(1024 * 1024 * 8)
          .conductorBufferLength(1024 * 1024 * 64 + TRAILER_LENGTH)
          .ipcTermBufferLength((1024 * 1024 * 64))
          .threadingMode(ThreadingMode.DEDICATED)
          .idleStrategy(new BusySpinIdleStrategy())
          .egressChannel("aeron:udp?endpoint=localhost:0")
          //          .egressChannel("aeron:udp?endpoint=" + host + ":0")
          //          .egressChannel("aeron:udp?endpoint=" + serverHosts.get(0).host + ":0")
          //          .ingressChannel("aeron:udp?endpoint=" + host + ":0")
          //          .egressChannel("aeron:udp")
          .ingressChannel("aeron:udp")
          .ingressEndpoints(serverHosts)
          .build();
    }

    public String toAeronIngressEndpoints() {
      final StringBuilder sb = new StringBuilder();
      for (int i = 0; i < ingressEndpoints.size(); i++) {
        final var endpoint = ingressEndpoints.get(i);
        sb.append(i).append('=');
        sb.append(endpoint.host).append(':').append(endpoint.clientFacingPort);
        sb.append(',');
      }
      sb.setLength(sb.length() - 1);
      return sb.toString();
    }

    public void validate() throws IllegalArgumentException {}
  }

  @Accessors(fluent = true)
  @Builder(toBuilder = true)
  public record ServerHost(String host, int clientFacingPort) {
    public static ServerHost of(String host, int port) {
      return new ServerHost(host, port);
    }
  }

  /** */
  private class EventSubscriptions {}

  private class Egress implements EgressListener {
    Thread thread;

    @Override
    public void onMessage(
        long clusterSessionId,
        long timestamp,
        DirectBuffer buffer,
        int offset,
        int length,
        Header header) {
      //      if (log.isTraceEnabled()) {
      //        log.trace("onMessage clusterSessionId={}  timestamp={}  nanoTs={}  header={}
      // length={}",
      //          clusterSessionId, timestamp, System.nanoTime(), header, length);
      //      }

      //      if (thread == null) {
      //        thread = Thread.currentThread();
      //      } else if (thread != Thread.currentThread()) {
      //        throw new RuntimeException("wrong thread");
      //      }

      if (length < 16) {
        return;
      }

      receivedCount.incrementAndGet();

      final var correlationId = buffer.getLong(offset);
      final var ledgerId = buffer.getLong(offset + 8);
      final var message = offerMap.remove(correlationId);

      if (message != null) {
        if (message instanceof EventImpl ei) {
          if (ei.sentNs() == 0) {
            throw new RuntimeException("0 sentNs");
          }
          final var latency = System.nanoTime() - ei.sentNs();
          if (latency > maxLatency.get()) {
            maxLatency.set(latency);
          }
          if (minLatency.get() == 0L) {
            minLatency.set(latency);
          } else if (minLatency.get() > latency) {
            minLatency.set(latency);
          }
          ei.pushLedgerId(ledgerId, System.nanoTime());
          //          offerLatency.addAndGet(ei.ledgerAckNs()/1000);
          //          log.trace("matched offer message  id={}  ledgerId={}  latency={}",
          //            correlationId, ledgerId, message.ledgerAckNs());
        }
      }
    }

    @Override
    public void onSessionEvent(
        long correlationId,
        long clusterSessionId,
        long leadershipTermId,
        int leaderMemberId,
        EventCode code,
        String detail) {
      if (log.isTraceEnabled()) {
        log.trace(
            "onSessionEvent correlationId={}  clusterSessionId={}  leadershipTermId={}  leaderMemberId={}  code={}  detail={}",
            correlationId,
            clusterSessionId,
            leadershipTermId,
            leaderMemberId,
            code,
            detail);
      }
    }

    @Override
    public void onNewLeader(
        long clusterSessionId, long leadershipTermId, int leaderMemberId, String ingressEndpoints) {
      if (log.isTraceEnabled()) {
        log.trace(
            "onNewLeader clusterSessionId={}  leadershipTermId={}  leaderMemberId={}  ingressEndpoints={}",
            clusterSessionId,
            leadershipTermId,
            leaderMemberId,
            ingressEndpoints);
      }
    }

    @Override
    public void onAdminResponse(
        long clusterSessionId,
        long correlationId,
        AdminRequestType requestType,
        AdminResponseCode responseCode,
        String message,
        DirectBuffer payload,
        int payloadOffset,
        int payloadLength) {
      if (log.isTraceEnabled()) {
        log.trace(
            "onAdminResponse clusterSessionId={}  correlationId={}  requestType={}  responseCode={}  "
                + "message={}  payloadLength={}",
            clusterSessionId,
            correlationId,
            requestType,
            responseCode,
            message,
            payloadLength);
      }
    }
  }
}
