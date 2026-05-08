cmake_minimum_required(VERSION 3.25)

find_package(GTest REQUIRED)

set(TEST_SOURCES
    test_spsc_queue.cpp
    test_market_data_parser.cpp
    test_strategy_engine.cpp
    test_crypto.cpp
    test_network_stack.cpp
)

add_executable(hft_tests ${TEST_SOURCES})

target_link_libraries(hft_tests
    PRIVATE
        GTest::gtest
        GTest::gtest_main
        Threads::Threads
        ${SECP256K1_LIBRARY}
        ${SIMDJSON_LIBRARY}
        $<$<BOOL:${URING_LIBRARY}>:${URING_LIBRARY}>
)

target_include_directories(hft_tests
    SYSTEM PRIVATE
        ${SECP256K1_INCLUDE_DIR}
        ${SIMDJSON_INCLUDE_DIR}
    PRIVATE
        ../include
)

target_compile_features(hft_tests PRIVATE cxx_std_23)

set_target_properties(hft_tests PROPERTIES
    RUNTIME_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin
)

add_test(NAME hft_unit_tests COMMAND hft_tests)
