# Engine Notes

Some design decisions and target numbers for the C++ matching engine.

## Targets

- Tick-to-trade: <500µs (not counting network)
- Jitter: <10µs internally
- Zero heap allocations after init
- Lock-free everything on hot paths

## CPU pinning

We isolate cores to avoid scheduler noise. Boot with:

```bash
isolcpus=1,2,3 nohz_full=1,2,3 rcu_nocbs=1,2,3
```

Threads are pinned like this:

| Role | Core | Priority |
|------|------|----------|
| Market data | 1 | 99 (SCHED_FIFO) |
| Strategy | 2 | 98 (SCHED_FIFO) |
| Execution | 3 | 97 (SCHED_FIFO) |

Also `mlockall()` the process so pages don't get swapped out mid-trade.

## Memory

Everything is preallocated at startup. Hot structures are 64-byte aligned so they don't false-share across cache lines. We use a bump allocator over a preallocated arena — just an atomic offset, no malloc.

## Lock-free queue

Single-producer single-consumer ring buffer. Head and tail are on separate cache lines. Producer writes tail with `release`, consumer reads head with `acquire`. Power-of-2 size so indexing is a mask instead of modulo.

## Branch hints

Hot paths are annotated with `__builtin_expect`. A mispredict costs 15-20 cycles, so we mark the common case as likely and bailouts as unlikely.

## JSON parsing

We use simdjson on-demand API — no DOM, no string copies, no exceptions. Just pull fields directly out of the buffer.

## Networking

Currently Boost.Asio with `TCP_NODELAY` and persistent connections. Might move to io_uring or DPDK later for lower latency.

## Crypto fast path

- Precompute the EIP-712 domain separator once
- Cache the signing public key
- Use libsecp256k1 with a precomputed signing context

Per order: hash the message struct, hash domain+message together, sign.

## Misc tricks

- HugePages: `mmap(..., MAP_HUGETLB, ...)` — fewer TLB misses
- NUMA pinning: `numactl --cpunodebind=0 --membind=0 ./hft_engine`
- TSC-based timing: `rdtsc()` before/after hot paths
- Prefetching: `__builtin_prefetch(&next_order, 0, 3)`

## Build flags

```bash
g++ -O3 -march=native -mtune=native \
    -fno-exceptions -fno-rtti \
    -flto -fwhole-program \
    -DNDEBUG \
    hft_engine.cpp -o hft_engine
```

## Sanity checks before deploying

- No `new`/`malloc` in hot path
- No mutexes anywhere
- No exceptions (compile with `-fno-exceptions`)
- No virtual calls on hot path
- All hot structs are `alignas(64)`
- CPU isolation actually worked (`taskset -pc`)
- Realtime priority stuck (`chrt -p`)
- Memory locked (`VmLck` in `/proc/<pid>/status`)
