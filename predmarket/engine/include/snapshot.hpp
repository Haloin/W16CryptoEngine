#pragma once

#include "order_book.hpp"

#include <cstdint>
#include <string>
#include <vector>
#include <filesystem>

namespace predmarket {

struct SnapshotOrder {
    std::string order_id;
    std::string user_id;
    uint32_t    price;
    uint64_t    remaining;
    uint64_t    sequence;
    bool        is_market;
    Side        side;
};

struct SnapshotLevel {
    uint32_t price;
    std::vector<SnapshotOrder> orders;
};

struct SnapshotBook {
    std::string market_id;
    uint64_t    sequence;
    std::vector<SnapshotLevel> bids;
    std::vector<SnapshotLevel> asks;
};

struct SnapshotHeader {
    uint64_t magic;
    uint64_t version;
    uint64_t timestamp_ns;
    uint64_t sequence;
    uint64_t book_count;
};

class SnapshotManager {
public:
    explicit SnapshotManager(const std::filesystem::path& dir);

    SnapshotManager(const SnapshotManager&)            = delete;
    SnapshotManager& operator=(const SnapshotManager&) = delete;

    bool save(const std::unordered_map<std::string, OrderBook>& books);
    bool load(std::unordered_map<std::string, OrderBook>& books,
              const FillCallback& on_fill);

    uint64_t last_sequence() const { return last_sequence_; }

    bool truncate_wal(const std::filesystem::path& wal_path);

private:
    std::filesystem::path dir_;
    uint64_t              last_sequence_;

    static constexpr uint64_t MAGIC   = 0x50524D4B54000001;
    static constexpr uint64_t VERSION = 1;
};

}
