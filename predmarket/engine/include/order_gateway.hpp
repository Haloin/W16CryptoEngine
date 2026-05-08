#pragma once

#include "hft_core.hpp"
#include "spsc_queue.hpp"
#include "strategy_engine.hpp"
#include <cstdint>
#include <atomic>

namespace hft {

struct alignas(64) OrderRequest {
    uint32_t market_id;
    uint32_t price;
    uint64_t quantity;
    uint8_t side;
    uint8_t order_type;
    uint64_t timestamp_ns;
    uint64_t client_order_id;
    
    OrderRequest()
        : market_id(0)
        , price(0)
        , quantity(0)
        , side(0)
        , order_type(0)
        , timestamp_ns(0)
        , client_order_id(0) {}
};

struct alignas(64) OrderResponse {
    uint64_t client_order_id;
    uint64_t exchange_order_id;
    uint32_t status;
    uint64_t timestamp_ns;
    
    OrderResponse()
        : client_order_id(0)
        , exchange_order_id(0)
        , status(0)
        , timestamp_ns(0) {}
};

class OrderGateway {
    SPSCQueue<Signal, 1024>* signal_queue_;
    SPSCQueue<OrderRequest, 1024>* order_queue_;
    
    std::atomic<uint64_t> order_id_counter_{1};
    
public:
    OrderGateway(
        SPSCQueue<Signal, 1024>* signal_queue,
        SPSCQueue<OrderRequest, 1024>* order_queue
    ) : signal_queue_(signal_queue)
      , order_queue_(order_queue) {
    }

    void run() noexcept {
        ThreadConfig::configure(ThreadConfig::Role::Execution);
        
        Signal signal;
        
        while (__builtin_expect(true, 1)) {
            if (__builtin_expect(signal_queue_->pop(signal), 0)) {
                process_signal(signal);
            }
            
            __builtin_ia32_pause();
        }
    }

private:
    __attribute__((always_inline))
    void process_signal(const Signal& signal) noexcept {
        const uint64_t signal_latency = rdtsc() - signal.timestamp_ns;
        
        if (__builtin_expect(signal_latency > 100000, 0)) {
            return;
        }
        
        OrderRequest order;
        order.market_id = signal.market_id;
        order.price = signal.target_price;
        order.quantity = calculate_position_size(signal);
        order.side = signal.direction > 0 ? 1 : 2;
        order.order_type = 1;
        order.timestamp_ns = rdtsc();
        order.client_order_id = generate_order_id();
        
        if (__builtin_expect(!order_queue_->push(order), 0)) {
            return;
        }
    }

    __attribute__((always_inline))
    uint64_t calculate_position_size(const Signal& signal) noexcept {
        uint64_t base_size = 1000000;
        
        uint64_t confidence_multiplier = signal.confidence;
        confidence_multiplier = confidence_multiplier > 100 ? 100 : confidence_multiplier;
        
        return base_size * confidence_multiplier / 50;
    }

    __attribute__((always_inline))
    uint64_t generate_order_id() noexcept {
        return order_id_counter_.fetch_add(1, std::memory_order_relaxed);
    }
};

}
