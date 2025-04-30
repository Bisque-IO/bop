package bop.cluster;

import bop.cluster.message.EventImpl;
import io.aeron.ExclusivePublication;
import io.aeron.Image;
import io.aeron.Publication;
import io.aeron.cluster.ConsensusModule;
import io.aeron.cluster.codecs.CloseReason;
import io.aeron.cluster.service.ClientSession;
import io.aeron.cluster.service.Cluster;
import io.aeron.cluster.service.ClusteredService;
import io.aeron.cluster.service.ClusteredServiceContainer;
import io.aeron.logbuffer.Header;
import java.nio.ByteBuffer;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.concurrent.TimeUnit;
import lombok.extern.slf4j.Slf4j;
import org.agrona.DirectBuffer;
import org.agrona.MutableDirectBuffer;
import org.agrona.collections.Long2ObjectHashMap;
import org.agrona.concurrent.UnsafeBuffer;

/**
 * Kernel is responsible for managing the clustered state and dispatching events and managing
 * pending commands.
 */
@Slf4j
public class ClusterKernel implements ClusteredService {
  private Cluster cluster;
  private final ConsensusModule.Context consensusModuleContext;
  ClusteredServiceContainer.Context serviceContainerContext;
  private final Long2ObjectHashMap<Session> sessions = new Long2ObjectHashMap<>();
  private final Long2ObjectHashMap<TimerSet> timers = new Long2ObjectHashMap<>();
  private Cluster.Role role;
  private final MutableDirectBuffer egressBuffer =
      new UnsafeBuffer(ByteBuffer.allocateDirect(1024 * 1024));
  private final Archives archives;
  private Archives.Recording logRecording;
  private long backgroundWorkCounter;

  private final ArrayDeque<TimerSet> timerPool = new ArrayDeque<>();
  private final ArrayDeque<Task> taskPool = new ArrayDeque<>();

  public ClusterKernel(Archives archives, ConsensusModule.Context consensusModuleContext) {
    this.archives = archives;
    this.consensusModuleContext = consensusModuleContext;
  }

  public Cluster cluster() {
    return cluster;
  }

  @Override
  public void onStart(Cluster cluster, Image snapshotImage) {
    if (log.isInfoEnabled()) {
      log.info("onStart cluster={}  snapshotImage={}", cluster, snapshotImage);
    }
    logRecording = archives.get(0);
    if (logRecording == null) {
      log.info("no recordings");
    }
  }

  @Override
  public void onSessionOpen(ClientSession session, long timestamp) {
    if (log.isInfoEnabled()) {
      log.info("onSessionOpen ({}) {}", timestamp, session);
    }
    sessions.put(session.id(), new Session(session, timestamp));
    //    Thread.ofVirtual().start(() -> {
    //      final var buf = new UnsafeBuffer(ByteBuffer.allocateDirect(1024));
    //      long result;
    //      while (!session.isClosing()) {
    //        buf.putLong(0, backgroundWorkCounter);
    //        result = session.offer(buf, 0, 8);
    //        if (result == Publication.NOT_CONNECTED) {
    //          break;
    //        } else if (result == Publication.BACK_PRESSURED || result == Publication.ADMIN_ACTION)
    // {
    //
    //        } else if (result == Publication.CLOSED) {
    //          break;
    //        } else if (result == Publication.MAX_POSITION_EXCEEDED) {
    //
    //        }
    //
    //        try {
    //          Thread.sleep(1000);
    //        } catch (InterruptedException e) {
    //          break;
    //        }
    //      }
    //    });
  }

  @Override
  public void onSessionClose(ClientSession session, long timestamp, CloseReason closeReason) {
    if (log.isInfoEnabled()) {
      log.info(
          "onSessionClose timestamp={}  reason={}  session={}", timestamp, closeReason, session);
    }
    sessions.remove(session.id());
  }

  @Override
  public void onSessionMessage(
      ClientSession session,
      long timestamp,
      DirectBuffer buffer,
      int offset,
      int length,
      Header header) {
    //        if (log.isTraceEnabled()) {
    //          log.trace(
    //            "onSessionMessage timestamp={}  session={}  header={}  length={}",
    //            timestamp, session, header, length
    //          );
    //        }
    if (session == null) {
      return;
    }
    if (length < EventImpl.ID_OFFSET + 8) {
      return;
    }
    final var s = sessions.get(session.id());
    s.lastAt = timestamp;
    s.messageCount++;
    final var egressBuffer = this.egressBuffer;
    egressBuffer.putLong(0, buffer.getLong(offset + EventImpl.ID_OFFSET));
    egressBuffer.putLong(8, header.position() - header.frameLength());
    while (true) {
      long result = session.offer(egressBuffer, 0, 16);
      if (result == Publication.NOT_CONNECTED) {
        return;
      } else if (result == Publication.BACK_PRESSURED || result == Publication.ADMIN_ACTION) {
        Thread.yield();
      } else if (result == Publication.CLOSED) {
        return;
      } else if (result == Publication.MAX_POSITION_EXCEEDED) {
        Thread.yield();
      } else {
        return;
      }
    }
  }

