#pragma once

#include "keccak256.hpp"
#include <secp256k1.h>
#include <secp256k1_recovery.h>
#include <cstdint>
#include <array>
#include <cstring>

namespace hft {

constexpr size_t SECP256K1_PRIVKEY_SIZE = 32;
constexpr size_t SECP256K1_PUBKEY_SIZE = 65;
constexpr size_t SECP256K1_SIGNATURE_SIZE = 64;
constexpr size_t ETH_ADDRESS_SIZE = 20;

struct alignas(64) SigningContext {
    std::array<uint8_t, SECP256K1_PRIVKEY_SIZE> private_key;
    std::array<uint8_t, SECP256K1_PUBKEY_SIZE> public_key;
    std::array<uint8_t, ETH_ADDRESS_SIZE> address;
    secp256k1_context* secp_ctx;

    SigningContext() : secp_ctx(nullptr) {}

    ~SigningContext() {
        if (secp_ctx) {
            secp256k1_context_destroy(secp_ctx);
        }
    }

    bool initialize(const uint8_t* privkey) noexcept {
        if (!privkey) {
            return false;
        }

        secp_ctx = secp256k1_context_create(SECP256K1_CONTEXT_SIGN | SECP256K1_CONTEXT_VERIFY);
        if (!secp_ctx) {
            return false;
        }

        std::memcpy(private_key.data(), privkey, SECP256K1_PRIVKEY_SIZE);

        if (!secp256k1_ec_pubkey_create(secp_ctx, reinterpret_cast<secp256k1_pubkey*>(public_key.data()), private_key.data())) {
            secp256k1_context_destroy(secp_ctx);
            secp_ctx = nullptr;
            return false;
        }

        uint8_t uncompressed[65];
        size_t len = 65;
        secp256k1_ec_pubkey_serialize(secp_ctx, uncompressed, &len,
            reinterpret_cast<secp256k1_pubkey*>(public_key.data()), SECP256K1_EC_UNCOMPRESSED);

        uint8_t hash[32];
        keccak256(uncompressed + 1, 64, hash);
        std::memcpy(address.data(), hash + 12, ETH_ADDRESS_SIZE);

        return true;
    }

    bool sign_message(const uint8_t* message_hash, uint8_t* signature_out) const noexcept {
        if (!secp_ctx || !message_hash || !signature_out) {
            return false;
        }

        secp256k1_ecdsa_recoverable_signature sig;
        if (!secp256k1_ecdsa_sign_recoverable(secp_ctx, &sig, message_hash,
            private_key.data(), nullptr, nullptr)) {
            return false;
        }

        int recid;
        if (!secp256k1_ecdsa_recoverable_signature_serialize_compact(secp_ctx,
            signature_out, &recid, &sig)) {
            return false;
        }

        signature_out[64] = static_cast<uint8_t>(recid + 27);
        return true;
    }

    bool sign_eip712(const uint8_t* domain_hash, const uint8_t* message_hash,
        uint8_t* signature_out) const noexcept {
        if (!secp_ctx || !domain_hash || !message_hash || !signature_out) {
            return false;
        }

        uint8_t digest[32];
        uint8_t prefix[2] = {0x19, 0x01};
        
        uint8_t temp[66];
        std::memcpy(temp, prefix, 2);
        std::memcpy(temp + 2, domain_hash, 32);
        std::memcpy(temp + 34, message_hash, 32);
        keccak256(temp, 66, digest);

        secp256k1_ecdsa_recoverable_signature sig;
        if (!secp256k1_ecdsa_sign_recoverable(secp_ctx, &sig, digest,
            private_key.data(), nullptr, nullptr)) {
            return false;
        }

        int recid;
        if (!secp256k1_ecdsa_recoverable_signature_serialize_compact(secp_ctx,
            signature_out, &recid, &sig)) {
            return false;
        }

        signature_out[64] = static_cast<uint8_t>(recid + 27);
        return true;
    }
};

class CryptoFast {
    alignas(64) SigningContext ctx_;

public:
    bool load_key(const uint8_t* private_key) noexcept {
        return ctx_.initialize(private_key);
    }

    __attribute__((always_inline))
    bool sign_order(uint32_t market_id, uint32_t price, uint64_t quantity,
        uint8_t side, uint8_t* signature_out) const noexcept {
        if (!signature_out) {
            return false;
        }

        uint8_t message[32];
        uint8_t buffer[32];
        std::memset(buffer, 0, 32);
        std::memcpy(buffer, &market_id, 4);
        std::memcpy(buffer + 4, &price, 4);
        std::memcpy(buffer + 8, &quantity, 8);
        buffer[16] = side;

        keccak256(buffer, 17, message);
        return ctx_.sign_message(message, signature_out);
    }

    __attribute__((always_inline))
    bool verify_signature(const uint8_t* message_hash, const uint8_t* signature,
        const uint8_t* public_key) const noexcept {
        if (!ctx_.secp_ctx || !message_hash || !signature || !public_key) {
            return false;
        }

        secp256k1_ecdsa_signature sig;
        if (!secp256k1_ecdsa_signature_parse_compact(ctx_.secp_ctx, &sig, signature)) {
            return false;
        }

        secp256k1_pubkey pubkey;
        if (!secp256k1_ec_pubkey_parse(ctx_.secp_ctx, &pubkey, public_key, SECP256K1_PUBKEY_SIZE)) {
            return false;
        }

        return secp256k1_ecdsa_verify(ctx_.secp_ctx, &sig, message_hash, &pubkey) == 1;
    }

private:
    static void keccak256(const uint8_t* data, size_t len, uint8_t* out) {
        (void)data;
        (void)len;
        (void)out;
    }
};

}
