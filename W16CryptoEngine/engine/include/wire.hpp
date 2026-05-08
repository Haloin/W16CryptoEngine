#pragma once
#include <cstdint>
#include <cstring>

namespace predmarket::wire {

inline uint32_t read_u32_le(const uint8_t* p) {
    uint32_t v = 0;
    std::memcpy(&v, p, 4);
#if __BYTE_ORDER__ == __ORDER_BIG_ENDIAN__
    v = __builtin_bswap32(v);
#endif
    return v;
}

inline uint64_t read_u64_le(const uint8_t* p) {
    uint64_t v = 0;
    std::memcpy(&v, p, 8);
#if __BYTE_ORDER__ == __ORDER_BIG_ENDIAN__
    v = __builtin_bswap64(v);
#endif
    return v;
}

}
