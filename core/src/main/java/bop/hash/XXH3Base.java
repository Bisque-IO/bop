/*
 * This implementation was derived from
 *
 * https://github.com/OpenHFT/Zero-Allocation-Hashing/blob/zero-allocation-hashing-0.16/src/main/java/net/openhft/hashing/XXH3.java
 *
 * which was published under the license below:
 *
 * Copyright 2015 Higher Frequency Trading http://www.higherfrequencytrading.com
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
package bop.hash;

abstract class XXH3Base {
  protected static final int BLOCK_LEN_EXP = 10;

  protected static final long SECRET_00 = 0xbe4ba423396cfeb8L;
  protected static final long SECRET_01 = 0x1cad21f72c81017cL;
  protected static final long SECRET_02 = 0xdb979083e96dd4deL;
  protected static final long SECRET_03 = 0x1f67b3b7a4a44072L;
  protected static final long SECRET_04 = 0x78e5c0cc4ee679cbL;
  protected static final long SECRET_05 = 0x2172ffcc7dd05a82L;
  protected static final long SECRET_06 = 0x8e2443f7744608b8L;
  protected static final long SECRET_07 = 0x4c263a81e69035e0L;
  protected static final long SECRET_08 = 0xcb00c391bb52283cL;
  protected static final long SECRET_09 = 0xa32e531b8b65d088L;
  protected static final long SECRET_10 = 0x4ef90da297486471L;
  protected static final long SECRET_11 = 0xd8acdea946ef1938L;
  protected static final long SECRET_12 = 0x3f349ce33f76faa8L;
  protected static final long SECRET_13 = 0x1d4f0bc7c7bbdcf9L;
  protected static final long SECRET_14 = 0x3159b4cd4be0518aL;
  protected static final long SECRET_15 = 0x647378d9c97e9fc8L;
  protected static final long SECRET_16 = 0xc3ebd33483acc5eaL;
  protected static final long SECRET_17 = 0xeb6313faffa081c5L;
  protected static final long SECRET_18 = 0x49daf0b751dd0d17L;
  protected static final long SECRET_19 = 0x9e68d429265516d3L;
  protected static final long SECRET_20 = 0xfca1477d58be162bL;
  protected static final long SECRET_21 = 0xce31d07ad1b8f88fL;
  protected static final long SECRET_22 = 0x280416958f3acb45L;
  protected static final long SECRET_23 = 0x7e404bbbcafbd7afL;

  protected static final long INIT_ACC_0 = 0x00000000C2B2AE3DL;
  protected static final long INIT_ACC_1 = 0x9E3779B185EBCA87L;
  protected static final long INIT_ACC_2 = 0xC2B2AE3D27D4EB4FL;
  protected static final long INIT_ACC_3 = 0x165667B19E3779F9L;
  protected static final long INIT_ACC_4 = 0x85EBCA77C2B2AE63L;
  protected static final long INIT_ACC_5 = 0x0000000085EBCA77L;
  protected static final long INIT_ACC_6 = 0x27D4EB2F165667C5L;
  protected static final long INIT_ACC_7 = 0x000000009E3779B1L;

  protected final long secret00;
  protected final long secret01;
  protected final long secret02;
  protected final long secret03;
  protected final long secret04;
  protected final long secret05;
  protected final long secret06;
  protected final long secret07;
  protected final long secret08;
  protected final long secret09;
  protected final long secret10;
  protected final long secret11;
  protected final long secret12;
  protected final long secret13;
  protected final long secret14;
  protected final long secret15;
  protected final long secret16;
  protected final long secret17;
  protected final long secret18;
  protected final long secret19;
  protected final long secret20;
  protected final long secret21;
  protected final long secret22;
  protected final long secret23;

  protected final long secret[];

  protected final long secShift00;
  protected final long secShift01;
  protected final long secShift02;
  protected final long secShift03;
  protected final long secShift04;
  protected final long secShift05;
  protected final long secShift06;
  protected final long secShift07;
  protected final long secShift08;
  protected final long secShift09;
  protected final long secShift10;
  protected final long secShift11;

  protected final long secShift16;
  protected final long secShift17;
  protected final long secShift18;
  protected final long secShift19;
  protected final long secShift20;
  protected final long secShift21;
  protected final long secShift22;
  protected final long secShift23;

  protected final long secShiftFinal0;
  protected final long secShiftFinal1;
  protected final long secShiftFinal2;
  protected final long secShiftFinal3;
  protected final long secShiftFinal4;
  protected final long secShiftFinal5;
  protected final long secShiftFinal6;
  protected final long secShiftFinal7;

  protected XXH3Base(long seed) {
    this.secret00 = SECRET_00 + seed;
    this.secret01 = SECRET_01 - seed;
    this.secret02 = SECRET_02 + seed;
    this.secret03 = SECRET_03 - seed;
    this.secret04 = SECRET_04 + seed;
    this.secret05 = SECRET_05 - seed;
    this.secret06 = SECRET_06 + seed;
    this.secret07 = SECRET_07 - seed;
    this.secret08 = SECRET_08 + seed;
    this.secret09 = SECRET_09 - seed;
    this.secret10 = SECRET_10 + seed;
    this.secret11 = SECRET_11 - seed;
    this.secret12 = SECRET_12 + seed;
    this.secret13 = SECRET_13 - seed;
    this.secret14 = SECRET_14 + seed;
    this.secret15 = SECRET_15 - seed;
    this.secret16 = SECRET_16 + seed;
    this.secret17 = SECRET_17 - seed;
    this.secret18 = SECRET_18 + seed;
    this.secret19 = SECRET_19 - seed;
    this.secret20 = SECRET_20 + seed;
    this.secret21 = SECRET_21 - seed;
    this.secret22 = SECRET_22 + seed;
    this.secret23 = SECRET_23 - seed;

    this.secShift00 = (SECRET_00 >>> 24) + (SECRET_01 << 40) + seed;
    this.secShift01 = (SECRET_01 >>> 24) + (SECRET_02 << 40) - seed;
    this.secShift02 = (SECRET_02 >>> 24) + (SECRET_03 << 40) + seed;
    this.secShift03 = (SECRET_03 >>> 24) + (SECRET_04 << 40) - seed;
    this.secShift04 = (SECRET_04 >>> 24) + (SECRET_05 << 40) + seed;
    this.secShift05 = (SECRET_05 >>> 24) + (SECRET_06 << 40) - seed;
    this.secShift06 = (SECRET_06 >>> 24) + (SECRET_07 << 40) + seed;
    this.secShift07 = (SECRET_07 >>> 24) + (SECRET_08 << 40) - seed;
    this.secShift08 = (SECRET_08 >>> 24) + (SECRET_09 << 40) + seed;
    this.secShift09 = (SECRET_09 >>> 24) + (SECRET_10 << 40) - seed;
    this.secShift10 = (SECRET_10 >>> 24) + (SECRET_11 << 40) + seed;
    this.secShift11 = (SECRET_11 >>> 24) + (SECRET_12 << 40) - seed;

    this.secShift16 = secret15 >>> 8 | secret16 << 56;
    this.secShift17 = secret16 >>> 8 | secret17 << 56;
    this.secShift18 = secret17 >>> 8 | secret18 << 56;
    this.secShift19 = secret18 >>> 8 | secret19 << 56;
    this.secShift20 = secret19 >>> 8 | secret20 << 56;
    this.secShift21 = secret20 >>> 8 | secret21 << 56;
    this.secShift22 = secret21 >>> 8 | secret22 << 56;
    this.secShift23 = secret22 >>> 8 | secret23 << 56;

    this.secShiftFinal0 = secret01 >>> 24 | secret02 << 40;
    this.secShiftFinal1 = secret02 >>> 24 | secret03 << 40;
    this.secShiftFinal2 = secret03 >>> 24 | secret04 << 40;
    this.secShiftFinal3 = secret04 >>> 24 | secret05 << 40;
    this.secShiftFinal4 = secret05 >>> 24 | secret06 << 40;
    this.secShiftFinal5 = secret06 >>> 24 | secret07 << 40;
    this.secShiftFinal6 = secret07 >>> 24 | secret08 << 40;
    this.secShiftFinal7 = secret08 >>> 24 | secret09 << 40;

    this.secret =
        new long[] {
          secret00, secret01, secret02, secret03, secret04, secret05, secret06, secret07,
          secret08, secret09, secret10, secret11, secret12, secret13, secret14, secret15,
          secret16, secret17, secret18, secret19, secret20, secret21, secret22, secret23
        };
  }

  protected static long unsignedLongMulXorFold(final long lhs, final long rhs) {
    long upper = Maths.unsignedMultiplyHigh(lhs, rhs);
    long lower = lhs * rhs;
    return lower ^ upper;
  }

  protected static long avalanche64(long h64) {
    h64 ^= h64 >>> 33;
    h64 *= INIT_ACC_2;
    h64 ^= h64 >>> 29;
    h64 *= INIT_ACC_3;
    return h64 ^ (h64 >>> 32);
  }

  protected static long avalanche3(long h64) {
    h64 ^= h64 >>> 37;
    h64 *= 0x165667919E3779F9L;
    return h64 ^ (h64 >>> 32);
  }

  protected static long mix2Accs(final long lh, final long rh, long sec0, long sec8) {
    return unsignedLongMulXorFold(lh ^ sec0, rh ^ sec8);
  }

  protected static long contrib(long a, long b) {
    long k = a ^ b;
    return (0xFFFFFFFFL & k) * (k >>> 32);
  }

  protected static long mixAcc(long acc, long sec) {
    return (acc ^ (acc >>> 47) ^ sec) * INIT_ACC_7;
  }

  protected abstract long finish12Bytes(long a, long b);

  public long hashLongIntToLong(long v1, int v2) {
    return finish12Bytes(v1, ((long) v2 << 32) ^ (v1 >>> 32));
  }

  public long hashIntIntIntToLong(int v1, int v2, int v3) {
    return finish12Bytes(
        (v1 & 0xFFFFFFFFL) ^ ((long) v2 << 32), ((long) v3 << 32) ^ (v2 & 0xFFFFFFFFL));
  }

  public long hashIntLongToLong(int v1, long v2) {
    return finish12Bytes((v1 & 0xFFFFFFFFL) ^ (v2 << 32), v2);
  }
}
