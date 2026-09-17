#include <Live2DCubismCore.h>

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <limits>
#include <memory>
#include <numeric>
#include <stdexcept>
#include <string>
#include <vector>

namespace
{
using Clock = std::chrono::steady_clock;
volatile double g_sink = 0.0;

uint64_t NowNs()
{
    return std::chrono::duration_cast<std::chrono::nanoseconds>(
        Clock::now().time_since_epoch()).count();
}

struct AlignedBuffer
{
    AlignedBuffer(size_t alignment, size_t size) : size(size)
    {
        if (posix_memalign(&data, alignment, std::max<size_t>(size, 1)) != 0)
        {
            throw std::bad_alloc();
        }
    }
    ~AlignedBuffer() { std::free(data); }
    AlignedBuffer(const AlignedBuffer&) = delete;
    AlignedBuffer& operator=(const AlignedBuffer&) = delete;
    void* data = nullptr;
    size_t size = 0;
};

std::vector<unsigned char> ReadFile(const char* path)
{
    std::ifstream file(path, std::ios::binary | std::ios::ate);
    if (!file)
    {
        throw std::runtime_error(std::string("cannot open ") + path);
    }
    const std::streamsize size = file.tellg();
    file.seekg(0);
    std::vector<unsigned char> bytes(static_cast<size_t>(size));
    if (!file.read(reinterpret_cast<char*>(bytes.data()), size))
    {
        throw std::runtime_error(std::string("cannot read ") + path);
    }
    return bytes;
}

struct Model
{
    Model(const csmMoc* moc, unsigned int modelSize)
        : memory(csmAlignofModel, modelSize)
        , model(csmInitializeModelInPlace(moc, memory.data, modelSize))
    {
        if (!model)
        {
            throw std::runtime_error("csmInitializeModelInPlace failed");
        }
        parameterCount = csmGetParameterCount(model);
        partCount = csmGetPartCount(model);
        drawableCount = csmGetDrawableCount(model);
        values = csmGetParameterValues(model);
        minimums = csmGetParameterMinimumValues(model);
        maximums = csmGetParameterMaximumValues(model);
        partOpacities = csmGetPartOpacities(model);
    }

    AlignedBuffer memory;
    csmModel* model;
    int parameterCount;
    int partCount;
    int drawableCount;
    float* values;
    const float* minimums;
    const float* maximums;
    float* partOpacities;
};

void WriteParameters(Model& model, uint64_t tick)
{
    for (int i = 0; i < model.parameterCount; ++i)
    {
        const uint64_t bits = (tick * 747796405ull + static_cast<uint64_t>(i) * 2891336453ull) ^
            (tick >> 7);
        const float unit = static_cast<float>(bits & 0xffffu) / 65535.0f;
        model.values[i] = model.minimums[i] + unit * (model.maximums[i] - model.minimums[i]);
    }
}

void WritePartOpacities(Model& model, uint64_t tick)
{
    for (int i = 0; i < model.partCount; ++i)
    {
        const uint64_t bits = tick * 277803737ull + static_cast<uint64_t>(i) * 2246822519ull;
        model.partOpacities[i] = 0.25f + 0.75f * static_cast<float>(bits & 0xffffu) / 65535.0f;
    }
}

double ReadDynamicDrawables(const Model& model)
{
    const csmFlags* flags = csmGetDrawableDynamicFlags(model.model);
    const float* opacities = csmGetDrawableOpacities(model.model);
    const int* orders = csmGetRenderOrders(model.model);
    const int* vertexCounts = csmGetDrawableVertexCounts(model.model);
    const csmVector2** positions = csmGetDrawableVertexPositions(model.model);
    const csmVector4* multiply = csmGetDrawableMultiplyColors(model.model);
    const csmVector4* screen = csmGetDrawableScreenColors(model.model);
    double sum = 0.0;
    const float* partOpacities = csmGetPartOpacities(model.model);
    for (int part = 0; part < model.partCount; ++part) sum += partOpacities[part];
    for (int drawable = 0; drawable < model.drawableCount; ++drawable)
    {
        sum += flags[drawable] + opacities[drawable] + orders[drawable];
        sum += multiply[drawable].X + multiply[drawable].W;
        sum += screen[drawable].Y + screen[drawable].Z;
        const int count = vertexCounts[drawable];
        const csmVector2* vertices = positions[drawable];
        for (int vertex = 0; vertex < count; ++vertex)
        {
            sum += vertices[vertex].X * 0.25 + vertices[vertex].Y * 0.5;
        }
    }
    return sum;
}

double ReadStaticMetadata(const Model& model)
{
    const csmParameterType* parameterTypes = csmGetParameterTypes(model.model);
    const float* defaults = csmGetParameterDefaultValues(model.model);
    const int* repeats = csmGetParameterRepeats(model.model);
    const int* keyCounts = csmGetParameterKeyCounts(model.model);
    const float** keyValues = csmGetParameterKeyValues(model.model);
    const int* partParents = csmGetPartParentPartIndices(model.model);
    const int* partOffscreens = csmGetPartOffscreenIndices(model.model);
    const int* textureIndices = csmGetDrawableTextureIndices(model.model);
    const int* drawOrders = csmGetDrawableDrawOrders(model.model);
    const int* maskCounts = csmGetDrawableMaskCounts(model.model);
    const int** masks = csmGetDrawableMasks(model.model);
    const int* indexCounts = csmGetDrawableIndexCounts(model.model);
    const unsigned short** indices = csmGetDrawableIndices(model.model);
    const int* parentParts = csmGetDrawableParentPartIndices(model.model);
    double sum = 0.0;
    for (int parameter = 0; parameter < model.parameterCount; ++parameter)
    {
        sum += parameterTypes[parameter] + defaults[parameter] + repeats[parameter];
        for (int key = 0; key < keyCounts[parameter]; ++key) sum += keyValues[parameter][key];
    }
    for (int part = 0; part < model.partCount; ++part)
    {
        sum += partParents[part] + partOffscreens[part];
    }
    for (int drawable = 0; drawable < model.drawableCount; ++drawable)
    {
        sum += textureIndices[drawable] + drawOrders[drawable] + parentParts[drawable];
        for (int i = 0; i < maskCounts[drawable]; ++i) sum += masks[drawable][i];
        for (int i = 0; i < indexCounts[drawable]; ++i) sum += indices[drawable][i] * 0.001;
    }
    return sum;
}

struct Measurement
{
    double nsPerOperation;
    uint64_t operations;
};

template<class Function>
Measurement Measure(uint64_t targetNs, Function&& function)
{
    for (int i = 0; i < 8; ++i) function();
    uint64_t operations = 0;
    const uint64_t start = NowNs();
    uint64_t elapsed = 0;
    do
    {
        for (int batch = 0; batch < 8; ++batch) function();
        operations += 8;
        elapsed = NowNs() - start;
    } while (elapsed < targetNs);
    return {static_cast<double>(elapsed) / operations, operations};
}

template<class Function>
double MedianSingleShot(int samples, Function&& function)
{
    std::vector<double> timings;
    timings.reserve(samples);
    for (int i = 0; i < samples; ++i)
    {
        const uint64_t start = NowNs();
        function();
        timings.push_back(static_cast<double>(NowNs() - start));
    }
    std::sort(timings.begin(), timings.end());
    return timings[timings.size() / 2];
}

void NullLog(const char*) {}

void PrintPhase(const char* name, const Measurement& result, bool comma = true)
{
    std::printf("    \"%s\":{\"ns_per_operation\":%.3f,\"operations_per_second\":%.3f,\"operations\":%llu}%s\n",
        name, result.nsPerOperation, 1.0e9 / result.nsPerOperation,
        static_cast<unsigned long long>(result.operations), comma ? "," : "");
}
}

