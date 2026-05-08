#pragma once

#include <pthread.h>
#include <sched.h>
#include <unistd.h>
#include <sys/mman.h>
#include <errno.h>
#include <cstdint>
#include <atomic>
#include <array>
#include <new>

namespace hft {

constexpr size_t CPU_CACHE_LINE = 64;
constexpr int MARKET_DATA_CORE = 1;
constexpr int STRATEGY_CORE = 2;
constexpr int EXECUTION_CORE = 3;

inline void pin_thread_to_core(int core_id) {
    cpu_set_t cpuset;
    CPU_ZERO(&cpuset);
    CPU_SET(core_id, &cpuset);
    
    pthread_t thread = pthread_self();
    int ret = pthread_setaffinity_np(thread, sizeof(cpu_set_t), &cpuset);
    if (ret != 0) {
        __builtin_trap();
    }
}

inline void set_realtime_priority(int priority = 99) {
    struct sched_param param;
    param.sched_priority = priority;
    
    int ret = sched_setscheduler(0, SCHED_FIFO, &param);
    if (ret != 0) {
        __builtin_trap();
    }
}

inline void lock_memory() {
    if (mlockall(MCL_CURRENT | MCL_FUTURE) == -1) {
        __builtin_trap();
    }
}

inline void prefault_stack(size_t stack_size = 8 * 1024 * 1024) {
    volatile char buffer[stack_size];
    for (size_t i = 0; i < stack_size; i += 4096) {
        buffer[i] = 0;
    }
}

inline uint64_t rdtsc() {
    uint32_t lo, hi;
    __asm__ __volatile__ ("rdtsc" : "=a" (lo), "=d" (hi));
    return ((uint64_t)hi << 32) | lo;
}

template<typename T, size_t N>
class alignas(CPU_CACHE_LINE) CacheAlignedArray {
    std::array<T, N> data_;
    char padding_[CPU_CACHE_LINE - (N * sizeof(T)) % CPU_CACHE_LINE];

public:
    T& operator[](size_t i) { return data_[i]; }
    const T& operator[](size_t i) const { return data_[i]; }
    static constexpr size_t size() { return N; }
};

class ThreadConfig {
public:
    enum class Role {
        MarketData,
        Strategy,
        Execution
    };

    static void configure(Role role) {
        lock_memory();
        
        switch (role) {
            case Role::MarketData:
                pin_thread_to_core(MARKET_DATA_CORE);
                set_realtime_priority(99);
                break;
            case Role::Strategy:
                pin_thread_to_core(STRATEGY_CORE);
                set_realtime_priority(98);
                break;
            case Role::Execution:
                pin_thread_to_core(EXECUTION_CORE);
                set_realtime_priority(97);
                break;
        }
        
        prefault_stack();
    }
};

}
