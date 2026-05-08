#pragma once

#include "hft_core.hpp"
#include "spsc_queue.hpp"
#include "market_data_parser.hpp"
#include <cstdint>
#include <atomic>

namespace hft {

struct alignas(64) Signal {
    uint32_t market_id;
    int32_t direction;
    uint32_t target_price;
    uint32_t stop_price;
    uint64_t timestamp_ns;
    uint8_t confidence;
    
    Signal()
        : market_id(0)
        , direction(0)
        , target_price(0)
        , stop_price(0)
        , timestamp_ns(0)
        , confidence(0) {}
};

struct alignas(64) StrategyState {
    std::atomic<uint64_t> last_price{0};
    std::atomic<uint64_t> volume_24h{0};
    std::atomic<int64_t> position{0};
    
    char padding[64 - 24];
};

class StrategyEngine {
    static constexpr size_t MAX_MARKETS = 256;
    
    alignas(64) CacheAlignedArray<StrategyState, MAX_MARKETS> states_;
    
    SPSCQueue<OrderBookUpdate, 1024>* md_queue_;
    SPSCQueue<Signal, 1024>* signal_queue_;
    
    uint64_t latency_threshold_ns_;
    
public:
    StrategyEngine(
        SPSCQueue<OrderBookUpdate, 1024>* md_queue,
        SPSCQueue<Signal, 1024>* signal_queue,
        uint64_t latency_threshold_ns = 500000
    ) : md_queue_(md_queue)
      , signal_queue_(signal_queue)
      , latency_threshold_ns_(latency_threshold_ns) {
    }

    void run() noexcept {
        ThreadConfig::configure(ThreadConfig::Role::Strategy);
        
        OrderBookUpdate update;
        
        while (__builtin_expect(true, 1)) {
            if (__builtin_expect(md_queue_->pop(update), 0)) {
                process_update(update);
            }
            
            __builtin_ia32_pause();
        }
    }

private:
    __attribute__((always_inline))
    void process_update(const OrderBookUpdate& update) noexcept {
        const uint64_t receive_time = rdtsc();
        
        if (__builtin_expect(update.timestamp_ns + latency_threshold_ns_ < receive_time, 0)) {
            return;
        }
        
        if (__builtin_expect(update.market_id >= MAX_MARKETS, 0)) {
            return;
        }
        
        auto& state = states_[update.market_id];
        
        if (__builtin_expect(update.bid_count > 0 && update.ask_count > 0, 1)) {
            const uint32_t best_bid = update.bids[0].price;
            const uint32_t best_ask = update.asks[0].price;
            const uint32_t mid_price = (best_bid + best_ask) >> 1;
            
            state.last_price.store(mid_price, std::memory_order_relaxed);
            
            Signal signal = generate_signal(update, state, mid_price);
            
            if (__builtin_expect(signal.direction != 0, 0)) {
                signal_queue_->push(signal);
            }
        }
    }

    __attribute__((always_inline))
    Signal generate_signal(
        const OrderBookUpdate& update,
        StrategyState& state,
        uint32_t mid_price
    ) noexcept {
        Signal signal;
        
        const int64_t current_pos = state.position.load(std::memory_order_relaxed);
        
        const uint64_t bid_depth = calculate_depth(update.bids, update.bid_count);
        const uint64_t ask_depth = calculate_depth(update.asks, update.ask_count);
        
        if (__builtin_expect(bid_depth > ask_depth * 2 && current_pos <= 0, 0)) {
            signal.direction = 1;
            signal.target_price = update.asks[0].price;
            signal.stop_price = static_cast<uint32_t>(mid_price * 0.995);
            signal.confidence = calculate_confidence(bid_depth, ask_depth);
        } else if (__builtin_expect(ask_depth > bid_depth * 2 && current_pos >= 0, 0)) {
            signal.direction = -1;
            signal.target_price = update.bids[0].price;
            signal.stop_price = static_cast<uint32_t>(mid_price * 1.005);
            signal.confidence = calculate_confidence(ask_depth, bid_depth);
        }
        
        signal.market_id = update.market_id;
        signal.timestamp_ns = rdtsc();
        
        return signal;
    }

    __attribute__((always_inline))
    uint64_t calculate_depth(const std::array<PriceLevel, 10>& levels, uint8_t count) noexcept {
        uint64_t total = 0;
        
        for (uint8_t i = 0; __builtin_expect(i < count && i < 5, 1); ++i) {
            total += levels[i].quantity;
        }
        
        return total;
    }

    __attribute__((always_inline))
    uint8_t calculate_confidence(uint64_t dominant_depth, uint64_t other_depth) noexcept {
        if (__builtin_expect(other_depth == 0, 0)) {
            return 100;
        }
        
        double ratio = static_cast<double>(dominant_depth) / static_cast<double>(other_depth);
        ratio = ratio > 10.0 ? 10.0 : ratio;
        
        return static_cast<uint8_t>((ratio - 1.0) * 11.11);
    }
};

}
