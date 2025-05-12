package bop.cluster;

/// Topic name with memoized XX3 hash. This should be used as constants "static final".
///
/// @param name
/// @param xx3
public record Topic(String name, long xx3) {
  public static Topic of(String name) {
    return new Topic(name, name.hashCode());
  }
}
