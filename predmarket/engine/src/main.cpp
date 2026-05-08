#include "order_book.hpp"
#include "wal.hpp"
#include "nats_client.hpp"
#include "wire.hpp"
#include "uuid.hpp"

#include <nlohmann/json.hpp>

#include <csignal>
#include <atomic>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <mutex>
#include <thread>
#include <chrono>
#include <unordered_map>

using json = nlohmann::json;

namespace {

std::atomic<bool> g_running{true};

void handle_signal(int) {
    g_running.store(false, std::memory_order_relaxed);
}

struct EngineConfig {
    const char* nats_url      = "nats://127.0.0.1:4222";
    const char* order_subject = "predmarket.orders";
    const char* fill_subject  = "predmarket.fills";
    const char* depth_subject = "predmarket.depth";
    const char* wal_dir       = "/var/lib/predmarket/wal";
};

EngineConfig load_config() {
    EngineConfig cfg;
    auto env = [](const char* key, const char* def) -> const char* {
        const char* v = std::getenv(key);
        return v ? v : def;
    };
    cfg.nats_url      = env("PREDMARKET_NATS_URL",      cfg.nats_url);
    cfg.order_subject = env("PREDMARKET_ORDER_SUBJECT", cfg.order_subject);
    cfg.fill_subject  = env("PREDMARKET_FILL_SUBJECT",  cfg.fill_subject);
    cfg.depth_subject = env("PREDMARKET_DEPTH_SUBJECT", cfg.depth_subject);
    cfg.wal_dir       = env("PREDMARKET_WAL_DIR",       cfg.wal_dir);
    return cfg;
}

thread_local char g_json_buf[1024];

int fill_to_json(const predmarket::Fill& f, char* out, int cap) {
    char fid[37], mid[37], maoid[37], taoid[37], mauid[37], tauid[37];
    f.fill_id.to_str(fid);
    f.market_id.to_str(mid);
    f.maker_order_id.to_str(maoid);
    f.taker_order_id.to_str(taoid);
    f.maker_user_id.to_str(mauid);
    f.taker_user_id.to_str(tauid);

    return std::snprintf(out, cap,
        "{\"fill_id\":\"%s\",\"market_id\":\"%s\","
        "\"maker_order_id\":\"%s\",\"taker_order_id\":\"%s\","
        "\"maker_user_id\":\"%s\",\"taker_user_id\":\"%s\","
        "\"price\":%u,\"quantity\":%llu,"
        "\"aggressor_side\":\"%s\","
        "\"sequence\":%llu,\"timestamp_ns\":%llu}",
        fid, mid, maoid, taoid, mauid, tauid,
        f.price,
        static_cast<unsigned long long>(f.quantity),
        f.aggressor_side == 0 ? "buy" : "sell",
        static_cast<unsigned long long>(f.sequence),
        static_cast<unsigned long long>(f.timestamp_ns)
    );
}

struct UuidBookMap {
    struct Entry {
        predmarket::Uuid                              key;
        std::unique_ptr<predmarket::OrderBook>        book;
        bool                                          occupied = false;
    };

    static constexpr std::uint32_t kBuckets = 256;
    Entry m_entries[kBuckets];

    predmarket::OrderBook* find(const predmarket::Uuid& id) {
        std::uint32_t h = predmarket::UuidHasher{}(id) & (kBuckets - 1);
        for (std::uint32_t i = 0; i < kBuckets; ++i) {
            std::uint32_t slot = (h + i) & (kBuckets - 1);
            if (!m_entries[slot].occupied) return nullptr;
            if (m_entries[slot].key == id) return m_entries[slot].book.get();
        }
        return nullptr;
    }

    predmarket::OrderBook* insert(const predmarket::Uuid& id, std::unique_ptr<predmarket::OrderBook> book) {
        std::uint32_t h = predmarket::UuidHasher{}(id) & (kBuckets - 1);
        for (std::uint32_t i = 0; i < kBuckets; ++i) {
            std::uint32_t slot = (h + i) & (kBuckets - 1);
            if (!m_entries[slot].occupied) {
                m_entries[slot].occupied = true;
                m_entries[slot].key      = id;
                m_entries[slot].book     = std::move(book);
                return m_entries[slot].book.get();
            }
        }
        return nullptr;
    }
};

struct Engine {
    EngineConfig      config;
    predmarket::NatsClient nats;
    predmarket::Wal   wal;
    UuidBookMap       books;
    std::mutex        mu;

    Engine(EngineConfig cfg)
        : config(cfg)
        , nats(cfg.nats_url)
        , wal(std::string(cfg.wal_dir) + "/engine.wal")
    {}

