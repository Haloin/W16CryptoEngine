#pragma once

#include <string>
#include <functional>
#include <memory>
#include <vector>
#include <cstdint>

struct natsConnection;
struct natsSubscription;

namespace predmarket {

using MessageCallback = std::function<void(const uint8_t* data, size_t len)>;

class NatsClient {
public:
    explicit NatsClient(const std::string& url);

    ~NatsClient();

    NatsClient(const NatsClient&)            = delete;
    NatsClient& operator=(const NatsClient&) = delete;

    void subscribe(const std::string& subject, MessageCallback cb);
    void publish(const std::string& subject, const uint8_t* data, size_t len);
    void drain();

private:
    natsConnection*  conn_;
    natsSubscription* sub_;
};

}
