package io.bisque.bop.connector;

public enum ResponseCode {
    OK(0),
    ERR(1);

    private final int code;
    ResponseCode(int code) { this.code = code; }
    int asU8() { return code; }
    static ResponseCode fromU8(int v) throws WireIO.WireException {
        return switch (v) {
            case 0 -> OK;
            case 1 -> ERR;
            default -> throw new WireIO.WireException("Invalid ResponseCode: " + v);
        };
    }
}