    predmarket::OrderBook& get_or_create(const predmarket::Uuid& market_id) {
        predmarket::OrderBook* existing = books.find(market_id);
        if ([[likely]] existing != nullptr) return *existing;

        auto on_fill = [this, market_id](const predmarket::Fill& fill) {
            int n = fill_to_json(fill, g_json_buf, sizeof(g_json_buf));
            if (n > 0) {
                nats.publish(config.fill_subject,
                    reinterpret_cast<const std::uint8_t*>(g_json_buf),
                    static_cast<std::size_t>(n));
            }

            char depth_subject[128];
            char mid[37];
            fill.market_id.to_str(mid);
            std::snprintf(depth_subject, sizeof(depth_subject), "%s.%s", config.depth_subject, mid);

            char depth_buf[256];
            int dn = std::snprintf(depth_buf, sizeof(depth_buf),
                "{\"market_id\":\"%s\",\"type\":\"fill\",\"price\":%u,\"quantity\":%llu}",
                mid, fill.price, static_cast<unsigned long long>(fill.quantity));
            if (dn > 0) {
                nats.publish(depth_subject,
                    reinterpret_cast<const std::uint8_t*>(depth_buf),
                    static_cast<std::size_t>(dn));
            }
        };

        auto book = std::make_unique<predmarket::OrderBook>(market_id, std::move(on_fill));
        predmarket::OrderBook* ptr = books.insert(market_id, std::move(book));
        return *ptr;
    }

    void dispatch(const std::uint8_t* data, std::size_t len) noexcept {
        if ([[unlikely]] len < 1) return;
        const std::uint8_t op = data[0];

        if (op == 0x01 && [[likely]] len >= 71) {
            const predmarket::Uuid order_id  = predmarket::Uuid::from_bytes(data + 1);
            const predmarket::Uuid market_id = predmarket::Uuid::from_bytes(data + 17);
            const predmarket::Uuid user_id   = predmarket::Uuid::from_bytes(data + 33);
            const std::uint8_t  side_raw  = data[49];
            const std::uint8_t  is_market = data[50];
            const std::uint32_t price     = predmarket::wire::read_u32_le(data + 51);
            const std::uint64_t quantity  = predmarket::wire::read_u64_le(data + 55);
            const std::uint64_t sequence  = predmarket::wire::read_u64_le(data + 63);

            const predmarket::Side side = (side_raw == 0)
                ? predmarket::Side::Buy
                : predmarket::Side::Sell;

            std::lock_guard<std::mutex> lock(mu);
            wal.append(predmarket::WalOpType::AddOrder, data, static_cast<std::uint32_t>(len));
            predmarket::OrderBook& book = get_or_create(market_id);

            if (is_market) {
                book.add_market_order(order_id, user_id, side, quantity, sequence);
            } else {
                book.add_limit_order(order_id, user_id, side, price, quantity, sequence);
            }

        } else if (op == 0x02 && len >= 41) {
            const predmarket::Uuid order_id  = predmarket::Uuid::from_bytes(data + 1);
            const predmarket::Uuid market_id = predmarket::Uuid::from_bytes(data + 17);
            const std::uint64_t   sequence   = predmarket::wire::read_u64_le(data + 33);

            std::lock_guard<std::mutex> lock(mu);
            predmarket::OrderBook* book = books.find(market_id);
            if (book) {
                wal.append(predmarket::WalOpType::CancelOrder, data, static_cast<std::uint32_t>(len));
                book->cancel_order(order_id);
            }
        }
    }
};

}

int main() {
    std::signal(SIGINT,  handle_signal);
    std::signal(SIGTERM, handle_signal);

    auto config = load_config();

    std::fprintf(stdout, "[predmarket-engine] connecting to %s\n", config.nats_url);

    Engine engine(config);

    auto records = engine.wal.recover();
    std::fprintf(stdout, "[predmarket-engine] replayed %zu WAL records\n", records.size());

    engine.nats.subscribe(config.order_subject,
        [&engine](const std::uint8_t* data, std::size_t len) {
            engine.dispatch(data, len);
        }
    );

    std::fprintf(stdout, "[predmarket-engine] ready\n");

    while (g_running.load(std::memory_order_relaxed)) {
        std::this_thread::sleep_for(std::chrono::milliseconds(10));
    }

    std::fprintf(stdout, "[predmarket-engine] draining\n");
    engine.nats.drain();
    std::fprintf(stdout, "[predmarket-engine] stopped\n");

    return 0;
}
