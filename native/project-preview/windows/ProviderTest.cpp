// Direct DLL checks, or read-only activation checks against an installed provider.
#include <windows.h>
#include <shobjidl.h>
#include <shlwapi.h>
#include <thumbcache.h>
#include <wrl/client.h>
#include <cstdio>
#include <filesystem>
#include <string>
#include <vector>
using Microsoft::WRL::ComPtr;
static const CLSID ClassId = {0xb7666151, 0x0ad8, 0x4b22, {0x82, 0x9e, 0x46, 0x40, 0xcb, 0x90, 0xd3, 0xbe}};
using CanUnload = HRESULT (WINAPI *)();
#define REQUIRE(expression) do { if (!(expression)) { std::fprintf(stderr, "FAIL line %d: %s\n", __LINE__, #expression); return 1; } } while (0)

static int Test(IClassFactory *factory, const std::wstring &directory) {
    for (auto variant : {L"image", L"absent", L"corrupt"}) {
        bool valid = std::wstring(variant) == L"image";
        ComPtr<IInitializeWithStream> initialize;
        REQUIRE(SUCCEEDED(factory->CreateInstance(nullptr, IID_PPV_ARGS(&initialize))));
        ComPtr<IStream> input;
        std::wstring path = directory + L"\\" + variant + L".zipzi";
        REQUIRE(SUCCEEDED(SHCreateStreamOnFileEx(path.c_str(), STGM_READ | STGM_SHARE_DENY_NONE,
            FILE_ATTRIBUTE_NORMAL, FALSE, nullptr, &input)));
        REQUIRE(SUCCEEDED(initialize->Initialize(input.Get(), STGM_READ)));
        input.Reset();
        ComPtr<IThumbnailProvider> thumbnail;
        REQUIRE(SUCCEEDED(initialize.As(&thumbnail)));
        HBITMAP bitmap = nullptr;
        WTS_ALPHATYPE alpha = WTSAT_UNKNOWN;
        HRESULT hr = thumbnail->GetThumbnail(256, &bitmap, &alpha);
        REQUIRE(SUCCEEDED(hr) == valid);
        if (valid) {
            BITMAP data{};
            REQUIRE(GetObjectW(bitmap, sizeof(data), &data));
            REQUIRE(data.bmWidth == 256 && data.bmHeight == 128 && alpha == WTSAT_ARGB);
            // A cross-process HBITMAP can be marshalled as a DDB with no bmBits.
            // Read pixels through GDI for both direct and isolated activation.
            BITMAPINFO info{};
            info.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
            info.bmiHeader.biWidth = data.bmWidth;
            info.bmiHeader.biHeight = -data.bmHeight;
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB;
            std::vector<BYTE> pixels(data.bmWidth * data.bmHeight * 4);
            HDC dc = CreateCompatibleDC(nullptr);
            REQUIRE(dc != nullptr);
            auto lines = GetDIBits(dc, bitmap, 0, data.bmHeight, pixels.data(), &info, DIB_RGB_COLORS);
            DeleteDC(dc);
            REQUIRE(lines == data.bmHeight);
            REQUIRE(pixels[0] == 0 && pixels[1] == 0 && pixels[2] == 255);
            if (data.bmBits) REQUIRE(static_cast<const BYTE *>(data.bmBits)[3] == 255);
            DeleteObject(bitmap);
        }
        ComPtr<IPreviewHandler> preview;
        REQUIRE(SUCCEEDED(initialize.As(&preview)));
        HWND parent = CreateWindowExW(0, L"STATIC", L"", WS_POPUP, 0, 0, 400, 300,
                                      nullptr, nullptr, GetModuleHandleW(nullptr), nullptr);
        REQUIRE(parent != nullptr);
        RECT rect{0, 0, 400, 300};
        REQUIRE(SUCCEEDED(preview->SetWindow(parent, &rect)));
        REQUIRE(SUCCEEDED(preview->DoPreview()) == valid);
        if (valid) {
            ComPtr<IOleWindow> window;
            REQUIRE(SUCCEEDED(initialize.As(&window)));
            HWND child = nullptr;
            REQUIRE(SUCCEEDED(window->GetWindow(&child)) && IsWindow(child));
            rect = {10, 20, 200, 160};
            REQUIRE(SUCCEEDED(preview->SetRect(&rect)));
            UpdateWindow(child);
            REQUIRE(SUCCEEDED(preview->Unload()) && !IsWindow(child));
        }
        REQUIRE(SUCCEEDED(preview->Unload()));
        DestroyWindow(parent);
    }
    return 0;
}