  @Override
  public void onTimerEvent(long correlationId, long timestamp) {
    if (log.isTraceEnabled()) {
      log.trace("onTimerEvent correlationId={}  timestamp={}", correlationId, timestamp);
    }
  }

  @Override
  public void onTakeSnapshot(ExclusivePublication snapshotPublication) {
    if (log.isTraceEnabled()) {
      log.trace("onTakeSnapshot snapshotPublication={}", snapshotPublication);
    }
    log.info("onTakeSnapshot snapshotPublication={}", snapshotPublication);
  }

  @Override
  public void onRoleChange(Cluster.Role newRole) {
    if (log.isTraceEnabled()) {
      log.trace("onRoleChange newRole={}", newRole);
    }
    if (this.role == newRole) {
      return;
    }
    this.role = newRole;
  }

  @Override
  public void onTerminate(Cluster cluster) {
    if (log.isTraceEnabled()) {
      log.trace("onTerminate cluster={}", cluster);
    }
  }

  @Override
  public void onNewLeadershipTermEvent(
      long leadershipTermId,
      long logPosition,
      long timestamp,
      long termBaseLogPosition,
      int leaderMemberId,
      int logSessionId,
      TimeUnit timeUnit,
      int appVersion) {
    if (log.isTraceEnabled()) {
      log.trace(
          "onNewLeadershipTermEvent leadershipTermId={}  logPosition={}  timestamp={}  termBaseLogPosition={}  leaderMemberId={}  timeUnit={}  appVersion={}",
          leadershipTermId,
          logPosition,
          timestamp,
          termBaseLogPosition,
          leaderMemberId,
          timeUnit,
          appVersion);
    }
  }

  @Override
  public int doBackgroundWork(long nowNs) {
    backgroundWorkCounter++;
    //    if (backgroundWorkCounter % 1000 == 0 && log.isTraceEnabled()) {
    //      log.trace("doBackgroundWork nowNs={}", nowNs);
    //    }
    return 0;
  }

  /** */
  private static class Session {
    ClientSession session;
    long created;
    long lastAt;
    long messageCount;
    long eventCount;
    long commandCount;
    long pending;
    long acks;
    long naks;
    long lastPoll;
    long dirty;

    public Session(ClientSession session, long created) {
      this.session = session;
      this.created = created;
    }

    void wrap(ClientSession session, long created) {
      this.session = session;
      this.created = created;
      this.lastAt = 0L;
      this.messageCount = 0L;
      this.eventCount = 0L;
      this.commandCount = 0L;
      this.pending = 0L;
      this.acks = 0L;
      this.naks = 0L;
    }
  }

  /** */
  private static class Task {
    int term;
    int termOffset;
    int frameSize;
    long position;
    long deadline;
    int timerIndex;
    TimerSet timer;
    int attempt;
    int maxAttempts;
    Session assignedTo;
    long sentAt;
    long receivedAckAt;
    Archives.Segment segment;
    UnsafeBuffer buffer;
    int offset;
    int length;

    void reset() {
      term = 0;
      termOffset = 0;
      frameSize = 0;
      position = 0;
      deadline = 0;
      timerIndex = -1;
      timer = null;
      attempt = 0;
      maxAttempts = 0;
      assignedTo = null;
      sentAt = 0;
      receivedAckAt = 0;
      segment = null;
      buffer = null;
      offset = 0;
      length = 0;
    }
  }

  private static class TimerSet {
    private ClusterKernel kernel;
    private long deadline;
    private long correlationId;
    private Task[] tasks;
    private int size;
    private ArrayList<Runnable> runnables;

    public void add(Task task) {
      if (task.timerIndex >= 0) {}
    }

    public void remove(Task task) {
      if (task.timerIndex < 0) {
        return;
      }
      final var tasks = this.tasks;
      final var size = this.size;
      if (tasks == null || tasks.length == 0 || size == 0) {
        return;
      }
      if (task.timerIndex >= size) {
        return;
      }
      final var existing = tasks[task.timerIndex];
      if (existing != task) {
        return;
      }
      if (--this.size <= 0) {
        tasks[task.timerIndex] = null;
      } else {
        if (task.timerIndex == size - 1) {
          tasks[task.timerIndex] = null;
        } else {
          tasks[task.timerIndex] = tasks[size - 1];
        }
      }
    }
  }
}
