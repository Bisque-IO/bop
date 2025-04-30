package bop.cluster;

import static org.agrona.concurrent.ringbuffer.RingBufferDescriptor.TRAILER_LENGTH;

import io.aeron.archive.Archive;
import io.aeron.archive.ArchiveThreadingMode;
import io.aeron.archive.checksum.Checksum;
import io.aeron.archive.client.AeronArchive;
import io.aeron.cluster.ClusteredMediaDriver;
import io.aeron.cluster.ConsensusModule;
import io.aeron.cluster.NanosecondClusterClock;
import io.aeron.cluster.service.ClusteredServiceContainer;
import io.aeron.driver.MediaDriver;
import io.aeron.driver.MinMulticastFlowControlSupplier;
import io.aeron.driver.ThreadingMode;
import io.aeron.security.DefaultAuthenticatorSupplier;
import java.io.File;
import java.net.Inet4Address;
import java.util.List;
import java.util.Objects;
import java.util.UUID;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.locks.ReentrantLock;
import java.util.function.Supplier;
import lombok.Builder;
import lombok.experimental.Accessors;
import lombok.extern.slf4j.Slf4j;
import org.agrona.IoUtil;
import org.agrona.concurrent.IdleStrategy;
import org.agrona.concurrent.NoOpLock;

@Slf4j
public class Server {
  private final Config config;
  private final ReentrantLock lock = new ReentrantLock();
  private MediaDriver.Context mediaDriverContext;
  private AeronArchive.Context replicationArchiveContext;
  private Archive.Context archiveContext;
  private AeronArchive aeronArchive;
  private AeronArchive.Context aeronArchiveContext;
  private ConsensusModule.Context consensusModuleContext;
  private ClusteredServiceContainer.Context clusteredServiceContext;
  private ClusteredMediaDriver clusteredMediaDriver;
  private ClusteredServiceContainer clusteredService;
  private ClusterKernel kernel;
  private Archives archives;

  private Server(Config config) {
    this.config = config;
  }

  public static Server create(Config config) {
    Objects.requireNonNull(config);
    return new Server(config);
  }

  private static void silent(Runnable r) {
    try {
      r.run();
    } catch (Throwable e) {
      // Ignore
    }
  }

  public static void main(String[] args) throws Throwable {
    //    SigInt.register(() -> {
    //      System.out.println("signal sent");
    //    });

    int nodeId = -1;
    String host = "127.0.0.1";
    for (int i = 0; i < args.length; i++) {
      if (args[i].trim().equals("-node")) {
        nodeId = Integer.parseInt(args[i + 1].trim());
      } else if (args[i].trim().equals("-host")) {
        host = args[i + 1].trim();
      }
    }

    Config config;
    if (nodeId > -1) {
      var hosts = Config.cluster3Hosts(host);
      System.out.println(
          "NODE ID: " + nodeId + "  ADDRESS: " + hosts.get(nodeId).host());
      config = Config.forTest(hosts.get(0).host, "" + nodeId).toBuilder()
          .members(hosts)
          .nodeId(nodeId)
          .build();
    } else {
      config = Config.forTest("127.0.0.1", "0");
    }
    final var server = Server.create(config);
    server.start();
    Thread.sleep(TimeUnit.HOURS.toMillis(24));
  }

  private void mediaDriverTerminated() {}

