package bop.c.mdbx;

public interface DebugFlags {
  int NONE = 0;
  int ASSERT = 1;
  int AUDIT = 2;
  int JITTER = 4;
  int DUMP = 8;
  int LEGACY_MULTIOPEN = 16;
  int LEGACY_OVERLAP = 32;
  int DONT_UPGRADE = 64;
  int DONTCHANGE = -1;
}
