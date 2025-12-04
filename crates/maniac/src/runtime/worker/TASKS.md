## Preemption

1. Add critical section functionality around task polling to prevent interrupts
2. Add proper epoch tracking for tasks and ensure preemption interrupts happen ONLY on expiration