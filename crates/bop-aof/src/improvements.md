Goal: Refine and Optimize

- Aof::append_record needs a variant Aof::append_reserve and Aof::append_reserve_complete to allow for zero-copy in-place appends
- Aof::open_reader does a full scan of Vec which is O(n), HashMap instead of Vec would make it O(1)

