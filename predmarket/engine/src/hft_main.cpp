#include "hft_orchestrator.hpp"
#include "hft_core.hpp"
#include <cstdio>
#include <csignal>
#include <cstring>

using namespace hft;

static volatile std::sig_atomic_t g_running = 1;

void signal_handler(int sig) {
    (void)sig;
    g_running = 0;
}

int main(int argc, char* argv[]) {
    (void)argc;
    (void)argv;
    
    std::signal(SIGINT, signal_handler);
    std::signal(SIGTERM, signal_handler);
    
    uint8_t private_key[32] = {0};
    
    HFTEngine engine;
    
    if (!engine.initialize(private_key)) {
        std::fprintf(stderr, "Failed to initialize HFT engine\n");
        return 1;
    }
    
    std::printf("HFT Engine initialized\n");
    std::printf("Latency target: <500us\n");
    std::printf("Jitter target: <10us\n");
    
    engine.start();
    
    const char* sample_md = R"({"sequence":1,"market_id":1,"bids":[[100,1000]],"asks":[[101,1000]]})";
    
    while (g_running) {
        if (!engine.on_market_data(sample_md, std::strlen(sample_md))) {
            __builtin_ia32_pause();
        }
    }
    
    engine.stop();
    
    std::printf("HFT Engine stopped\n");
    
    return 0;
}
