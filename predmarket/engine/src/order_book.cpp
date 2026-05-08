#include "order_book.hpp"

#include <cstring>
#include <cassert>
#include <immintrin.h>
#include <x86intrin.h>

namespace predmarket {

namespace {

__attribute__((always_inline))
static std::uint64_t rdtsc_ns() noexcept {
    std::uint64_t tsc = __rdtsc();
    return tsc;
}

__attribute__((always_inline))
static Uuid gen_fill_id() noexcept {
    Uuid u;
    std::uint64_t a = __rdtsc();
    std::uint64_t b = __rdtsc() ^ (a << 17);
    a = (a & 0xffffffffffff0fffULL) | 0x0000000000004000ULL;
    b = (b & 0x3fffffffffffffffULL) | 0x8000000000000000ULL;
    std::memcpy(u.bytes,     &a, 8);
    std::memcpy(u.bytes + 8, &b, 8);
    return u;
}

}

OrderBook::OrderBook(Uuid market_id, FillCallback on_fill) noexcept
    : m_market_id(market_id)
    , m_on_fill(std::move(on_fill))
    , m_sequence(0)
    , m_fill_sequence(0)
    , m_best_bid(0)
    , m_best_ask(10001)
    , m_order_free_top(kMaxOrdersPerBook)
    , m_level_free_top(kMaxPriceLevels)
    , m_bid_head(kInvalidSlot)
    , m_ask_head(kInvalidSlot)
{
    for (std::uint32_t i = 0; i < kMaxOrdersPerBook; ++i) {
        m_order_free[i] = kMaxOrdersPerBook - 1 - i;
    }
    for (std::uint32_t i = 0; i < kMaxPriceLevels; ++i) {
        m_level_free[i] = kMaxPriceLevels - 1 - i;
    }
    for (std::uint32_t i = 0; i < kMaxOrdersPerBook; ++i) {
        m_index[i].slot       = kInvalidSlot;
        m_index[i].level_slot = kInvalidSlot;
    }
    std::memset(m_bid_price_map, 0xFF, sizeof(m_bid_price_map));
    std::memset(m_ask_price_map, 0xFF, sizeof(m_ask_price_map));
}

__attribute__((always_inline))
std::uint32_t OrderBook::alloc_order() noexcept {
    if ([[unlikely]] m_order_free_top == 0) return kInvalidSlot;
    return m_order_free[--m_order_free_top];
}

__attribute__((always_inline))
void OrderBook::free_order(std::uint32_t s) noexcept {
    m_order_free[m_order_free_top++] = s;
}

__attribute__((always_inline))
std::uint32_t OrderBook::find_or_create_level(std::uint32_t price, Side side) noexcept {
    auto& price_map = (side == Side::Buy) ? m_bid_price_map[price] : m_ask_price_map[price];

    if ([[likely]] price_map != kInvalidSlot) {
        return price_map;
    }

    if ([[unlikely]] m_level_free_top == 0) return kInvalidSlot;
    std::uint32_t ls = m_level_free[--m_level_free_top];

    LevelSlot& lv = m_levels[ls];
    lv.total_qty  = 0;
    lv.head       = kInvalidSlot;
    lv.tail       = kInvalidSlot;
    lv.count      = 0;
    lv.price      = price;

    price_map = ls;

    auto& head = (side == Side::Buy) ? m_bid_head : m_ask_head;

    if (head == kInvalidSlot) {
        lv.next_level = kInvalidSlot;
        lv.prev_level = kInvalidSlot;
        head = ls;
        return ls;
    }

    std::uint32_t cur  = head;
    std::uint32_t prev = kInvalidSlot;

    const bool descending = (side == Side::Buy);

    while (cur != kInvalidSlot) {
        const std::uint32_t cur_price = m_levels[cur].price;
        const bool should_insert_before = descending ? (price > cur_price) : (price < cur_price);
        if (should_insert_before) break;
        prev = cur;
        cur  = m_levels[cur].next_level;
    }

    lv.next_level = cur;
    lv.prev_level = prev;

    if (prev != kInvalidSlot) {
        m_levels[prev].next_level = ls;
    } else {
        head = ls;
    }
    if (cur != kInvalidSlot) {
        m_levels[cur].prev_level = ls;
    }

    return ls;
}

__attribute__((always_inline))
void OrderBook::remove_level(std::uint32_t ls, Side side) noexcept {
    LevelSlot& lv = m_levels[ls];

    auto& head = (side == Side::Buy) ? m_bid_head : m_ask_head;
    auto& pmap = (side == Side::Buy) ? m_bid_price_map[lv.price] : m_ask_price_map[lv.price];

    if (lv.prev_level != kInvalidSlot) {
        m_levels[lv.prev_level].next_level = lv.next_level;
    } else {
        head = lv.next_level;
    }
    if (lv.next_level != kInvalidSlot) {
        m_levels[lv.next_level].prev_level = lv.prev_level;
    }

    pmap = kInvalidSlot;
    m_level_free[m_level_free_top++] = ls;

    if (side == Side::Buy) {
        m_best_bid = (head != kInvalidSlot) ? m_levels[head].price : 0;
    } else {
        m_best_ask = (head != kInvalidSlot) ? m_levels[head].price : 10001;
    }
}

__attribute__((always_inline))
void OrderBook::enqueue(std::uint32_t ls, std::uint32_t os) noexcept {
    LevelSlot& lv   = m_levels[ls];
    OrderSlot& ord  = m_orders[os];

    ord.next = kInvalidSlot;
    ord.prev = lv.tail;

    if (lv.tail != kInvalidSlot) {
        m_orders[lv.tail].next = os;
    } else {
        lv.head = os;
    }

    lv.tail       = os;
    lv.total_qty += ord.remaining;
    ++lv.count;
}

__attribute__((always_inline))
void OrderBook::dequeue(std::uint32_t ls, std::uint32_t os) noexcept {
    LevelSlot& lv  = m_levels[ls];
    OrderSlot& ord = m_orders[os];

    if (ord.prev != kInvalidSlot) {
        m_orders[ord.prev].next = ord.next;
    } else {
        lv.head = ord.next;
    }
    if (ord.next != kInvalidSlot) {
        m_orders[ord.next].prev = ord.prev;
    } else {
        lv.tail = ord.prev;
    }
    --lv.count;
}

__attribute__((always_inline))
Fill OrderBook::make_fill(
    std::uint32_t maker_slot,
    Uuid          taker_id,
    Uuid          taker_user,
    std::uint32_t price,
    std::uint64_t qty,
    Side          aggressor
) const noexcept {
    const OrderSlot& maker = m_orders[maker_slot];
    Fill f;
    f.fill_id        = gen_fill_id();
    f.market_id      = m_market_id;
    f.maker_order_id = maker.order_id;
    f.taker_order_id = taker_id;
    f.maker_user_id  = maker.user_id;
    f.taker_user_id  = taker_user;
    f.quantity       = qty;
    f.price          = price;
    f.aggressor_side = static_cast<std::uint8_t>(aggressor);
    f.sequence       = m_fill_sequence;
    f.timestamp_ns   = rdtsc_ns();
    return f;
}

__attribute__((always_inline))
void OrderBook::match(
    Uuid          order_id,
    Uuid          user_id,
    Side          aggressor,
    std::uint32_t limit_price,
    std::uint64_t& remaining,
    std::uint64_t  sequence,
    bool           is_market
) noexcept {
    auto& resting_head = (aggressor == Side::Buy) ? m_ask_head : m_bid_head;
    const Side resting_side = (aggressor == Side::Buy) ? Side::Sell : Side::Buy;

    while (remaining > 0 && resting_head != kInvalidSlot) {
        LevelSlot& level = m_levels[resting_head];

        if (!is_market) [[likely]] {
            const bool crossed = (aggressor == Side::Buy)
                ? (limit_price >= level.price)
                : (limit_price <= level.price);
            if (!crossed) break;
        }

        _mm_prefetch(reinterpret_cast<const char*>(&m_orders[level.head]), _MM_HINT_T0);

        while (remaining > 0 && level.head != kInvalidSlot) {
            std::uint32_t maker_slot = level.head;
            OrderSlot& maker = m_orders[maker_slot];

            if (level.head != level.tail) [[likely]] {
                _mm_prefetch(reinterpret_cast<const char*>(&m_orders[maker.next]), _MM_HINT_T0);
            }

            const std::uint64_t fill_qty = remaining < maker.remaining ? remaining : maker.remaining;

            const_cast<OrderBook*>(this)->m_fill_sequence++;
            Fill fill = make_fill(maker_slot, order_id, user_id, level.price, fill_qty, aggressor);
            m_on_fill(fill);

            maker.remaining  -= fill_qty;
            remaining        -= fill_qty;
            level.total_qty  -= fill_qty;

            const bool maker_done = (maker.remaining == 0);

            if (maker_done) {
                const std::uint32_t uid_hash = UuidHasher{}(maker.order_id) & (kMaxOrdersPerBook - 1);
                m_index[uid_hash].slot = kInvalidSlot;

                dequeue(resting_head, maker_slot);
                free_order(maker_slot);
            }
        }

        if (level.head == kInvalidSlot) {
            remove_level(resting_head, resting_side);
        }
    }
}

void OrderBook::add_limit_order(
    Uuid          order_id,
    Uuid          user_id,
    Side          side,
    std::uint32_t price,
    std::uint64_t quantity,
    std::uint64_t sequence
) noexcept {
    if ([[unlikely]] price == 0 || price >= 10000 || quantity == 0) return;

    m_sequence = sequence;

    std::uint64_t remaining = quantity;
    match(order_id, user_id, side, price, remaining, sequence, false);

    if (remaining == 0) return;

    std::uint32_t os = alloc_order();
    if ([[unlikely]] os == kInvalidSlot) return;

    OrderSlot& ord = m_orders[os];
    ord.order_id   = order_id;
    ord.user_id    = user_id;
    ord.quantity   = quantity;
    ord.remaining  = remaining;
    ord.sequence   = sequence;
    ord.price      = price;
    ord.side       = static_cast<std::uint8_t>(side);
    ord.is_market  = 0;

    std::uint32_t ls = find_or_create_level(price, side);
    if ([[unlikely]] ls == kInvalidSlot) { free_order(os); return; }

    enqueue(ls, os);

    const std::uint32_t idx = UuidHasher{}(order_id) & (kMaxOrdersPerBook - 1);
    m_index[idx].slot       = os;
    m_index[idx].level_slot = ls;
    m_index[idx].side       = static_cast<std::uint8_t>(side);

    if (side == Side::Buy) {
        m_best_bid = (price > m_best_bid) ? price : m_best_bid;
    } else {
        m_best_ask = (price < m_best_ask) ? price : m_best_ask;
    }
}

void OrderBook::add_market_order(
    Uuid          order_id,
    Uuid          user_id,
    Side          side,
    std::uint64_t quantity,
    std::uint64_t sequence
) noexcept {
    if ([[unlikely]] quantity == 0) return;
    m_sequence = sequence;
    std::uint64_t remaining = quantity;
    match(order_id, user_id, side, 0, remaining, sequence, true);
}

bool OrderBook::cancel_order(Uuid order_id) noexcept {
    const std::uint32_t idx = UuidHasher{}(order_id) & (kMaxOrdersPerBook - 1);
    const IndexEntry& ie = m_index[idx];

    if ([[unlikely]] ie.slot == kInvalidSlot) return false;

    const std::uint32_t os = ie.slot;
    const std::uint32_t ls = ie.level_slot;
    const Side          sd = static_cast<Side>(ie.side);

    OrderSlot& ord = m_orders[os];
    LevelSlot& lv  = m_levels[ls];

    lv.total_qty -= ord.remaining;
    dequeue(ls, os);

    const_cast<IndexEntry&>(ie).slot = kInvalidSlot;
    free_order(os);

    if (lv.head == kInvalidSlot) {
        remove_level(ls, sd);
    }

    return true;
}

std::size_t OrderBook::top_bids(PriceLevel* out, std::size_t max_depth) const noexcept {
    std::size_t   n   = 0;
    std::uint32_t cur = m_bid_head;
    while (cur != kInvalidSlot && n < max_depth) {
        const LevelSlot& lv = m_levels[cur];
        out[n++] = { lv.price, lv.total_qty, lv.count };
        cur = lv.next_level;
    }
    return n;
}

std::size_t OrderBook::top_asks(PriceLevel* out, std::size_t max_depth) const noexcept {
    std::size_t   n   = 0;
    std::uint32_t cur = m_ask_head;
    while (cur != kInvalidSlot && n < max_depth) {
        const LevelSlot& lv = m_levels[cur];
        out[n++] = { lv.price, lv.total_qty, lv.count };
        cur = lv.next_level;
    }
    return n;
}

}
