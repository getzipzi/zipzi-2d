#pragma once

namespace BenchmarkConfig
{
    constexpr const char* CaseId = ZIPZI_BENCHMARK_CASE_ID;
    constexpr const char* CoreProvider = ZIPZI_BENCHMARK_CORE_PROVIDER;
    constexpr const char* WorkloadId = ZIPZI_BENCHMARK_WORKLOAD_ID;
    constexpr const char* ModelName = ZIPZI_BENCHMARK_MODEL_NAME;
    constexpr const char* ModelHash = ZIPZI_BENCHMARK_MODEL_HASH;
    constexpr int ModelCount = ZIPZI_BENCHMARK_MODEL_COUNT;
    constexpr int Columns = ZIPZI_BENCHMARK_COLUMNS;
    constexpr int Rows = ZIPZI_BENCHMARK_ROWS;
    constexpr int Width = ZIPZI_BENCHMARK_WIDTH;
    constexpr int Height = ZIPZI_BENCHMARK_HEIGHT;
    constexpr double WarmupSeconds = ZIPZI_BENCHMARK_WARMUP_SECONDS;
    constexpr double SampleSeconds = ZIPZI_BENCHMARK_SAMPLE_SECONDS;
}
