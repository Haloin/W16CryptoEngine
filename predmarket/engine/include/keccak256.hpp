#pragma once

#include <cstdint>
#include <cstring>

namespace hft {

class Keccak256 {
public:
    static void hash(const uint8_t* input, size_t len, uint8_t output[32]) {
        uint64_t st[25];
        std::memset(st, 0, sizeof(st));
        
        const size_t rate = 136;
        while (len >= rate) {
            for (size_t i = 0; i < rate / 8; ++i) {
                st[i] ^= load64(input + i * 8);
            }
            keccakf(st);
            input += rate;
            len -= rate;
        }
        
        uint8_t temp[144];
        std::memset(temp, 0, sizeof(temp));
        std::memcpy(temp, input, len);
        temp[len] = 0x01;
        temp[rate - 1] |= 0x80;
        
        for (size_t i = 0; i < rate / 8; ++i) {
            st[i] ^= load64(temp + i * 8);
        }
        
        keccakf(st);
        
        for (int i = 0; i < 4; ++i) {
            store64(output + i * 8, st[i]);
        }
    }

private:
    static uint64_t load64(const uint8_t* x) {
        uint64_t r = 0;
        for (int i = 0; i < 8; ++i) {
            r |= static_cast<uint64_t>(x[i]) << (i * 8);
        }
        return r;
    }
    
    static void store64(uint8_t* x, uint64_t u) {
        for (int i = 0; i < 8; ++i) {
            x[i] = static_cast<uint8_t>(u >> (i * 8));
        }
    }
    
