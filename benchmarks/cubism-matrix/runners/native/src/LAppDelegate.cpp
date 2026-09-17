/**
 * Copyright(c) Live2D Inc. All rights reserved.
 *
 * Use of this source code is governed by the Live2D Open Software license
 * that can be found at https://www.live2d.com/eula/live2d-open-software-license-agreement_en.html.
 */

#include "LAppDelegate.hpp"
#include "BenchmarkConfig.hpp"
#include <iostream>
#include <sstream>
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <vector>
#include <mach-o/dyld.h>
#include <libgen.h>
#include <GL/glew.h>
#include <GLFW/glfw3.h>
#include "LAppView.hpp"
#include "LAppPal.hpp"
#include "LAppDefine.hpp"
#include "LAppLive2DManager.hpp"
#include "LAppTextureManager.hpp"

using namespace Csm;
using namespace std;
using namespace LAppDefine;

namespace {
    LAppDelegate* s_instance = NULL;
}

LAppDelegate* LAppDelegate::GetInstance()
{
    if (s_instance == NULL)
    {
        s_instance = new LAppDelegate();
    }

    return s_instance;
}

void LAppDelegate::ReleaseInstance()
{
    if (s_instance != NULL)
    {
        delete s_instance;
    }

    s_instance = NULL;
}

bool LAppDelegate::Initialize()
{
    if (DebugLogEnable)
    {
        LAppPal::PrintLogLn("START");
    }

    // GLFWの初期化
    if (glfwInit() == GL_FALSE)
    {
        if (DebugLogEnable)
        {
            LAppPal::PrintLogLn("Can't initilize GLFW");
        }
        return GL_FALSE;
    }

    // Windowの生成_
    // Keep the framebuffer at the requested 1280x720 on Retina displays so it
    // matches the Godot benchmark's render resolution.
    glfwWindowHint(GLFW_COCOA_RETINA_FRAMEBUFFER, GLFW_FALSE);
    _window = glfwCreateWindow(RenderTargetWidth, RenderTargetHeight, "SAMPLE", NULL, NULL);
    if (_window == NULL)
    {
        if (DebugLogEnable)
        {
            LAppPal::PrintLogLn("Can't create GLFW window.");
        }
        glfwTerminate();
        return GL_FALSE;
    }

    // Windowのコンテキストをカレントに設定
    glfwMakeContextCurrent(_window);
    glfwSwapInterval(0);

    if (glewInit() != GLEW_OK) {
        if (DebugLogEnable)
        {
            LAppPal::PrintLogLn("Can't initilize glew.");
        }
        glfwTerminate();
        return GL_FALSE;
    }

    //テクスチャサンプリング設定
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR);

    //透過設定
    glEnable(GL_BLEND);
    glBlendFunc(GL_ONE, GL_ONE_MINUS_SRC_ALPHA);

    //コールバック関数の登録
    glfwSetMouseButtonCallback(_window, EventHandler::OnMouseCallBack);
    glfwSetCursorPosCallback(_window, EventHandler::OnMouseCallBack);

    // ウィンドウサイズ記憶
    int width, height;
    glfwGetWindowSize(LAppDelegate::GetInstance()->GetWindow(), &width, &height);
    _windowWidth = width;
    _windowHeight = height;

    // Cubism SDK の初期化
    InitializeCubism();

    SetExecuteAbsolutePath();

    //load model
    LAppLive2DManager::GetInstance();

    //AppViewの初期化
    _view->Initialize(width, height);
    // Benchmark scene has no sample UI sprites; only the configured Cubism models are rendered.

    return GL_TRUE;
}

void LAppDelegate::Release()
{
    // Windowの削除
    glfwDestroyWindow(_window);

    glfwTerminate();

    delete _textureManager;
    delete _view;

    // リソースを解放
    LAppLive2DManager::ReleaseInstance();

    //Cubism SDK の解放
    CubismFramework::Dispose();
}