static int TestMultiplePreviews(IClassFactory *factory, const std::wstring &directory,
                               HMODULE module, CanUnload canUnload) {
    HWND parent = CreateWindowExW(0, L"STATIC", L"", WS_POPUP, 0, 0, 400, 300,
                                  nullptr, nullptr, GetModuleHandleW(nullptr), nullptr);
    REQUIRE(parent != nullptr);
    ComPtr<IInitializeWithStream> initialize[2];
    ComPtr<IPreviewHandler> preview[2];
    HWND child[2]{};
    for (int index = 0; index < 2; ++index) {
        REQUIRE(SUCCEEDED(factory->CreateInstance(nullptr, IID_PPV_ARGS(&initialize[index]))));
        ComPtr<IStream> input;
        std::wstring path = directory + L"\\image.zipzi";
        REQUIRE(SUCCEEDED(SHCreateStreamOnFileEx(path.c_str(), STGM_READ | STGM_SHARE_DENY_NONE,
            FILE_ATTRIBUTE_NORMAL, FALSE, nullptr, &input)));
        REQUIRE(SUCCEEDED(initialize[index]->Initialize(input.Get(), STGM_READ)));
        REQUIRE(SUCCEEDED(initialize[index].As(&preview[index])));
        RECT rect{0, 0, 400, 300};
        REQUIRE(SUCCEEDED(preview[index]->SetWindow(parent, &rect)));
        REQUIRE(SUCCEEDED(preview[index]->DoPreview()));
        ComPtr<IOleWindow> window;
        REQUIRE(SUCCEEDED(initialize[index].As(&window)));
        REQUIRE(SUCCEEDED(window->GetWindow(&child[index])) && IsWindow(child[index]));
    }
    REQUIRE(canUnload() == S_FALSE);
    REQUIRE(SUCCEEDED(preview[0]->Unload()) && !IsWindow(child[0]));
    preview[0].Reset(); initialize[0].Reset();
    WNDCLASSW cls{};
    REQUIRE(GetClassInfoW(module, L"ZipziStaticProjectPreview", &cls));
    REQUIRE(canUnload() == S_FALSE && IsWindow(child[1]));
    UpdateWindow(child[1]);
    REQUIRE(SUCCEEDED(preview[1]->Unload()) && !IsWindow(child[1]));
    preview[1].Reset(); initialize[1].Reset();
    DestroyWindow(parent);
    return 0;
}

int wmain(int argc, wchar_t **argv) {
    if (argc != 3 || FAILED(CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED))) return 2;
    const auto directory = std::filesystem::absolute(argv[2]).make_preferred().wstring();
    if (std::wstring(argv[1]) == L"--registered") {
        ComPtr<IClassFactory> factory;
        REQUIRE(SUCCEEDED(CoGetClassObject(ClassId, CLSCTX_LOCAL_SERVER, nullptr,
                                          IID_PPV_ARGS(&factory))));
        REQUIRE(Test(factory.Get(), directory) == 0);
        ComPtr<IShellItemImageFactory> image;
        std::wstring path = directory + L"\\image.zipzi";
        REQUIRE(SUCCEEDED(SHCreateItemFromParsingName(path.c_str(), nullptr, IID_PPV_ARGS(&image))));
        HBITMAP thumbnail = nullptr;
        HRESULT result = image->GetImage({256, 256}, SIIGBF_THUMBNAILONLY, &thumbnail);
        if (FAILED(result)) std::fprintf(stderr, "Shell GetImage failed: 0x%08lX\n", static_cast<unsigned long>(result));
        REQUIRE(SUCCEEDED(result));
        REQUIRE(thumbnail != nullptr);
        DeleteObject(thumbnail);
        image.Reset();
        factory.Reset();
        CoUninitialize();
        std::puts("PASS Windows registered COM: Shell thumbnail lookup, isolated activation, pixels, resize, unload and fallback");
        return 0;
    }
    for (int cycle = 0; cycle < 3; ++cycle) {
        HMODULE module = LoadLibraryW(argv[1]);
        REQUIRE(module != nullptr);
        auto factoryFunction = reinterpret_cast<HRESULT (WINAPI *)(REFCLSID, REFIID, void **)>(
            GetProcAddress(module, "DllGetClassObject"));
        auto canUnload = reinterpret_cast<CanUnload>(GetProcAddress(module, "DllCanUnloadNow"));
        REQUIRE(factoryFunction && canUnload);
        ComPtr<IClassFactory> factory;
        REQUIRE(SUCCEEDED(factoryFunction(ClassId, IID_PPV_ARGS(&factory))));
        REQUIRE(canUnload() == S_FALSE);
        REQUIRE(Test(factory.Get(), directory) == 0);
        REQUIRE(TestMultiplePreviews(factory.Get(), directory, module, canUnload) == 0);
        // A surviving native window must veto unloading even with no COM owners.
        HWND remaining = CreateWindowExW(0, L"ZipziStaticProjectPreview", L"", WS_POPUP,
            0, 0, 100, 100, nullptr, nullptr, module, nullptr);
        REQUIRE(remaining != nullptr);
        factory.Reset();
        REQUIRE(canUnload() == S_FALSE);
        REQUIRE(DestroyWindow(remaining));
        REQUIRE(canUnload() == S_OK);
        WNDCLASSW cls{};
        REQUIRE(!GetClassInfoW(module, L"ZipziStaticProjectPreview", &cls));
        REQUIRE(FreeLibrary(module));
    }
    CoUninitialize();
    std::puts("PASS Windows native COM: pixels, fallback, overlapping previews and class retirement across DLL reloads");
    return 0;
}