  public void start() throws Throwable {
    lock.lock();
    try {
      if (mediaDriverContext != null) {
        throw new IllegalStateException("already running");
      }

      final var archiveDir = new File(config.archiveDir);
      if (!archiveDir.exists()) {
        archiveDir.mkdirs();
      }
      if (!archiveDir.isDirectory()) {
        throw new IllegalStateException("archive dir is not a directory: " + archiveDir);
      }

      archives = Archives.create(archiveDir);

      mediaDriverContext = new MediaDriver.Context()
          .aeronDirectoryName(config.aeronDirName)
          .conductorIdleStrategy(config.conductorIdleStrategy)
          .receiverIdleStrategy(config.receiverIdleStrategy)
          .senderIdleStrategy(config.senderIdleStrategy)
          .errorHandler(this::onMediaDriverError)
          .threadingMode(config.threadingMode)
          .termBufferSparseFile(true)
          .multicastFlowControlSupplier(new MinMulticastFlowControlSupplier())
          .terminationHook(this::mediaDriverTerminated)
          .publicationTermBufferLength(config.termLength)
          .spiesSimulateConnection(true)
          .socketSndbufLength(config.socketSndbufLength)
          .socketRcvbufLength(config.socketRcvbufLength)
          .conductorBufferLength(config.conductorBufferLength)
          .ipcTermBufferLength(config.ipcTermBufferLength)
          .dirDeleteOnStart(config.driverDeleteOnStart)
          .dirDeleteOnShutdown(config.driverDeleteOnShutdown);

      final var node = config.members.get(config.nodeId);

      replicationArchiveContext = new AeronArchive.Context()
          .idleStrategy(config.replicationIdleStrategy)
          .errorHandler(this::onReplicationArchiveError)
          .controlResponseChannel(
              "aeron:udp?endpoint=" + node.host + ":" + node.replicationControlResponsePort);

      archiveContext = new Archive.Context()
          .idleStrategySupplier(config.archiveIdleStrategy)
          .errorHandler(this::onArchiveError)
          .aeronDirectoryName(config.aeronDirName)
          .archiveDir(new File(config.archiveDir))
          .authenticatorSupplier(DefaultAuthenticatorSupplier.INSTANCE)
          .controlChannel("aeron:udp?endpoint=" + node.host + ":" + node.archiveControlPort)
          .archiveClientContext(replicationArchiveContext)
          .localControlChannel("aeron:ipc?term-length=64k")
          .segmentFileLength(config.termLength)
          .controlTermBufferLength(config.termLength)
          .controlTermBufferSparse(true)
          .recordingEventsEnabled(false)
          .recordChecksum(config.recordChecksum)
          .replayChecksum(config.replayChecksum)
          .threadingMode(config.archiveThreadingMode)
          .replicationChannel(replicationArchiveContext.controlResponseChannel());

      aeronArchiveContext = new AeronArchive.Context()
          .idleStrategy(config.aeronArchiveIdleStrategy)
          .errorHandler(this::onAeronArchiveError)
          .lock(NoOpLock.INSTANCE)
          .controlRequestChannel(archiveContext.localControlChannel())
          .controlResponseChannel(archiveContext.localControlChannel())
          .controlRequestStreamId(archiveContext.localControlStreamId())
          .controlTermBufferLength(config.termLength)
          .controlTermBufferSparse(true)
          .aeronDirectoryName(config.aeronDirName);

      consensusModuleContext = new ConsensusModule.Context()
          .idleStrategySupplier(config.consensusIdleStrategy)
          .clusterClock(new NanosecondClusterClock())
          .errorHandler(this::onConsensusError)
          .clusterMemberId(config.nodeId)
          .clusterMembers(config.toAeronClusterMembers())
          .clusterDir(new File(config.clusterDir))
          .maxConcurrentSessions(16)
          .isIpcIngressAllowed(true)
          .ingressChannel("aeron:udp")
          .replicationChannel("aeron:udp?endpoint=" + node.host + ":0")
          .archiveContext(aeronArchiveContext.clone());

      kernel = new ClusterKernel(archives, consensusModuleContext);

      clusteredServiceContext = new ClusteredServiceContainer.Context()
          .idleStrategySupplier(config.clusterServiceIdleStrategy)
          .errorHandler(this::onClusteredServiceError)
          .aeronDirectoryName(config.aeronDirName)
          .archiveContext(aeronArchiveContext.clone())
          .serviceId(0)
          .clusterDir(new File(config.clusterDir))
          .clusteredService(kernel);

      kernel.serviceContainerContext = clusteredServiceContext;

      clusteredMediaDriver =
          ClusteredMediaDriver.launch(mediaDriverContext, archiveContext, consensusModuleContext);

      aeronArchive = AeronArchive.connect(aeronArchiveContext.clone());
      Tool.purgeRecordings(aeronArchive);

      clusteredService = ClusteredServiceContainer.launch(clusteredServiceContext);
    } catch (Throwable e) {
      log.error("Server.start() failed", e);
      doClose();
      throw e;
    } finally {
      lock.unlock();
    }
  }

