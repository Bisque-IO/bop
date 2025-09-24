Goal: Refine and Optimize


TailState does not need a "pending", we only care about the absolute tail single value, zero allocations. Atomically overwrite the durable size. We don't need anything else since a reader can utilize the Segment id to get latest stats.