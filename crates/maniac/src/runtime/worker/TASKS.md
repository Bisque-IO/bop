## Preemption

1. Add critical section functionality around task polling to prevent interrupts
2. Add proper epoch tracking for tasks and ensure preemption interrupts happen ONLY on expiration
3. Rewrite task polling with proper generator support

## Scheduler

1. Implement proper WorkerWaker signaling mechanism


example output of over-stealing:

```text
Config: workers=8 tasks=1024 yields/task=10000

  [tokio]  workers=8 tasks=1024 yields/task=10000
    Completed   1024 tasks in 84.127232ms (~12172.04 tasks/sec)
    Total yields: 10240000 (121.72M yields/sec)

  [maniac] workers=8 tasks=1024 yields/task=10000
    Completed   1024 tasks in 1.963868515s (~521.42 tasks/sec)
    Total yields: 10240000 (5.21M yields/sec)

    scheduler Stats:
  Workers: 8
  Arena:
    Total capacity: 262144
    Active tasks: 0
  Totals:
    Tasks polled: 10241024
    Completed: 1024
    Yielded: 10240000
    Waiting: 0
    CAS failures: 10931
    Empty scans: 9223854
    Yield queue polls: 0
    Signal polls: 10241024
    Steal attempts: 8677258 (successes: 396448)
    Leaf summary checks: 19464878 (hits: 11317875)
    Leaf steal attempts: 0 (successes: 0)
    Timer fires: 0
    Preempts: 0
  Per-worker:
    [0] polled=1358010 completed=137 yielded=1357873 steals=177892/3600465
    [1] polled=1201427 completed=118 yielded=1201309 steals=16318/607687
    [2] polled=1272290 completed=131 yielded=1272159 steals=52191/897434
    [3] polled=1286137 completed=127 yielded=1286010 steals=15011/559090
    [4] polled=1175653 completed=122 yielded=1175531 steals=45570/682518
    [5] polled=1146199 completed=111 yielded=1146088 steals=8592/320048
    [6] polled=1293956 completed=133 yielded=1293823 steals=34056/234734
    [7] polled=1507352 completed=145 yielded=1507207 steals=46818/1775282
```

example output of reasonable stealing:

```text
Config: workers=8 tasks=1024 yields/task=10000

  [tokio]  workers=8 tasks=1024 yields/task=10000
    Completed   1024 tasks in 77.821398ms (~13158.33 tasks/sec)
    Total yields: 10240000 (131.58M yields/sec)

  [maniac] workers=8 tasks=1024 yields/task=10000
    Completed   1024 tasks in 47.015489ms (~21780.06 tasks/sec)
    Total yields: 10240000 (217.80M yields/sec)

    scheduler Stats:
  Workers: 8
  Arena:
    Total capacity: 262144
    Active tasks: 0
  Totals:
    Tasks polled: 10241024
    Completed: 1024
    Yielded: 10240000
    Waiting: 0
    CAS failures: 18462
    Empty scans: 80888
    Yield queue polls: 0
    Signal polls: 10241024
    Steal attempts: 436676 (successes: 375256)
    Leaf summary checks: 10321912 (hits: 10259984)
    Leaf steal attempts: 0 (successes: 0)
    Timer fires: 0
    Preempts: 0
  Per-worker:
    [0] polled=1331263 completed=133 yielded=1331130 steals=1190/28976
    [1] polled=1316676 completed=135 yielded=1316541 steals=126557/131220
    [2] polled=1363146 completed=133 yielded=1363013 steals=277/5055
    [3] polled=1390778 completed=142 yielded=1390636 steals=200663/206833
    [4] polled=1199200 completed=117 yielded=1199083 steals=230/5854
    [5] polled=1259944 completed=125 yielded=1259819 steals=111/2546
    [6] polled=1176029 completed=115 yielded=1175914 steals=45925/49232
    [7] polled=1203988 completed=124 yielded=1203864 steals=303/6960
```
