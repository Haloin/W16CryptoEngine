#pragma once

#include "market_data_parser.hpp"
#include <simdjson.h>

namespace hft {

class SimdJsonParser {
    simdjson::ondemand::parser parser_;

public:
    bool parse_orderbook(const char* json, size_t len, OrderBookUpdate& out) noexcept {
        simdjson::ondemand::document doc;
        auto error = parser_.iterate(json, len).get(doc);
        if (error) {
            return false;
        }

        simdjson::ondemand::object obj;
        error = doc.get_object().get(obj);
        if (error) {
            return false;
        }

        simdjson::ondemand::value seq_val;
        if (obj.find_field("sequence").get(seq_val) == simdjson::SUCCESS) {
            int64_t seq;
            if (seq_val.get_int64().get(seq) == simdjson::SUCCESS) {
                out.sequence = static_cast<uint64_t>(seq);
            }
        }

        simdjson::ondemand::value ts_val;
        if (obj.find_field("timestamp").get(ts_val) == simdjson::SUCCESS) {
            int64_t ts;
            if (ts_val.get_int64().get(ts) == simdjson::SUCCESS) {
                out.timestamp_ns = static_cast<uint64_t>(ts);
            }
        }

        simdjson::ondemand::value market_val;
        if (obj.find_field("market_id").get(market_val) == simdjson::SUCCESS) {
            int64_t market_id;
            if (market_val.get_int64().get(market_id) == simdjson::SUCCESS) {
                out.market_id = static_cast<uint32_t>(market_id);
            }
        }

        simdjson::ondemand::value bids_val;
        if (obj.find_field("bids").get(bids_val) == simdjson::SUCCESS) {
            simdjson::ondemand::array bids_arr;
            if (bids_val.get_array().get(bids_arr) == simdjson::SUCCESS) {
                uint8_t i = 0;
                for (auto bid : bids_arr) {
                    if (i >= OrderBookUpdate::MAX_LEVELS) break;

                    simdjson::ondemand::object bid_obj;
                    if (bid.get_object().get(bid_obj) != simdjson::SUCCESS) continue;

                    simdjson::ondemand::value price_val;
                    if (bid_obj.find_field("price").get(price_val) == simdjson::SUCCESS) {
                        int64_t price;
                        if (price_val.get_int64().get(price) == simdjson::SUCCESS) {
                            out.bids[i].price = static_cast<uint32_t>(price);
                        }
                    }

                    simdjson::ondemand::value qty_val;
                    if (bid_obj.find_field("quantity").get(qty_val) == simdjson::SUCCESS) {
                        int64_t qty;
                        if (qty_val.get_int64().get(qty) == simdjson::SUCCESS) {
                            out.bids[i].quantity = static_cast<uint64_t>(qty);
                        }
                    }

                    ++i;
                }
                out.bid_count = i;
            }
        }

        simdjson::ondemand::value asks_val;
        if (obj.find_field("asks").get(asks_val) == simdjson::SUCCESS) {
            simdjson::ondemand::array asks_arr;
            if (asks_val.get_array().get(asks_arr) == simdjson::SUCCESS) {
                uint8_t i = 0;
                for (auto ask : asks_arr) {
                    if (i >= OrderBookUpdate::MAX_LEVELS) break;

                    simdjson::ondemand::object ask_obj;
                    if (ask.get_object().get(ask_obj) != simdjson::SUCCESS) continue;

                    simdjson::ondemand::value price_val;
                    if (ask_obj.find_field("price").get(price_val) == simdjson::SUCCESS) {
                        int64_t price;
                        if (price_val.get_int64().get(price) == simdjson::SUCCESS) {
                            out.asks[i].price = static_cast<uint32_t>(price);
                        }
                    }

                    simdjson::ondemand::value qty_val;
                    if (ask_obj.find_field("quantity").get(qty_val) == simdjson::SUCCESS) {
                        int64_t qty;
                        if (qty_val.get_int64().get(qty) == simdjson::SUCCESS) {
                            out.asks[i].quantity = static_cast<uint64_t>(qty);
                        }
                    }

                    ++i;
                }
                out.ask_count = i;
            }
        }

        return true;
    }

    bool parse_trade(const char* json, size_t len, TradeUpdate& out) noexcept {
        simdjson::ondemand::document doc;
        auto error = parser_.iterate(json, len).get(doc);
        if (error) {
            return false;
        }

        simdjson::ondemand::object obj;
        error = doc.get_object().get(obj);
        if (error) {
            return false;
        }

        simdjson::ondemand::value seq_val;
        if (obj.find_field("sequence").get(seq_val) == simdjson::SUCCESS) {
            int64_t seq;
            if (seq_val.get_int64().get(seq) == simdjson::SUCCESS) {
                out.sequence = static_cast<uint64_t>(seq);
            }
        }

        simdjson::ondemand::value ts_val;
        if (obj.find_field("timestamp").get(ts_val) == simdjson::SUCCESS) {
            int64_t ts;
            if (ts_val.get_int64().get(ts) == simdjson::SUCCESS) {
                out.timestamp_ns = static_cast<uint64_t>(ts);
            }
        }

        simdjson::ondemand::value market_val;
        if (obj.find_field("market_id").get(market_val) == simdjson::SUCCESS) {
            int64_t market_id;
            if (market_val.get_int64().get(market_id) == simdjson::SUCCESS) {
                out.market_id = static_cast<uint32_t>(market_id);
            }
        }

        simdjson::ondemand::value price_val;
        if (obj.find_field("price").get(price_val) == simdjson::SUCCESS) {
            int64_t price;
            if (price_val.get_int64().get(price) == simdjson::SUCCESS) {
                out.price = static_cast<uint32_t>(price);
            }
        }

        simdjson::ondemand::value qty_val;
        if (obj.find_field("quantity").get(qty_val) == simdjson::SUCCESS) {
            int64_t qty;
            if (qty_val.get_int64().get(qty) == simdjson::SUCCESS) {
                out.quantity = static_cast<uint64_t>(qty);
            }
        }

        simdjson::ondemand::value side_val;
        if (obj.find_field("side").get(side_val) == simdjson::SUCCESS) {
            int64_t side;
            if (side_val.get_int64().get(side) == simdjson::SUCCESS) {
                out.side = static_cast<uint8_t>(side);
            }
        }

        return true;
    }
};

}