    static void keccakf(uint64_t st[25]) {
        static const uint64_t keccakf_rndc[24] = {
            0x0000000000000001ULL, 0x0000000000008082ULL, 0x800000000000808aULL,
            0x8000000080008000ULL, 0x000000000000808bULL, 0x0000000080000001ULL,
            0x8000000080008081ULL, 0x8000000000008009ULL, 0x000000000000008aULL,
            0x0000000000000088ULL, 0x0000000080008009ULL, 0x000000008000000aULL,
            0x000000008000808bULL, 0x800000000000008bULL, 0x8000000000008089ULL,
            0x8000000000008003ULL, 0x8000000000008002ULL, 0x8000000000000080ULL,
            0x000000000000800aULL, 0x800000008000000aULL, 0x8000000080008081ULL,
            0x8000000000008080ULL, 0x0000000080000001ULL, 0x8000000080008008ULL
        };
        
        for (int round = 0; round < 24; ++round) {
            uint64_t t[5], bc[5];
            
            bc[0] = st[0] ^ st[5] ^ st[10] ^ st[15] ^ st[20];
            bc[1] = st[1] ^ st[6] ^ st[11] ^ st[16] ^ st[21];
            bc[2] = st[2] ^ st[7] ^ st[12] ^ st[17] ^ st[22];
            bc[3] = st[3] ^ st[8] ^ st[13] ^ st[18] ^ st[23];
            bc[4] = st[4] ^ st[9] ^ st[14] ^ st[19] ^ st[24];
            
            t[0] = bc[4] ^ ((bc[1] << 1) | (bc[1] >> 63));
            t[1] = bc[0] ^ ((bc[2] << 1) | (bc[2] >> 63));
            t[2] = bc[1] ^ ((bc[3] << 1) | (bc[3] >> 63));
            t[3] = bc[2] ^ ((bc[4] << 1) | (bc[4] >> 63));
            t[4] = bc[3] ^ ((bc[0] << 1) | (bc[0] >> 63));
            
            st[0] ^= t[0]; st[5] ^= t[0]; st[10] ^= t[0]; st[15] ^= t[0]; st[20] ^= t[0];
            st[1] ^= t[1]; st[6] ^= t[1]; st[11] ^= t[1]; st[16] ^= t[1]; st[21] ^= t[1];
            st[2] ^= t[2]; st[7] ^= t[2]; st[12] ^= t[2]; st[17] ^= t[2]; st[22] ^= t[2];
            st[3] ^= t[3]; st[8] ^= t[3]; st[13] ^= t[3]; st[18] ^= t[3]; st[23] ^= t[3];
            st[4] ^= t[4]; st[9] ^= t[4]; st[14] ^= t[4]; st[19] ^= t[4]; st[24] ^= t[4];
            
            t[0] = st[1];
            st[1] = (st[6] << 44) | (st[6] >> (64 - 44));
            st[6] = (st[9] << 20) | (st[9] >> (64 - 20));
            st[9] = (st[22] << 61) | (st[22] >> (64 - 61));
            st[22] = (st[14] << 39) | (st[14] >> (64 - 39));
            st[14] = (st[20] << 18) | (st[20] >> (64 - 18));
            st[20] = (st[2] << 62) | (st[2] >> (64 - 62));
            st[2] = (st[12] << 43) | (st[12] >> (64 - 43));
            st[12] = (st[13] << 25) | (st[13] >> (64 - 25));
            st[13] = (st[19] << 8) | (st[19] >> (64 - 8));
            st[19] = (st[23] << 56) | (st[23] >> (64 - 56));
            st[23] = (st[15] << 41) | (st[15] >> (64 - 41));
            st[15] = (st[4] << 27) | (st[4] >> (64 - 27));
            st[4] = (st[24] << 14) | (st[24] >> (64 - 14));
            st[24] = (st[21] << 2) | (st[21] >> (64 - 2));
            st[21] = (st[8] << 55) | (st[8] >> (64 - 55));
            st[8] = (st[16] << 45) | (st[16] >> (64 - 45));
            st[16] = (st[5] << 36) | (st[5] >> (64 - 36));
            st[5] = (st[3] << 28) | (st[3] >> (64 - 28));
            st[3] = (st[18] << 21) | (st[18] >> (64 - 21));
            st[18] = (st[17] << 15) | (st[17] >> (64 - 15));
            st[17] = (st[11] << 10) | (st[11] >> (64 - 10));
            st[11] = (st[7] << 6) | (st[7] >> (64 - 6));
            st[7] = (st[10] << 3) | (st[10] >> (64 - 3));
            st[10] = t[0];
            
            bc[0] = st[0]; bc[1] = st[1]; bc[2] = st[2]; bc[3] = st[3]; bc[4] = st[4];
            
            st[0] ^= (~bc[1]) & bc[2];
            st[1] ^= (~bc[2]) & bc[3];
            st[2] ^= (~bc[3]) & bc[4];
            st[3] ^= (~bc[4]) & bc[0];
            st[4] ^= (~bc[0]) & bc[1];
            
            bc[0] = st[5]; bc[1] = st[6]; bc[2] = st[7]; bc[3] = st[8]; bc[4] = st[9];
            
            st[5] ^= (~bc[1]) & bc[2];
            st[6] ^= (~bc[2]) & bc[3];
            st[7] ^= (~bc[3]) & bc[4];
            st[8] ^= (~bc[4]) & bc[0];
            st[9] ^= (~bc[0]) & bc[1];
            
            bc[0] = st[10]; bc[1] = st[11]; bc[2] = st[12]; bc[3] = st[13]; bc[4] = st[14];
            
            st[10] ^= (~bc[1]) & bc[2];
            st[11] ^= (~bc[2]) & bc[3];
            st[12] ^= (~bc[3]) & bc[4];
            st[13] ^= (~bc[4]) & bc[0];
            st[14] ^= (~bc[0]) & bc[1];
            
            bc[0] = st[15]; bc[1] = st[16]; bc[2] = st[17]; bc[3] = st[18]; bc[4] = st[19];
            
            st[15] ^= (~bc[1]) & bc[2];
            st[16] ^= (~bc[2]) & bc[3];
            st[17] ^= (~bc[3]) & bc[4];
            st[18] ^= (~bc[4]) & bc[0];
            st[19] ^= (~bc[0]) & bc[1];
            
            bc[0] = st[20]; bc[1] = st[21]; bc[2] = st[22]; bc[3] = st[23]; bc[4] = st[24];
            
            st[20] ^= (~bc[1]) & bc[2];
            st[21] ^= (~bc[2]) & bc[3];
            st[22] ^= (~bc[3]) & bc[4];
            st[23] ^= (~bc[4]) & bc[0];
            st[24] ^= (~bc[0]) & bc[1];
            
            st[0] ^= keccakf_rndc[round];
        }
    }
};

inline void keccak256(const uint8_t* input, size_t len, uint8_t output[32]) {
    Keccak256::hash(input, len, output);
}

}
