package bop.alloc;

public class ByteString {
  protected byte[] data;
  protected int offset;
  protected int length;
  private volatile int dataRefs;

  public ByteString(byte[] data) {
    this.data = data;
    dataRefs = 1;
  }

  public ByteString(byte[] data, int offset, int length) {
    this.data = data;
    this.offset = offset;
    this.length = length;
    dataRefs = 1;
  }

  public ByteString(String data) {}

  public ByteString substring(int start, int end) {
    return new ByteString(data, offset + start, end - start);
  }
}