void LAppDelegate::Run()
{
    const double warmupSeconds = BenchmarkConfig::WarmupSeconds;
    const double sampleSeconds = BenchmarkConfig::SampleSeconds;
    const double benchmarkStart = glfwGetTime();
    double sampleStart = 0.0;
    double previousFrame = 0.0;
    std::vector<double> frameTimesMs;

    std::printf(
        "BENCHMARK_READY case=%s renderer=opengl models=%d size=%dx%d mipmaps=on warmup_s=%.1f sample_s=%.1f\n",
        BenchmarkConfig::CaseId, BenchmarkConfig::ModelCount,
        BenchmarkConfig::Width, BenchmarkConfig::Height,
        warmupSeconds, sampleSeconds);
    std::fflush(stdout);

    //メインループ
    while (glfwWindowShouldClose(_window) == GL_FALSE && !_isEnd)
    {
        int width, height;
        float scaleWidth, scaleHeight;

        glfwGetFramebufferSize(LAppDelegate::GetInstance()->GetWindow(), &width, &height);
        glfwGetWindowContentScale(LAppDelegate::GetInstance()->GetWindow(), &scaleWidth, &scaleHeight);
        if (scaleWidth == 0.0f)
        {
            scaleWidth = 1.0f;
        }
        if (scaleHeight == 0.0f)
        {
            scaleHeight = 1.0f;
        }

        if((_windowWidth!=width || _windowHeight!=height) && width>0 && height>0)
        {
            _view->Initialize(width / scaleWidth, height / scaleHeight);
            // レンダーターゲットを破棄する（次フレームで新サイズで再作成される）
            _view->DestroySpriteRenderTarget();
            // オフスクリーンのサイズ変更
            LAppLive2DManager::GetInstance()->SetRenderTargetSize(width, height);

            _windowWidth = width;
            _windowHeight = height;
        }

        // 時間更新
        LAppPal::UpdateTime();

        // 画面の初期化
        glClearColor(0.0f, 0.0f, 0.0f, 1.0f);
        glClear(GL_COLOR_BUFFER_BIT | GL_DEPTH_BUFFER_BIT);
        glClearDepth(1.0);

        //描画更新
        _view->Render();

        // バッファの入れ替え
        glfwSwapBuffers(_window);

        // Poll for and process events
        glfwPollEvents();

        const double now = glfwGetTime();
        if (sampleStart == 0.0 && now - benchmarkStart >= warmupSeconds)
        {
            sampleStart = now;
            previousFrame = now;
            std::printf("BENCHMARK_SAMPLE_BEGIN\n");
            std::fflush(stdout);
        }
        else if (sampleStart > 0.0)
        {
            frameTimesMs.push_back((now - previousFrame) * 1000.0);
            previousFrame = now;

            if (now - sampleStart >= sampleSeconds)
            {
                std::sort(frameTimesMs.begin(), frameTimesMs.end());
                const size_t frameCount = frameTimesMs.size();
                const auto percentile = [&frameTimesMs, frameCount](double fraction)
                {
                    const size_t index = std::min(
                        static_cast<size_t>(std::ceil(fraction * frameCount)) - 1,
                        frameCount - 1);
                    return frameTimesMs[index];
                };
                const double elapsed = now - sampleStart;
                std::printf(
                    "BENCHMARK_RESULT {\"schema_version\":1,\"case_id\":\"%s\","
                    "\"core_backend\":\"%s\",\"host_backend\":\"cubism-framework-native\","
                    "\"graphics_api\":\"opengl\",\"build_profile\":\"release\","
                    "\"workload_id\":\"%s\",\"model\":\"%s\",\"model_hash\":\"%s\","
                    "\"instances\":%d,\"viewport\":[%d,%d],\"texture_mipmaps\":true,"
                    "\"warmup_seconds\":%.6f,"
                    "\"sample_seconds\":%.6f,\"frames\":%zu,\"average_fps\":%.6f,"
                    "\"p50_frame_ms\":%.6f,\"p95_frame_ms\":%.6f,\"p99_frame_ms\":%.6f}\n",
                    BenchmarkConfig::CaseId, BenchmarkConfig::CoreProvider,
                    BenchmarkConfig::WorkloadId, BenchmarkConfig::ModelName, BenchmarkConfig::ModelHash,
                    BenchmarkConfig::ModelCount, BenchmarkConfig::Width, BenchmarkConfig::Height,
                    warmupSeconds, elapsed, frameCount, frameCount / elapsed,
                    percentile(0.50), percentile(0.95), percentile(0.99));
                std::fflush(stdout);
                _isEnd = true;
            }
        }
    }

    Release();

    LAppDelegate::ReleaseInstance();
}

LAppDelegate::LAppDelegate():
    _cubismOption(),
    _window(NULL),
    _captured(false),
    _mouseX(0.0f),
    _mouseY(0.0f),
    _isEnd(false),
    _windowWidth(0),
    _windowHeight(0)
{
    _executeAbsolutePath = "";
    _view = new LAppView();
    _textureManager = new LAppTextureManager();
}

LAppDelegate::~LAppDelegate()
{

}

void LAppDelegate::InitializeCubism()
{
    //setup cubism
    _cubismOption.LogFunction = LAppPal::PrintMessageLn;
    _cubismOption.LoggingLevel = LAppDefine::CubismLoggingLevel;
    _cubismOption.LoadFileFunction = LAppPal::LoadFileAsBytes;
    _cubismOption.ReleaseBytesFunction = LAppPal::ReleaseBytes;
    Csm::CubismFramework::StartUp(&_cubismAllocator, &_cubismOption);

    //Initialize cubism
    CubismFramework::Initialize();

    //default proj
    CubismMatrix44 projection;

    LAppPal::UpdateTime();
}

void LAppDelegate::OnMouseCallBack(GLFWwindow* window, int button, int action, int modify)
{
    if (_view == NULL)
    {
        return;
    }
    if (GLFW_MOUSE_BUTTON_LEFT != button)
    {
        return;
    }

    if (GLFW_PRESS == action)
    {
        _captured = true;
        _view->OnTouchesBegan(_mouseX, _mouseY);
    }
    else if (GLFW_RELEASE == action)
    {
        if (_captured)
        {
            _captured = false;
            _view->OnTouchesEnded(_mouseX, _mouseY);
        }
    }
}

void LAppDelegate::OnMouseCallBack(GLFWwindow* window, double x, double y)
{
    _mouseX = static_cast<float>(x);
    _mouseY = static_cast<float>(y);

    if (!_captured)
    {
        return;
    }
    if (_view == NULL)
    {
        return;
    }

    _view->OnTouchesMoved(_mouseX, _mouseY);
}

void LAppDelegate::SetExecuteAbsolutePath()
{
    char path[1024];
    uint32_t size = sizeof(path);
    _NSGetExecutablePath(path, &size);
    this->_executeAbsolutePath = dirname(path);
    this->_executeAbsolutePath += "/";

    LAppPal::SetExecutableAbsolutePath(this->_executeAbsolutePath);
}
