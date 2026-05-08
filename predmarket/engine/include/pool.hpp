#pragma once

#include <cstdint>
#include <cstddef>
#include <type_traits>
#include <immintrin.h>

namespace predmarket {

template<typename T, std::size_t Capacity>
class alignas(64) SlabPool {
public:
    SlabPool() noexcept : m_top(0) {
        for (std::size_t i = 0; i < Capacity; ++i) {
            m_free[i] = static_cast<std::uint32_t>(Capacity - 1 - i);
        }
    }

    SlabPool(const SlabPool&)            = delete;
    SlabPool& operator=(const SlabPool&) = delete;

    __attribute__((always_inline))
    T* acquire() noexcept {
        if ([[unlikely]] m_top == 0) return nullptr;
        std::uint32_t idx = m_free[--m_top];
        return std::launder(reinterpret_cast<T*>(&m_storage[idx]));
    }

    __attribute__((always_inline))
    void release(T* ptr) noexcept {
        std::uint32_t idx = static_cast<std::uint32_t>(
            reinterpret_cast<AlignedStorage*>(ptr) - &m_storage[0]
        );
        m_free[m_top++] = idx;
    }

    static constexpr std::size_t capacity() noexcept { return Capacity; }

private:
    using AlignedStorage = std::aligned_storage_t<sizeof(T), alignof(T)>;

    alignas(64) AlignedStorage   m_storage[Capacity];
    alignas(64) std::uint32_t    m_free[Capacity];
    std::uint32_t                m_top;
};

}
