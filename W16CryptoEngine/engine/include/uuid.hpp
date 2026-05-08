#pragma once

#include <cstdint>
#include <cstring>
#include <cstdio>
#include <x86intrin.h>

namespace predmarket {

struct alignas(16) Uuid {
    std::uint8_t bytes[16];

    static __attribute__((always_inline)) Uuid from_bytes(const std::uint8_t* p) noexcept {
        Uuid u;
        std::memcpy(u.bytes, p, 16);
        return u;
    }

    __attribute__((always_inline))
    bool operator==(const Uuid& o) const noexcept {
        std::uint64_t a0, a1, b0, b1;
        std::memcpy(&a0, bytes,     8);
        std::memcpy(&a1, bytes + 8, 8);
        std::memcpy(&b0, o.bytes,     8);
        std::memcpy(&b1, o.bytes + 8, 8);
        return (a0 == b0) & (a1 == b1);
    }

    __attribute__((always_inline))
    bool operator!=(const Uuid& o) const noexcept {
        return !(*this == o);
    }

    void to_str(char out[37]) const noexcept {
        static constexpr char hex[] = "0123456789abcdef";
        const std::uint8_t* b = bytes;
        int i = 0;
        for (int j : {0,1,2,3})   { out[i++] = hex[b[j]>>4]; out[i++] = hex[b[j]&0xf]; }
        out[i++] = '-';
        for (int j : {4,5})       { out[i++] = hex[b[j]>>4]; out[i++] = hex[b[j]&0xf]; }
        out[i++] = '-';
        for (int j : {6,7})       { out[i++] = hex[b[j]>>4]; out[i++] = hex[b[j]&0xf]; }
        out[i++] = '-';
        for (int j : {8,9})       { out[i++] = hex[b[j]>>4]; out[i++] = hex[b[j]&0xf]; }
        out[i++] = '-';
        for (int j : {10,11,12,13,14,15}) { out[i++] = hex[b[j]>>4]; out[i++] = hex[b[j]&0xf]; }
        out[36] = '\0';
    }
};

struct UuidHasher {
    __attribute__((always_inline))
    std::size_t operator()(const Uuid& u) const noexcept {
        std::uint64_t a, b;
        std::memcpy(&a, u.bytes,     8);
        std::memcpy(&b, u.bytes + 8, 8);
        a ^= a >> 33;
        a *= 0xff51afd7ed558ccdULL;
        a ^= a >> 33;
        a *= 0xc4ceb9fe1a85ec53ULL;
        a ^= a >> 33;
        return a ^ b;
    }
};

}
