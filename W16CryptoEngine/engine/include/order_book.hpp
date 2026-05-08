#pragma once

#include "uuid.hpp"
#include "pool.hpp"

#include <cstdint>
#include <cstddef>
#include <cstring>
#include <functional>
#include <immintrin.h>

namespace predmarket {

static constexpr std::uint32_t kMaxPriceLevels   = 16384;
static constexpr std::uint32_t kMaxOrdersPerBook = 131072;
static constexpr std::uint32_t kInvalidSlot      = 0xFFFFFFFFu;

enum class Side : std::uint8_t { Buy = 0, Sell = 1 };

struct alignas(64) OrderSlot {
    Uuid          order_id;
    Uuid          user_id;
    std::uint64_t quantity;
    std::uint64_t remaining;
    std::uint64_t sequence;
    std::uint32_t price;
    std::uint32_t next;
    std::uint32_t prev;
    std::uint8_t  side;
    std::uint8_t  is_market;
    std::uint8_t  _pad[6];
};
static_assert(sizeof(OrderSlot) == 128);

struct alignas(64) LevelSlot {
    std::uint64_t total_qty;
    std::uint32_t head;
    std::uint32_t tail;
    std::uint32_t count;
    std::uint32_t price;
    std::uint32_t next_level;
    std::uint32_t prev_level;
    std::uint8_t  _pad[32];
};
static_assert(sizeof(LevelSlot) == 64);

struct alignas(16) IndexEntry {
    std::uint32_t slot;
    std::uint32_t level_slot;
    std::uint8_t  side;
    std::uint8_t  _pad[7];
};
static_assert(sizeof(IndexEntry) == 16);

struct Fill {
    Uuid          fill_id;
    Uuid          market_id;
    Uuid          maker_order_id;
    Uuid          taker_order_id;
    Uuid          maker_user_id;
    Uuid          taker_user_id;
    std::uint64_t quantity;
    std::uint64_t sequence;
    std::uint64_t timestamp_ns;
    std::uint32_t price;
    std::uint8_t  aggressor_side;
    std::uint8_t  _pad[3];
};

struct PriceLevel {
    std::uint32_t price;
    std::uint64_t quantity;
    std::uint32_t count;
};

using FillCallback = std::function<void(const Fill&)>;

class alignas(64) OrderBook {
public:
    explicit OrderBook(Uuid market_id, FillCallback on_fill) noexcept;

    OrderBook(const OrderBook&)            = delete;
    OrderBook& operator=(const OrderBook&) = delete;
    OrderBook(OrderBook&&)                 = delete;
    OrderBook& operator=(OrderBook&&)      = delete;

    __attribute__((always_inline))
    void add_limit_order(
        Uuid          order_id,
        Uuid          user_id,
        Side          side,
        std::uint32_t price,
        std::uint64_t quantity,
        std::uint64_t sequence
    ) noexcept;

    __attribute__((always_inline))
    void add_market_order(
        Uuid          order_id,
        Uuid          user_id,
        Side          side,
        std::uint64_t quantity,
        std::uint64_t sequence
    ) noexcept;

    bool cancel_order(Uuid order_id) noexcept;

    std::uint32_t best_bid() const noexcept { return m_best_bid; }
    std::uint32_t best_ask() const noexcept { return m_best_ask; }
    std::uint64_t sequence() const noexcept { return m_sequence; }
    Uuid          market_id() const noexcept { return m_market_id; }

    std::size_t top_bids(PriceLevel* out, std::size_t max_depth) const noexcept;
    std::size_t top_asks(PriceLevel* out, std::size_t max_depth) const noexcept;

private:
    __attribute__((always_inline)) std::uint32_t alloc_order() noexcept;
    __attribute__((always_inline)) void          free_order(std::uint32_t s) noexcept;
    __attribute__((always_inline)) std::uint32_t find_or_create_level(std::uint32_t price, Side side) noexcept;
    __attribute__((always_inline)) void          remove_level(std::uint32_t ls, Side side) noexcept;
    __attribute__((always_inline)) void          enqueue(std::uint32_t ls, std::uint32_t os) noexcept;
    __attribute__((always_inline)) void          dequeue(std::uint32_t ls, std::uint32_t os) noexcept;

    __attribute__((always_inline))
    void match(
        Uuid          order_id,
        Uuid          user_id,
        Side          aggressor,
        std::uint32_t limit_price,
        std::uint64_t& remaining,
        std::uint64_t  sequence,
        bool           is_market
    ) noexcept;

    __attribute__((always_inline))
    Fill make_fill(
        std::uint32_t maker_slot,
        Uuid          taker_id,
        Uuid          taker_user,
        std::uint32_t price,
        std::uint64_t qty,
        Side          aggressor
    ) const noexcept;

    alignas(64) OrderSlot     m_orders[kMaxOrdersPerBook];
    alignas(64) LevelSlot     m_levels[kMaxPriceLevels];
    alignas(64) std::uint32_t m_order_free[kMaxOrdersPerBook];
    alignas(64) std::uint32_t m_level_free[kMaxPriceLevels];
    alignas(64) IndexEntry    m_index[kMaxOrdersPerBook];
    alignas(64) std::uint32_t m_bid_price_map[10001];
    alignas(64) std::uint32_t m_ask_price_map[10001];

    Uuid          m_market_id;
    FillCallback  m_on_fill;
    std::uint64_t m_sequence;
    std::uint64_t m_fill_sequence;
    std::uint32_t m_best_bid;
    std::uint32_t m_best_ask;
    std::uint32_t m_order_free_top;
    std::uint32_t m_level_free_top;
    std::uint32_t m_bid_head;
    std::uint32_t m_ask_head;
};

}
