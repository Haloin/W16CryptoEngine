#pragma once

#include <atomic>
#include <cstdint>
#include <type_traits>
#include <new>

namespace hft {

template<typename T, size_t Capacity>
class alignas(64) SPSCQueue {
    static_assert(std::is_trivially_copyable_v<T>);
    static_assert((Capacity & (Capacity - 1)) == 0);

    alignas(64) std::atomic<size_t> head_{0};
    alignas(64) std::atomic<size_t> tail_{0};
    alignas(64) T buffer_[Capacity];

    static constexpr size_t MASK = Capacity - 1;

public:
    SPSCQueue() = default;
    ~SPSCQueue() = default;

    SPSCQueue(const SPSCQueue&) = delete;
    SPSCQueue& operator=(const SPSCQueue&) = delete;

    bool push(const T& item) noexcept {
        const size_t current_tail = tail_.load(std::memory_order_relaxed);
        const size_t next_tail = (current_tail + 1) & MASK;

        if (__builtin_expect(next_tail == head_.load(std::memory_order_acquire), 0)) {
            return false;
        }

        buffer_[current_tail] = item;
        tail_.store(next_tail, std::memory_order_release);
        return true;
    }

    bool pop(T& item) noexcept {
        const size_t current_head = head_.load(std::memory_order_relaxed);

        if (__builtin_expect(current_head == tail_.load(std::memory_order_acquire), 0)) {
            return false;
        }

        item = buffer_[current_head];
        head_.store((current_head + 1) & MASK, std::memory_order_release);
        return true;
    }

    size_t size() const noexcept {
        const size_t head = head_.load(std::memory_order_relaxed);
        const size_t tail = tail_.load(std::memory_order_relaxed);
        return (tail - head) & MASK;
    }

    bool empty() const noexcept {
        return head_.load(std::memory_order_relaxed) == 
               tail_.load(std::memory_order_relaxed);
    }

    static constexpr size_t capacity() { return Capacity; }
};

template<typename T, size_t N>
class alignas(64) ObjectPool {
    static_assert(std::is_trivially_destructible_v<T>);

    alignas(64) std::array<T, N> pool_;
    alignas(64) std::atomic<size_t> next_{0};

public:
    ObjectPool() = default;

    template<typename... Args>
    T* acquire(Args&&... args) {
        size_t idx = next_.fetch_add(1, std::memory_order_relaxed);
        
        if (__builtin_expect(idx >= N, 0)) {
            return nullptr;
        }

        T* ptr = &pool_[idx];
        new (ptr) T(std::forward<Args>(args)...);
        return ptr;
    }

    void release(T* ptr) {
        (void)ptr;
    }

    void reset() {
        next_.store(0, std::memory_order_relaxed);
    }

    static constexpr size_t capacity() { return N; }
};

template<size_t Size>
class alignas(64) BumpAllocator {
    alignas(64) std::array<std::byte, Size> buffer_;
    alignas(64) std::atomic<size_t> offset_{0};

public:
    void* allocate(size_t n) noexcept {
        size_t current = offset_.load(std::memory_order_relaxed);
        size_t aligned_n = (n + 63) & ~63;
        
        if (__builtin_expect(current + aligned_n > Size, 0)) {
            return nullptr;
        }

        while (!offset_.compare_exchange_weak(
            current, current + aligned_n,
            std::memory_order_relaxed,
            std::memory_order_relaxed)) {
            
            if (__builtin_expect(current + aligned_n > Size, 0)) {
                return nullptr;
            }
        }

        return &buffer_[current];
    }

    void deallocate(void*) noexcept {}

    void reset() noexcept {
        offset_.store(0, std::memory_order_relaxed);
    }

    size_t used() const noexcept {
        return offset_.load(std::memory_order_relaxed);
    }
};

}
