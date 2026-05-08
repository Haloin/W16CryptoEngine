#include "nats_client.hpp"

#include <nats/nats.h>
#include <stdexcept>
#include <string>

namespace predmarket {

namespace {

struct CallbackContext {
    MessageCallback cb;
};

void nats_msg_handler(natsConnection*, natsSubscription*, natsMsg* msg, void* ctx) {
    auto* context = static_cast<CallbackContext*>(ctx);
    const auto* data = reinterpret_cast<const uint8_t*>(natsMsg_GetData(msg));
    int         len  = natsMsg_GetDataLength(msg);
    if (data && len > 0) {
        context->cb(data, static_cast<size_t>(len));
    }
    natsMsg_Destroy(msg);
}

}

NatsClient::NatsClient(const std::string& url)
    : conn_(nullptr)
    , sub_(nullptr)
{
    natsOptions* opts = nullptr;
    natsOptions_Create(&opts);
    natsOptions_SetURL(opts, url.c_str());
    natsOptions_SetMaxReconnect(opts, -1);
    natsOptions_SetReconnectWait(opts, 1000);

    natsStatus status = natsConnection_Connect(&conn_, opts);
    natsOptions_Destroy(opts);

    if (status != NATS_OK) {
        throw std::runtime_error("nats connect failed: " + std::to_string(status));
    }
}

NatsClient::~NatsClient() {
    if (sub_)  natsSubscription_Destroy(sub_);
    if (conn_) natsConnection_Destroy(conn_);
}

void NatsClient::subscribe(const std::string& subject, MessageCallback cb) {
    auto* ctx = new CallbackContext{std::move(cb)};

    natsStatus status = natsConnection_Subscribe(
        &sub_, conn_, subject.c_str(), nats_msg_handler, ctx
    );

    if (status != NATS_OK) {
        delete ctx;
        throw std::runtime_error("nats subscribe failed: " + std::to_string(status));
    }
}

void NatsClient::publish(const std::string& subject, const uint8_t* data, size_t len) {
    natsConnection_Publish(
        conn_,
        subject.c_str(),
        reinterpret_cast<const void*>(data),
        static_cast<int>(len)
    );
}

void NatsClient::drain() {
    if (conn_) {
        natsConnection_Drain(conn_);
    }
}

}
