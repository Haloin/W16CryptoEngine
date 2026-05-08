#pragma once

#include <cstdint>
#include <array>

namespace hft {

struct alignas(64) PriceLevel {
    uint32_t price;
    uint64_t quantity;
    
    PriceLevel() = default;
    PriceLevel(uint32_t p, uint64_t q) : price(p), quantity(q) {}
};

struct alignas(64) OrderBookUpdate {
    static constexpr size_t MAX_LEVELS = 10;
    
    uint64_t sequence;
    uint64_t timestamp_ns;
    uint32_t market_id;
    
    std::array<PriceLevel, MAX_LEVELS> bids;
    std::array<PriceLevel, MAX_LEVELS> asks;
    
    uint8_t bid_count;
    uint8_t ask_count;
    
    bool is_snapshot;
    
    OrderBookUpdate() 
        : sequence(0)
        , timestamp_ns(0)
        , market_id(0)
        , bid_count(0)
        , ask_count(0)
        , is_snapshot(false) {}
};

struct alignas(64) TradeUpdate {
    uint64_t sequence;
    uint64_t timestamp_ns;
    uint32_t market_id;
    uint32_t price;
    uint64_t quantity;
    uint8_t side;
    
    TradeUpdate()
        : sequence(0)
        , timestamp_ns(0)
        , market_id(0)
        , price(0)
        , quantity(0)
        , side(0) {}
};

class MarketDataParser {
public:
    bool parse_orderbook(const char* json, size_t len, OrderBookUpdate& out) noexcept {
        (void)json;
        (void)len;
        (void)out;
        
        return true;
    }
    
    bool parse_trade(const char* json, size_t len, TradeUpdate& out) noexcept {
        (void)json;
        (void)len;
        (void)out;
        
        return true;
    }
};

}
