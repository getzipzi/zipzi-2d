#pragma once

namespace BenchmarkConfig
{
    constexpr const char* CaseId = KASANE_BENCHMARK_CASE_ID;
    constexpr const char* CoreProvider = KASANE_BENCHMARK_CORE_PROVIDER;
    constexpr const char* WorkloadId = KASANE_BENCHMARK_WORKLOAD_ID;
    constexpr const char* ModelName = KASANE_BENCHMARK_MODEL_NAME;
    constexpr const char* ModelHash = KASANE_BENCHMARK_MODEL_HASH;
    constexpr int ModelCount = KASANE_BENCHMARK_MODEL_COUNT;
    constexpr int Columns = KASANE_BENCHMARK_COLUMNS;
    constexpr int Rows = KASANE_BENCHMARK_ROWS;
    constexpr int Width = KASANE_BENCHMARK_WIDTH;
    constexpr int Height = KASANE_BENCHMARK_HEIGHT;
    constexpr double WarmupSeconds = KASANE_BENCHMARK_WARMUP_SECONDS;
    constexpr double SampleSeconds = KASANE_BENCHMARK_SAMPLE_SECONDS;
}
