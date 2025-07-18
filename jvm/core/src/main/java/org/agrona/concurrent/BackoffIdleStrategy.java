/*
 * Copyright 2014-2025 Real Logic Limited.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * https://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
package org.agrona.concurrent;

import java.util.concurrent.locks.LockSupport;

@SuppressWarnings("unused")
abstract class BackoffIdleStrategyPrePad {
  byte p000,
      p001,
      p002,
      p003,
      p004,
      p005,
      p006,
      p007,
      p008,
      p009,
      p010,
      p011,
      p012,
      p013,
      p014,
      p015;
  byte p016,
      p017,
      p018,
      p019,
      p020,
      p021,
      p022,
      p023,
      p024,
      p025,
      p026,
      p027,
      p028,
      p029,
      p030,
      p031;
  byte p032,
      p033,
      p034,
      p035,
      p036,
      p037,
      p038,
      p039,
      p040,
      p041,
      p042,
      p043,
      p044,
      p045,
      p046,
      p047;
  byte p048,
      p049,
      p050,
      p051,
      p052,
      p053,
      p054,
      p055,
      p056,
      p057,
      p058,
      p059,
      p060,
      p061,
      p062,
      p063;
}

abstract class BackoffIdleStrategyData extends BackoffIdleStrategyPrePad {
  /** Denotes a non-idle state. */
  protected static final int NOT_IDLE = 0;

  /** Denotes a spinning state. */
  protected static final int SPINNING = 1;

  /** Denotes a yielding state. */
  protected static final int YIELDING = 2;

  /** Denotes a parking state. */
  protected static final int PARKING = 3;

  /** Max number of spins. */
  protected final long maxSpins;

  /** Max number of yields. */
  protected final long maxYields;

  /** Min park period in nanoseconds. */
  protected final long minParkPeriodNs;

  /** Max park period in nanoseconds. */
  protected final long maxParkPeriodNs;

  /** Current state. */
  protected int state = NOT_IDLE;

  /** Number of spins. */
  protected long spins;

  /** Number of yields. */
  protected long yields;

  /** Park period in nanoseconds. */
  protected long parkPeriodNs;

  BackoffIdleStrategyData(
      final long maxSpins,
      final long maxYields,
      final long minParkPeriodNs,
      final long maxParkPeriodNs) {
    this.maxSpins = maxSpins;
    this.maxYields = maxYields;
    this.minParkPeriodNs = minParkPeriodNs;
    this.maxParkPeriodNs = maxParkPeriodNs;
  }
}

/**
 * Idling strategy for threads when they have no work to do.
 *
 * <p>Spin for maxSpins, then {@link Thread#yield()} for maxYields, then
 * {@link LockSupport#parkNanos(long)} on an exponential backoff to maxParkPeriodNs.
 *
 * <p>Under Linux, multiple timer events will be coalesced in a 50 us window to minimize the timer
 * overhead on the CPU. E.g. if you want to wait 10 us, it could be you need to wait 50us. This
 * situation can be improved by changing the value of the timerslack_ns property which defaults to
 * 50000. This can be done like this: <code>
 *     echo 10000 &gt; /proc/PID/timerslack_ns
 * </code> This will set the timer slack to 10 microseconds for the given PID of the thread. This
 * property can't be set at the process level, so needs to be set for each thread specifically.
 * Also, it isn't guaranteed that after setting the property, the waiting time will be respected.
 */
public final class BackoffIdleStrategy extends BackoffIdleStrategyData implements IdleStrategy {
  /** Name to be returned from {@link #alias()}. */
  public static final String ALIAS = "backoff";

  /** Default number of times the strategy will spin without work before going to next state. */
  public static final long DEFAULT_MAX_SPINS = 10L;

  /** Default number of times the strategy will yield without work before going to next state. */
  public static final long DEFAULT_MAX_YIELDS = 5L;

  /** Default minimum interval the strategy will park a thread. */
  public static final long DEFAULT_MIN_PARK_PERIOD_NS = 1000L;

  /** Default maximum interval the strategy will park a thread. */
  public static final long DEFAULT_MAX_PARK_PERIOD_NS = 1_000_000L;

  byte p064,
      p065,
      p066,
      p067,
      p068,
      p069,
      p070,
      p071,
      p072,
      p073,
      p074,
      p075,
      p076,
      p077,
      p078,
      p079;
  byte p080,
      p081,
      p082,
      p083,
      p084,
      p085,
      p086,
      p087,
      p088,
      p089,
      p090,
      p091,
      p092,
      p093,
      p094,
      p095;
  byte p096,
      p097,
      p098,
      p099,
      p100,
      p101,
      p102,
      p103,
      p104,
      p105,
      p106,
      p107,
      p108,
      p109,
      p110,
      p111;
  byte p112,
      p113,
      p114,
      p115,
      p116,
      p117,
      p118,
      p119,
      p120,
      p121,
      p122,
      p123,
      p124,
      p125,
      p126,
      p127;

  /**
   * Default constructor using {@link #DEFAULT_MAX_SPINS}, {@link #DEFAULT_MAX_YIELDS},
   * {@link #DEFAULT_MIN_PARK_PERIOD_NS}, and {@link #DEFAULT_MAX_YIELDS}.
   */
  public BackoffIdleStrategy() {
    super(
        DEFAULT_MAX_SPINS,
        DEFAULT_MAX_YIELDS,
        DEFAULT_MIN_PARK_PERIOD_NS,
        DEFAULT_MAX_PARK_PERIOD_NS);
  }

  /**
   * Create a set of state tracking idle behavior.
   *
   * @param maxSpins to perform before moving to {@link Thread#yield()}
   * @param maxYields to perform before moving to {@link LockSupport#parkNanos(long)}
   * @param minParkPeriodNs to use when initiating parking
   * @param maxParkPeriodNs to use when parking
   */
  public BackoffIdleStrategy(
      final long maxSpins,
      final long maxYields,
      final long minParkPeriodNs,
      final long maxParkPeriodNs) {
    super(maxSpins, maxYields, minParkPeriodNs, maxParkPeriodNs);
  }

  /** {@inheritDoc} */
  public void idle(final int workCount) {
    if (workCount > 0) {
      reset();
    } else {
      idle();
    }
  }

  /** {@inheritDoc} */
  public void idle() {
    switch (state) {
      case NOT_IDLE:
        state = SPINNING;
        spins++;
        break;

      case SPINNING:
        Thread.onSpinWait();
        if (++spins > maxSpins) {
          state = YIELDING;
          yields = 0;
        }
        break;

      case YIELDING:
        if (++yields > maxYields) {
          state = PARKING;
          parkPeriodNs = minParkPeriodNs;
        } else {
          Thread.yield();
        }
        break;

      case PARKING:
        LockSupport.parkNanos(parkPeriodNs);
        parkPeriodNs = Math.min(parkPeriodNs << 1, maxParkPeriodNs);
        break;
    }
  }

  /** {@inheritDoc} */
  public void reset() {
    spins = 0;
    yields = 0;
    parkPeriodNs = minParkPeriodNs;
    state = NOT_IDLE;
  }

  /** {@inheritDoc} */
  public String alias() {
    return ALIAS;
  }

  /** {@inheritDoc} */
  public String toString() {
    return "BackoffIdleStrategy{" + "alias="
        + ALIAS + ", maxSpins="
        + maxSpins + ", maxYields="
        + maxYields + ", minParkPeriodNs="
        + minParkPeriodNs + ", maxParkPeriodNs="
        + maxParkPeriodNs + '}';
  }
}
