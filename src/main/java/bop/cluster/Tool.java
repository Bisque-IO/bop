package bop.cluster;

import io.aeron.archive.client.AeronArchive;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;

public class Tool {
  public static final String CONTROL_REQUEST_CHANNEL = "aeron:udp?endpoint=127.0.0.1:8010";
  public static final String CONTROL_RESPONSE_CHANNEL = "aeron:udp?endpoint=127.0.0.1:0";

  public static void main(String[] args) {
    String nodeId = "";
    //    String aeronDir = args[0];
    //    String controlEndpoint = args[0].trim();
    String controlEndpoint = "192.168.8.131:9104";
    String archiveDir = "";
    //    for (int i = 0; i < args.length+1; i+=2) {
    //      if (args[i].equals("-node")) {
    //        nodeId = args[i+1];
    //      } else if (args[i].equals("-aeronDir")) {
    //        aeronDir = args[i+1].trim();
    //        if (aeronDir.startsWith("\"") && aeronDir.endsWith("\"")) {
    //          aeronDir = aeronDir.substring(1, aeronDir.length()-1);
    //        }
    //      } else if (args[i].equals("-archiveDir")) {
    //        archiveDir = args[i+1].trim();
    //        if (archiveDir.startsWith("\"") && archiveDir.endsWith("\"")) {
    //          archiveDir = archiveDir.substring(1, archiveDir.length()-1);
    //        }
    //      }
    //    }

    try (AeronArchive archive = AeronArchive.connect(new AeronArchive.Context()
        .aeronDirectoryName("/home/me/.bus/0/aeron")
        .controlRequestChannel(CONTROL_REQUEST_CHANNEL)
        .controlResponseChannel(CONTROL_RESPONSE_CHANNEL))) {
      purgeRecordings(archive);
    }
  }

  public static void purgeRecordings(final AeronArchive archive) {
    // Step 1: Collect all recordings
    List<RecordingInfo> recordings = new ArrayList<>();
    archive.listRecordings(
        0,
        Integer.MAX_VALUE,
        (controlSessionId,
            correlationId,
            recordingId,
            startTimestamp,
            stopTimestamp,
            startPosition,
            stopPosition,
            initialTermId,
            segmentFileLength,
            termBufferLength,
            mtuLength,
            sessionId,
            streamId,
            strippedChannel,
            originalChannel,
            sourceIdentity) -> {
          recordings.add(new RecordingInfo(
              recordingId,
              startTimestamp,
              stopTimestamp,
              streamId,
              strippedChannel,
              originalChannel));
        });

    for (RecordingInfo recordingInfo : recordings) {
      System.out.println(recordingInfo);
    }

    recordings.sort(Comparator.comparingLong(o -> o.startTimestamp));

    final var snapshots = recordings.stream().filter(r -> r.isSnapshot()).toList();
    for (var snapshot : snapshots) {}

    // Step 2: Find the latest snapshot recording
    RecordingInfo latestSnapshot = recordings.stream()
        .filter(RecordingInfo::isSnapshot)
        .max(Comparator.comparingLong(r -> r.startTimestamp))
        .orElse(null);

    if (latestSnapshot == null) {
      System.out.println("No snapshot recordings found. Nothing to purge.");
      return;
    }

    System.out.println("Latest snapshot recording ID: " + latestSnapshot.recordingId);

    // Step 3: Purge all recordings older than the latest snapshot
    for (RecordingInfo recording : recordings) {
      if (recording.startTimestamp < latestSnapshot.startTimestamp) {
        System.out.println("Purging old recording: " + recording.recordingId);
        archive.purgeRecording(recording.recordingId);
      }
    }
  }

  record RecordingInfo(
      long recordingId,
      long startTimestamp,
      long stopTimestamp,
      int streamId,
      String strippedChannel,
      String originalChannel) {

    boolean isSnapshot() {
      // Aeron cluster snapshots are usually on a specific stream ID
      // Common default for snapshots: streamId == 100
      return streamId == 100
          || strippedChannel.contains(
              "snapshot"); // or check strippedChannel contains "snapshot" if you customize
    }
  }
}
