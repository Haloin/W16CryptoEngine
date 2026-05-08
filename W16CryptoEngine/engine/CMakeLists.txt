cmake_minimum_required(VERSION 3.25)
project(predmarket-hft VERSION 1.0.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD          23)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS        OFF)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
set(CMAKE_INTERPROCEDURAL_OPTIMIZATION_RELEASE ON)

if(NOT CMAKE_BUILD_TYPE)
    set(CMAKE_BUILD_TYPE Release)
endif()

if(CMAKE_BUILD_TYPE STREQUAL "Release")
    set(CMAKE_CXX_FLAGS_RELEASE "")
    add_compile_options(
        -O3
        -march=native
        -mtune=native
        -mavx2
        -mfma
        -mbmi2
        -mlzcnt
        -funroll-loops
        -fomit-frame-pointer
        -fno-stack-protector
        -fno-exceptions
        -fno-rtti
        -fvisibility=hidden
        -fvisibility-inlines-hidden
        -ffast-math
        -fno-math-errno
        -DNDEBUG
        -D_FORTIFY_SOURCE=0
    )
    add_link_options(
        -Wl,--gc-sections
        -Wl,--strip-all
        -Wl,-O3
        -s
    )
elseif(CMAKE_BUILD_TYPE STREQUAL "Debug")
    add_compile_options(
        -O0
        -g3
        -fno-omit-frame-pointer
        -fsanitize=address,undefined
    )
    add_link_options(
        -fsanitize=address,undefined
    )
elseif(CMAKE_BUILD_TYPE STREQUAL "RelWithDebInfo")
    add_compile_options(
        -O3
        -g3
        -march=native
        -mtune=native
    )
endif()

add_compile_options(
    -Wall
    -Wextra
    -Wpedantic
    -Werror=return-type
    -Werror=uninitialized
    -Wno-unused-parameter
    -pthread
)

option(USE_IO_URING "Enable io_uring for kernel bypass networking" ON)
option(USE_SIMDJSON "Enable simdjson for zero-copy parsing" ON)
option(USE_SECP256K1 "Enable libsecp256k1 for fast signing" ON)
option(USE_NUMA "Enable NUMA-aware memory allocation" ON)
option(USE_HUGEPAGES "Enable transparent hugepages" ON)

find_package(PkgConfig REQUIRED)
find_package(Threads REQUIRED)

find_library(SECP256K1_LIBRARY secp256k1 PATHS /usr/local/lib)
find_path(SECP256K1_INCLUDE_DIR secp256k1.h PATHS /usr/local/include)

find_library(SIMDJSON_LIBRARY simdjson PATHS /usr/local/lib)
find_path(SIMDJSON_INCLUDE_DIR simdjson.h PATHS /usr/local/include)

if(USE_IO_URING)
    find_library(URING_LIBRARY uring PATHS /usr/local/lib)
    if(URING_LIBRARY)
        add_compile_definitions(USE_IO_URING=1)
    endif()
endif()

if(USE_NUMA)
    find_library(NUMA_LIBRARY numa)
    if(NUMA_LIBRARY)
        add_compile_definitions(USE_NUMA=1)
    endif()
endif()

set(HFT_SOURCES
    src/hft_main.cpp
)

set(HFT_HEADERS
    include/hft_core.hpp
    include/spsc_queue.hpp
    include/market_data_parser.hpp
    include/strategy_engine.hpp
    include/order_gateway.hpp
    include/crypto_fast.hpp
    include/hft_orchestrator.hpp
)

add_executable(predmarket-hft ${HFT_SOURCES})

target_include_directories(predmarket-hft
    SYSTEM PRIVATE
        ${SECP256K1_INCLUDE_DIR}
        ${SIMDJSON_INCLUDE_DIR}
    PRIVATE
        include
)

target_link_libraries(predmarket-hft
    PRIVATE
        Threads::Threads
        ${SECP256K1_LIBRARY}
        ${SIMDJSON_LIBRARY}
        $<$<BOOL:${URING_LIBRARY}>:${URING_LIBRARY}>
        $<$<BOOL:${NUMA_LIBRARY}>:${NUMA_LIBRARY}>
)

target_compile_definitions(predmarket-hft
    PRIVATE
        $<$<CONFIG:Release>:HFT_PRODUCTION=1>
        $<$<BOOL:${USE_HUGEPAGES}>:USE_HUGEPAGES=1>
)

target_compile_features(predmarket-hft PRIVATE cxx_std_23)

set_target_properties(predmarket-hft PROPERTIES
    RUNTIME_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin
    POSITION_INDEPENDENT_CODE OFF
)

install(TARGETS predmarket-hft RUNTIME DESTINATION bin)
install(FILES ${HFT_HEADERS} DESTINATION include/predmarket)

enable_testing()
add_subdirectory(tests)
