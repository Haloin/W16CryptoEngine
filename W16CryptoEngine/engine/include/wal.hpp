#pragma once

#include <cstdint>
#include <string>
#include <fstream>
#include <filesystem>
#include <vector>

namespace predmarket {

enum class WalOpType : uint8_t {
    AddOrder    = 1,
    CancelOrder = 2,
    Fill        = 3,
};

struct WalRecord {
    uint64_t   sequence;
    WalOpType  op_type;
    uint32_t   payload_len;
    std::vector<uint8_t> payload;
};

class Wal {
public:
    explicit Wal(const std::filesystem::path& path);

    ~Wal();

    Wal(const Wal&)            = delete;
    Wal& operator=(const Wal&) = delete;

    void append(WalOpType op, const uint8_t* data, uint32_t len);
    void fsync();

    std::vector<WalRecord> recover();

    uint64_t last_sequence() const { return last_sequence_; }

private:
    void write_u64(uint64_t v);
    void write_u32(uint32_t v);
    void write_u8(uint8_t v);

    std::ofstream     file_;
    std::filesystem::path path_;
    uint64_t          last_sequence_;
    uint64_t          next_sequence_;
};

}
