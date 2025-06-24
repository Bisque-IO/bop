package bop.cluster.message;

import lombok.Builder;
import lombok.experimental.Accessors;

@Accessors(fluent = true)
@Builder(toBuilder = true)
public record MessagePoolConfig(
    int minSize,
    int maxSize,
    int minReliableSize,
    int maxReliableSize,
    int partitions,
    int maxIdleMillis,
    int maxWaitMillis,
    int scavengeIntervalMilliseconds,
    double scavengeRatio) {
  public MessagePoolConfig validate() {
    final var b = toBuilder();
    if (minSize <= 0) {
      b.minSize = 0;
    }
    if (maxSize < 128) {
      b.maxSize = 128;
    }
    if (minReliableSize <= 0) {
      b.minReliableSize = b.minSize;
    }
    if (maxReliableSize <= 0) {
      b.maxReliableSize = Math.max(b.maxSize, b.minReliableSize);
    }
    if (partitions <= 0) {
      b.partitions = Runtime.getRuntime().availableProcessors();
    }
    if (maxIdleMillis < 1000) {
      b.maxIdleMillis = 1000;
    }
    if (maxWaitMillis <= 0) {
      b.maxWaitMillis = 50;
    }
    if (scavengeIntervalMilliseconds < 5000) {
      b.scavengeIntervalMilliseconds = 1000 * 60 * 2;
    }
    if (scavengeRatio < 0.1) {
      b.scavengeRatio = 0.5;
    } else if (scavengeRatio > 1.0) {
      b.scavengeRatio = 1.0;
    }
    return b.build();
  }
}
