#include <gtest/gtest.h>
#include "spsc_queue.hpp"
#include <thread>
#include <atomic>

using namespace hft;

TEST(SPSCQueueTest, BasicPushPop) {
    SPSCQueue<int, 1024> queue;
    
    EXPECT_TRUE(queue.push(42));
    
    int value;
    EXPECT_TRUE(queue.pop(value));
    EXPECT_EQ(value, 42);
}

TEST(SPSCQueueTest, QueueFull) {
    SPSCQueue<int, 4> queue;
    
    EXPECT_TRUE(queue.push(1));
    EXPECT_TRUE(queue.push(2));
    EXPECT_TRUE(queue.push(3));
    EXPECT_TRUE(queue.push(4));
    EXPECT_FALSE(queue.push(5));
}

TEST(SPSCQueueTest, QueueEmpty) {
    SPSCQueue<int, 1024> queue;
    
    int value;
    EXPECT_FALSE(queue.pop(value));
}

TEST(SPSCQueueTest, MultiThreaded) {
    SPSCQueue<int, 1024> queue;
    std::atomic<int> produced{0};
    std::atomic<int> consumed{0};
    
    std::thread producer([&]() {
        for (int i = 0; i < 1000; ++i) {
            while (!queue.push(i)) {
                __builtin_ia32_pause();
            }
            produced.fetch_add(1);
        }
    });
    
    std::thread consumer([&]() {
        int value;
        for (int i = 0; i < 1000; ++i) {
            while (!queue.pop(value)) {
                __builtin_ia32_pause();
            }
            EXPECT_EQ(value, i);
            consumed.fetch_add(1);
        }
    });
    
    producer.join();
    consumer.join();
    
    EXPECT_EQ(produced.load(), 1000);
    EXPECT_EQ(consumed.load(), 1000);
}

TEST(SPSCQueueTest, CacheAlignedStruct) {
    struct alignas(64) TestStruct {
        uint64_t a;
        uint64_t b;
        uint64_t c;
        uint64_t d;
    };
    
    SPSCQueue<TestStruct, 1024> queue;
    
    TestStruct in{1, 2, 3, 4};
    EXPECT_TRUE(queue.push(in));
    
    TestStruct out;
    EXPECT_TRUE(queue.pop(out));
    EXPECT_EQ(out.a, 1);
    EXPECT_EQ(out.b, 2);
    EXPECT_EQ(out.c, 3);
    EXPECT_EQ(out.d, 4);
}
