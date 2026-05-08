#include <gtest/gtest.h>
#include "simdjson_parser.hpp"

using namespace hft;

TEST(MarketDataParserTest, ParseOrderBook) {
    SimdJsonParser parser;
    
    const char* json = R"({
        "sequence": 12345,
        "timestamp": 1678901234567890123,
        "market_id": 42,
        "bids": [
            {"price": 5000, "quantity": 100},
            {"price": 4999, "quantity": 200}
        ],
        "asks": [
            {"price": 5001, "quantity": 150},
            {"price": 5002, "quantity": 75}
        ]
    })";
    
    OrderBookUpdate update;
    EXPECT_TRUE(parser.parse_orderbook(json, strlen(json), update));
    
    EXPECT_EQ(update.sequence, 12345);
    EXPECT_EQ(update.timestamp_ns, 1678901234567890123);
    EXPECT_EQ(update.market_id, 42);
    EXPECT_EQ(update.bid_count, 2);
    EXPECT_EQ(update.ask_count, 2);
    EXPECT_EQ(update.bids[0].price, 5000);
    EXPECT_EQ(update.bids[0].quantity, 100);
    EXPECT_EQ(update.asks[0].price, 5001);
    EXPECT_EQ(update.asks[0].quantity, 150);
}

TEST(MarketDataParserTest, ParseInvalidJson) {
    SimdJsonParser parser;
    
    const char* json = "invalid json";
    OrderBookUpdate update;
    EXPECT_FALSE(parser.parse_orderbook(json, strlen(json), update));
}

TEST(MarketDataParserTest, ParseEmptyBook) {
    SimdJsonParser parser;
    
    const char* json = R"({
        "sequence": 1,
        "timestamp": 123,
        "market_id": 1,
        "bids": [],
        "asks": []
    })";
    
    OrderBookUpdate update;
    EXPECT_TRUE(parser.parse_orderbook(json, strlen(json), update));
    EXPECT_EQ(update.bid_count, 0);
    EXPECT_EQ(update.ask_count, 0);
}

TEST(MarketDataParserTest, ParseTrade) {
    SimdJsonParser parser;
    
    const char* json = R"({
        "sequence": 12345,
        "timestamp": 1678901234567890123,
        "market_id": 42,
        "price": 5000,
        "quantity": 100,
        "side": 1
    })";
    
    TradeUpdate trade;
    EXPECT_TRUE(parser.parse_trade(json, strlen(json), trade));
    
    EXPECT_EQ(trade.sequence, 12345);
    EXPECT_EQ(trade.timestamp_ns, 1678901234567890123);
    EXPECT_EQ(trade.market_id, 42);
    EXPECT_EQ(trade.price, 5000);
    EXPECT_EQ(trade.quantity, 100);
    EXPECT_EQ(trade.side, 1);
}
