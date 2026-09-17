cmake_minimum_required(VERSION 3.16)

project(CubismCoreBenchmark LANGUAGES CXX)

set(SDK_ROOT_PATH "${CMAKE_CURRENT_LIST_DIR}/../../../../third_party/CubismSdkForNative-5-r.5" CACHE PATH "Cubism Native SDK root")
set(CORE_PROVIDER "cubism" CACHE STRING "Core implementation: cubism or purism")
set_property(CACHE CORE_PROVIDER PROPERTY STRINGS cubism purism)
set(PURISM_CORE_LIBRARY "${CMAKE_CURRENT_LIST_DIR}/../../../../target/cubism-matrix/core/purism-v6/libPurismCore.a" CACHE FILEPATH "Purism Core v6 static library")
set(BENCHMARK_CASE_ID "${CORE_PROVIDER}-core" CACHE STRING "Benchmark case identifier")
set(BENCHMARK_MODEL_COUNT 40 CACHE STRING "Models in the working-set phase")
set(BENCHMARK_PHASE_MILLISECONDS 750 CACHE STRING "Target duration of each steady-state phase")

execute_process(COMMAND uname -m COMMAND tr -d '\n' OUTPUT_VARIABLE ARCHITECTURE)
if(CORE_PROVIDER STREQUAL "cubism")
  set(CORE_LIBRARY "${SDK_ROOT_PATH}/Core/lib/macos/${ARCHITECTURE}/libLive2DCubismCore.a")
elseif(CORE_PROVIDER STREQUAL "purism")
  set(CORE_LIBRARY "${PURISM_CORE_LIBRARY}")
else()
  message(FATAL_ERROR "CORE_PROVIDER must be 'cubism' or 'purism'")
endif()
if(NOT EXISTS "${CORE_LIBRARY}")
  message(FATAL_ERROR "Core library does not exist: ${CORE_LIBRARY}")
endif()

add_executable(cubism-core-benchmark main.cpp)
target_compile_features(cubism-core-benchmark PRIVATE cxx_std_17)
target_compile_options(cubism-core-benchmark PRIVATE -O3 -DNDEBUG)
target_compile_definitions(cubism-core-benchmark PRIVATE
  BENCHMARK_CASE_ID="${BENCHMARK_CASE_ID}"
  BENCHMARK_CORE_PROVIDER="${CORE_PROVIDER}"
  BENCHMARK_MODEL_COUNT=${BENCHMARK_MODEL_COUNT}
  BENCHMARK_PHASE_MILLISECONDS=${BENCHMARK_PHASE_MILLISECONDS}
)
target_include_directories(cubism-core-benchmark PRIVATE "${SDK_ROOT_PATH}/Core/include")
target_link_libraries(cubism-core-benchmark PRIVATE "${CORE_LIBRARY}")