  public void close() throws Exception {
    lock.lock();
    try {
      doClose();
    } finally {
      lock.unlock();
    }
  }

  private void doClose() {
    if (clusteredService != null) {
      silent(clusteredService::close);
      clusteredService = null;
    }
    if (clusteredMediaDriver != null) {
      silent(clusteredMediaDriver::close);
      clusteredMediaDriver = null;
    }
    if (clusteredServiceContext != null) {
      silent(clusteredServiceContext::close);
      clusteredServiceContext = null;
    }
    if (consensusModuleContext != null) {
      silent(consensusModuleContext::close);
      consensusModuleContext = null;
    }
    if (aeronArchiveContext != null) {
      silent(aeronArchiveContext::close);
      aeronArchiveContext = null;
    }
    if (archiveContext != null) {
      silent(archiveContext::close);
      archiveContext = null;
    }
    if (replicationArchiveContext != null) {
      silent(replicationArchiveContext::close);
      replicationArchiveContext = null;
    }
    if (mediaDriverContext != null) {
      silent(mediaDriverContext::close);
      mediaDriverContext = null;
    }
  }

  //  public boolean offer(Object message) {
  //    kernel.cluster().offer()
  //  }

  private void onMediaDriverError(Throwable e) {
    log.error("onMediaDriverError", e);
  }

  private void onArchiveError(Throwable e) {
    log.error("onArchiveError", e);
  }

  private void onReplicationArchiveError(Throwable e) {
    log.error("onReplicationArchiveError", e);
  }

  private void onAeronArchiveError(Throwable e) {
    log.error("onAeronArchiveError", e);
  }

  private void onConsensusError(Throwable e) {
    log.error("onConsensusError", e);
  }

  private void onClusteredServiceError(Throwable e) {
    log.error("onClusteredServiceError", e);
  }

