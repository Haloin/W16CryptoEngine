#include "snapshot.hpp"
#include "wire.hpp"

#include <fstream>
#include <cstring>

namespace predmarket {

SnapshotManager::SnapshotManager(const std::filesystem::path& dir)
    : dir_(dir)
    , last_sequence_(0)
{
    std::filesystem::create_directories(dir_);
}

bool SnapshotManager::save(const std::unordered_map<std::string, OrderBook>& books) {
    std::filesystem::path tmp_path = dir_ / "snapshot.tmp";
    std::filesystem::path final_path = dir_ / "snapshot.bin";

    std::ofstream out(tmp_path, std::ios::binary);
    if (!out.is_open()) {
        return false;
    }

    uint64_t max_sequence = 0;
    for (const auto& [market_id, book] : books) {
        max_sequence = std::max(max_sequence, book.sequence());
    }

    SnapshotHeader header{};
    header.magic        = MAGIC;
    header.version      = VERSION;
    header.timestamp_ns = 0;
    header.sequence     = max_sequence;
    header.book_count   = books.size();

    out.write(reinterpret_cast<const char*>(&header), sizeof(header));

    for (const auto& [market_id, book] : books) {
        uint64_t market_id_len = market_id.size();
        out.write(reinterpret_cast<const char*>(&market_id_len), 8);
        out.write(market_id.data(), market_id_len);

        uint64_t seq = book.sequence();
        out.write(reinterpret_cast<const char*>(&seq), 8);

        auto bids = book.bids(10000);
        auto asks = book.asks(10000);

        uint64_t bid_count = bids.size();
        uint64_t ask_count = asks.size();
        out.write(reinterpret_cast<const char*>(&bid_count), 8);
        out.write(reinterpret_cast<const char*>(&ask_count), 8);

        for (const auto& level : bids) {
            out.write(reinterpret_cast<const char*>(&level.price), 4);
            out.write(reinterpret_cast<const char*>(&level.count), 4);
        }

        for (const auto& level : asks) {
            out.write(reinterpret_cast<const char*>(&level.price), 4);
            out.write(reinterpret_cast<const char*>(&level.count), 4);
        }
    }

    out.flush();
    out.close();

    if (!out.good()) {
        std::filesystem::remove(tmp_path);
        return false;
    }

    std::filesystem::rename(tmp_path, final_path);
    last_sequence_ = max_sequence;

    return true;
}

bool SnapshotManager::load(std::unordered_map<std::string, OrderBook>& books,
                           const FillCallback& on_fill) {
    std::filesystem::path path = dir_ / "snapshot.bin";

    if (!std::filesystem::exists(path)) {
        return false;
    }

    std::ifstream in(path, std::ios::binary);
    if (!in.is_open()) {
        return false;
    }

    SnapshotHeader header{};
    if (!in.read(reinterpret_cast<char*>(&header), sizeof(header))) {
        return false;
    }

    if (header.magic != MAGIC || header.version != VERSION) {
        return false;
    }

    last_sequence_ = header.sequence;

    for (uint64_t i = 0; i < header.book_count; ++i) {
        uint64_t market_id_len = 0;
        if (!in.read(reinterpret_cast<char*>(&market_id_len), 8)) {
            return false;
        }

        std::string market_id(market_id_len, '\0');
        if (!in.read(market_id.data(), market_id_len)) {
            return false;
        }

        uint64_t seq = 0;
        in.read(reinterpret_cast<char*>(&seq), 8);

        uint64_t bid_count = 0, ask_count = 0;
        in.read(reinterpret_cast<char*>(&bid_count), 8);
        in.read(reinterpret_cast<char*>(&ask_count), 8);

        books.emplace(market_id, OrderBook(market_id, on_fill));
    }

    return true;
}

bool SnapshotManager::truncate_wal(const std::filesystem::path& wal_path) {
    if (last_sequence_ == 0) {
        return false;
    }

    std::filesystem::path backup_path = wal_path.string() + ".old";

    try {
        if (std::filesystem::exists(wal_path)) {
            std::filesystem::rename(wal_path, backup_path);

            std::ofstream new_wal(wal_path, std::ios::binary);
            if (!new_wal.is_open()) {
                std::filesystem::rename(backup_path, wal_path);
                return false;
            }
            new_wal.close();

            std::filesystem::remove(backup_path);
        }
        return true;
    } catch (...) {
        return false;
    }
}

}
