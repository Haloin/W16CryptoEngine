#include "wal.hpp"

#include <stdexcept>
#include <cstring>

namespace predmarket {

Wal::Wal(const std::filesystem::path& path)
    : path_(path)
    , last_sequence_(0)
    , next_sequence_(1)
{
    std::filesystem::create_directories(path.parent_path());
    file_.open(path, std::ios::binary | std::ios::app);
    if (!file_.is_open()) {
        throw std::runtime_error("failed to open WAL: " + path.string());
    }
}

Wal::~Wal() {
    if (file_.is_open()) {
        file_.flush();
        file_.close();
    }
}

void Wal::append(WalOpType op, const uint8_t* data, uint32_t len) {
    write_u64(next_sequence_);
    write_u8(static_cast<uint8_t>(op));
    write_u32(len);
    file_.write(reinterpret_cast<const char*>(data), len);
    last_sequence_ = next_sequence_++;
}

void Wal::fsync() {
    file_.flush();
#ifdef __linux__
    ::fdatasync(static_cast<int>(
        reinterpret_cast<std::uintptr_t>(
            reinterpret_cast<void*>(file_.rdbuf()))));
#endif
}

std::vector<WalRecord> Wal::recover() {
    std::ifstream in(path_, std::ios::binary);
    if (!in.is_open()) {
        return {};
    }

    std::vector<WalRecord> records;

    while (in.good()) {
        WalRecord rec;
        if (!in.read(reinterpret_cast<char*>(&rec.sequence), 8)) break;
        if (!in.read(reinterpret_cast<char*>(&rec.op_type), 1))  break;

        uint32_t len = 0;
        if (!in.read(reinterpret_cast<char*>(&len), 4)) break;

        rec.payload_len = len;
        rec.payload.resize(len);
        if (!in.read(reinterpret_cast<char*>(rec.payload.data()), len)) break;

        last_sequence_ = std::max(last_sequence_, rec.sequence);
        records.push_back(std::move(rec));
    }

    next_sequence_ = last_sequence_ + 1;
    return records;
}

void Wal::write_u64(uint64_t v) {
    file_.write(reinterpret_cast<const char*>(&v), 8);
}

void Wal::write_u32(uint32_t v) {
    file_.write(reinterpret_cast<const char*>(&v), 4);
}

void Wal::write_u8(uint8_t v) {
    file_.write(reinterpret_cast<const char*>(&v), 1);
}

}