  @Accessors(fluent = true)
  @Builder(toBuilder = true)
  public record Config(
      String aeronDirName,
      String clusterDir,
      String archiveDir,
      List<HostConfig> members,
      int nodeId,
      int termLength,
      int socketSndbufLength,
      int socketRcvbufLength,
      int conductorBufferLength,
      int ipcTermBufferLength,
      IdleStrategy conductorIdleStrategy,
      IdleStrategy receiverIdleStrategy,
      IdleStrategy senderIdleStrategy,
      IdleStrategy replicationIdleStrategy,
      Supplier<IdleStrategy> archiveIdleStrategy,
      IdleStrategy aeronArchiveIdleStrategy,
      Supplier<IdleStrategy> consensusIdleStrategy,
      Supplier<IdleStrategy> clusterServiceIdleStrategy,
      ThreadingMode threadingMode,
      ArchiveThreadingMode archiveThreadingMode,
      Checksum recordChecksum,
      Checksum replayChecksum,
      boolean driverTermBufferSparse,
      boolean controlTermBufferSparse,
      boolean driverDeleteOnStart,
      boolean driverDeleteOnShutdown) {
    public static List<HostConfig> cluster3Hosts() {
      return cluster3Hosts(Inet4Address.getLoopbackAddress().getHostAddress());
    }

    public static List<HostConfig> cluster3Hosts(String host) {
      return List.of(
          HostConfig.builder()
              .nodeId(0)
              .host(host)
              .clientFacingPort(9100)
              .memberPort(9101)
              .logPort(9102)
              .transferPort(9103)
              .archiveControlPort(9104)
              .replicationControlResponsePort(9105)
              .build(),
          HostConfig.builder()
              .nodeId(1)
              .host(host)
              .clientFacingPort(9200)
              .memberPort(9201)
              .logPort(9202)
              .transferPort(9203)
              .archiveControlPort(9204)
              .replicationControlResponsePort(9205)
              .build(),
          HostConfig.builder()
              .nodeId(2)
              .host(host)
              .clientFacingPort(9300)
              .memberPort(9301)
              .logPort(9302)
              .transferPort(9303)
              .archiveControlPort(9304)
              .replicationControlResponsePort(9305)
              .build());
    }

    public static Config forTest() {
      return forTest(
          Inet4Address.getLoopbackAddress().getHostAddress(),
          UUID.randomUUID().toString().replace("-", ""));
    }

    public static Config forTest(String host, String baseDir) {
      var userHome = System.getProperty("user.home");
      if (!userHome.endsWith(File.separator)) {
        userHome = userHome + File.separator;
      }
      final var aeronDir = new File(userHome + ".bus/" + baseDir + "/aeron");
      //      final var aeronDir = new File(".bus/aeron");
      final var clusterDir = new File(userHome + ".bus/" + baseDir + "/cluster");
      final var archiveDir = new File(userHome + ".bus/" + baseDir + "/archive");
      IoUtil.ensureDirectoryExists(aeronDir, "aeronDir");
      IoUtil.ensureDirectoryExists(clusterDir, "clusterDir");
      IoUtil.ensureDirectoryExists(archiveDir, "archiveDir");
      return Config.builder()
          .aeronDirName(aeronDir.getAbsolutePath())
          .clusterDir(clusterDir.getAbsolutePath())
          .archiveDir(archiveDir.getAbsolutePath())
          .members(List.of(HostConfig.builder()
              .host(host)
              .clientFacingPort(9100)
              .memberPort(9101)
              .logPort(9102)
              .transferPort(9103)
              .archiveControlPort(9104)
              .replicationControlResponsePort(9105)
              .build()))
          .nodeId(0)
          .termLength(1024 * 1024 * 64)
          .socketSndbufLength(1024 * 1024 * 8)
          .socketRcvbufLength(1024 * 1024 * 8)
          .conductorBufferLength(1024 * 1024 * 64 + TRAILER_LENGTH)
          .ipcTermBufferLength((1024 * 1024 * 64))
          .threadingMode(ThreadingMode.DEDICATED)
          .archiveThreadingMode(ArchiveThreadingMode.DEDICATED)
          .driverTermBufferSparse(true)
          .controlTermBufferSparse(true)
          .driverDeleteOnStart(true)
          .driverDeleteOnShutdown(true)
          .build();
    }

    public String toAeronClusterMembers() {
      final var sb = new StringBuilder();
      for (int i = 0; i < members.size(); i++) {
        final var member = members.get(i);
        sb.append(i);
        sb.append(',').append(member.host).append(':').append(member.clientFacingPort);
        sb.append(',').append(member.host).append(':').append(member.memberPort);
        sb.append(',').append(member.host).append(':').append(member.logPort);
        sb.append(',').append(member.host).append(':').append(member.transferPort);
        sb.append(',').append(member.host).append(':').append(member.archiveControlPort);
        if (i < members.size() - 1) {
          sb.append('|');
        }
      }
      return sb.toString();
    }
  }

  @Accessors(fluent = true)
  @Builder(toBuilder = true)
  public record HostConfig(
      int nodeId,
      String host,
      int clientFacingPort,
      int memberPort,
      int logPort,
      int transferPort,
      int archiveControlPort,
      int replicationControlResponsePort) {}
}