int main(int argc, char** argv)
{
    if (argc != 2)
    {
        std::fprintf(stderr, "usage: %s model.moc3\n", argv[0]);
        return 2;
    }

    try
    {
        csmSetLogFunction(NullLog);
        const std::vector<unsigned char> source = ReadFile(argv[1]);
        if (source.size() > std::numeric_limits<unsigned int>::max())
        {
            throw std::runtime_error("moc3 is too large for the Core ABI");
        }
        const unsigned int mocSize = static_cast<unsigned int>(source.size());
        AlignedBuffer mocMemory(csmAlignofMoc, source.size());
        std::memcpy(mocMemory.data, source.data(), source.size());
        csmMoc* moc = csmReviveMocInPlace(mocMemory.data, mocSize);
        if (!moc) throw std::runtime_error("csmReviveMocInPlace failed");
        const unsigned int modelSize = csmGetSizeofModel(moc);

        std::vector<std::unique_ptr<Model>> models;
        models.reserve(BENCHMARK_MODEL_COUNT);
        for (int i = 0; i < BENCHMARK_MODEL_COUNT; ++i)
        {
            models.emplace_back(new Model(moc, modelSize));
        }
        Model& primary = *models.front();
        csmUpdateModel(primary.model);

        uint64_t totalVertices = 0;
        const int* vertexCounts = csmGetDrawableVertexCounts(primary.model);
        for (int i = 0; i < primary.drawableCount; ++i) totalVertices += vertexCounts[i];

        // Startup operations allocate internal state in some ABI providers and
        // have no matching public destroy API. Keep samples bounded to avoid
        // turning their implementation detail into memory pressure.
        AlignedBuffer startupMoc(csmAlignofMoc, source.size());
        const double copyNs = MedianSingleShot(17, [&]
        {
            std::memcpy(startupMoc.data, source.data(), source.size());
            g_sink += static_cast<unsigned char*>(startupMoc.data)[source.size() / 2];
        });
        const double consistencyWithCopyNs = MedianSingleShot(17, [&]
        {
            std::memcpy(startupMoc.data, source.data(), source.size());
            g_sink += csmHasMocConsistency(startupMoc.data, mocSize);
        });
        const double reviveWithCopyNs = MedianSingleShot(17, [&]
        {
            std::memcpy(startupMoc.data, source.data(), source.size());
            g_sink += csmReviveMocInPlace(startupMoc.data, mocSize) != nullptr;
        });
        AlignedBuffer startupModel(csmAlignofModel, modelSize);
        const double initializeNs = MedianSingleShot(17, [&]
        {
            g_sink += csmInitializeModelInPlace(moc, startupModel.data, modelSize) != nullptr;
        });

        const uint64_t phaseNs = static_cast<uint64_t>(BENCHMARK_PHASE_MILLISECONDS) * 1000000ull;
        uint64_t tick = 1;
        const Measurement parameterWrite = Measure(phaseNs, [&]
        {
            WriteParameters(primary, tick++);
            g_sink += primary.values[tick % primary.parameterCount];
        });
        const Measurement partOpacityWrite = Measure(phaseNs, [&]
        {
            WritePartOpacities(primary, tick++);
            g_sink += primary.partOpacities[tick % primary.partCount];
        });
        const Measurement idleUpdate = Measure(phaseNs, [&]
        {
            csmResetDrawableDynamicFlags(primary.model);
            csmUpdateModel(primary.model);
            g_sink += csmGetDrawableDynamicFlags(primary.model)[0];
        });
        const Measurement animatedUpdate = Measure(phaseNs, [&]
        {
            WriteParameters(primary, tick++);
            WritePartOpacities(primary, tick);
            csmResetDrawableDynamicFlags(primary.model);
            csmUpdateModel(primary.model);
            g_sink += csmGetDrawableOpacities(primary.model)[0];
        });
        const Measurement dynamicReadback = Measure(phaseNs, [&]
        {
            g_sink += ReadDynamicDrawables(primary);
        });
        const Measurement staticMetadata = Measure(phaseNs, [&]
        {
            g_sink += ReadStaticMetadata(primary);
        });
        const Measurement workingSetFrame = Measure(phaseNs, [&]
        {
            for (std::unique_ptr<Model>& model : models)
            {
                WriteParameters(*model, tick++);
                WritePartOpacities(*model, tick);
                csmResetDrawableDynamicFlags(model->model);
                csmUpdateModel(model->model);
                g_sink += ReadDynamicDrawables(*model);
            }
        });

        csmVector2 canvasSize = {}, canvasOrigin = {};
        float pixelsPerUnit = 0.0f;
        csmReadCanvasInfo(primary.model, &canvasSize, &canvasOrigin, &pixelsPerUnit);
        const double finalChecksum = ReadDynamicDrawables(primary) + g_sink * 1.0e-300;

        std::printf("BENCHMARK_RESULT {\n");
        std::printf("  \"schema_version\":1,\"case_id\":\"%s\",\"core_backend\":\"%s\",\"host_backend\":\"core-only\",\n",
            BENCHMARK_CASE_ID, BENCHMARK_CORE_PROVIDER);
        std::printf("  \"core_version\":%u,\"moc_version\":%u,\"moc_bytes\":%u,\"model_bytes\":%u,\n",
            csmGetVersion(), csmGetMocVersion(source.data(), mocSize), mocSize, modelSize);
        std::printf("  \"model_instances\":%d,\"parameters\":%d,\"parts\":%d,\"drawables\":%d,\"vertices\":%llu,\n",
            BENCHMARK_MODEL_COUNT, primary.parameterCount, csmGetPartCount(primary.model),
            primary.drawableCount, static_cast<unsigned long long>(totalVertices));
        std::printf("  \"canvas\":[%.3f,%.3f,%.3f],\"checksum\":%.9g,\n",
            canvasSize.X, canvasSize.Y, pixelsPerUnit, finalChecksum);
        std::printf("  \"startup\":{\"samples\":17,\"copy_ns\":%.3f,\"consistency_with_copy_ns\":%.3f,\"revive_with_copy_ns\":%.3f,\"initialize_ns\":%.3f},\n",
            copyNs, consistencyWithCopyNs, reviveWithCopyNs, initializeNs);
        std::printf("  \"phases\":{\n");
        PrintPhase("parameter_write", parameterWrite);
        PrintPhase("part_opacity_write", partOpacityWrite);
        PrintPhase("idle_update", idleUpdate);
        PrintPhase("animated_update", animatedUpdate);
        PrintPhase("dynamic_drawable_readback", dynamicReadback);
        PrintPhase("static_metadata_readback", staticMetadata);
        PrintPhase("working_set_frame_40", workingSetFrame, false);
        std::printf("  },\n");
        std::printf("  \"working_set_ns_per_model\":%.3f,\"working_set_model_updates_per_second\":%.3f\n",
            workingSetFrame.nsPerOperation / BENCHMARK_MODEL_COUNT,
            1.0e9 * BENCHMARK_MODEL_COUNT / workingSetFrame.nsPerOperation);
        std::printf("}\n");
    }
    catch (const std::exception& error)
    {
        std::fprintf(stderr, "benchmark failed: %s\n", error.what());
        return 1;
    }
    return 0;
}
