#pragma once

#include "hft_core.hpp"
#include "spsc_queue.hpp"
#include "market_data_parser.hpp"
#include "strategy_engine.hpp"
#include "order_gateway.hpp"
#include "crypto_fast.hpp"
#include <thread>
#include <atomic>

namespace hft {

class HFTEngine {
    alignas(64) SPSCQueue<OrderBookUpdate, 1024> md_queue_;
    alignas(64) SPSCQueue<Signal, 1024> signal_queue_;
    alignas(64) SPSCQueue<OrderRequest, 1024> order_queue_;
    
    StrategyEngine strategy_;
    OrderGateway gateway_;
    CryptoFast crypto_;
    
    std::thread md_thread_;
    std::thread strategy_thread_;
    std::thread gateway_thread_;
    
    std::atomic<bool> running_{false};
    
public:
    HFTEngine() 
        : strategy_(&md_queue_, &signal_queue_, 500000)
        , gateway_(&signal_queue_, &order_queue_) {
    }

    bool initialize(const uint8_t* private_key) noexcept {
        if (!crypto_.load_key(private_key)) {
            return false;
        }
        
        return true;
    }

    void start() noexcept {
        running_.store(true, std::memory_order_relaxed);
        
        strategy_thread_ = std::thread([this]() {
            strategy_.run();
        });
        
        gateway_thread_ = std::thread([this]() {
            gateway_.run();
        });
    }

    void stop() noexcept {
        running_.store(false, std::memory_order_relaxed);
        
        if (strategy_thread_.joinable()) {
            strategy_thread_.join();
        }
        
        if (gateway_thread_.joinable()) {
            gateway_thread_.join();
        }
    }

    __attribute__((always_inline))
    bool on_market_data(const char* json, size_t len) noexcept {
        OrderBookUpdate update;
        
        MarketDataParser parser;
        if (__builtin_expect(!parser.parse_orderbook(json, len, update), 0)) {
            return false;
        }
        
        return md_queue_.push(update);
    }

    __attribute__((always_inline))
    bool submit_order(OrderRequest& order) noexcept {
        order.timestamp_ns = rdtsc();
        return order_queue_.push(order);
    }

    const SPSCQueue<OrderRequest, 1024>& order_queue() const noexcept {
        return order_queue_;
    }

    bool is_running() const noexcept {
        return running_.load(std::memory_order_relaxed);
    }
};

}
