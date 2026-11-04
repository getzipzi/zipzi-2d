// Native COM smoke test without registry mutation or an Explorer restart.
#include <windows.h>
#include <shobjidl.h>
#include <shlwapi.h>
#include <thumbcache.h>
#include <wrl/client.h>
#include <cstdio>
#include <string>
using Microsoft::WRL::ComPtr;
static const CLSID ClassId = {0xb7666151, 0x0ad8, 0x4b22, {0x82, 0x9e, 0x46, 0x40, 0xcb, 0x90, 0xd3, 0xbe}};
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
            auto first = static_cast<const BYTE *>(data.bmBits);
            REQUIRE(first && first[0] == 0 && first[1] == 0 && first[2] == 255 && first[3] == 255);
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

int wmain(int argc, wchar_t **argv) {
    if (argc != 3 || FAILED(CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED))) return 2;
    HMODULE module = LoadLibraryW(argv[1]);
    REQUIRE(module != nullptr);
    auto factoryFunction = reinterpret_cast<HRESULT (WINAPI *)(REFCLSID, REFIID, void **)>(
        GetProcAddress(module, "DllGetClassObject"));
    auto canUnload = reinterpret_cast<HRESULT (WINAPI *)()>(GetProcAddress(module, "DllCanUnloadNow"));
    REQUIRE(factoryFunction && canUnload);
    ComPtr<IClassFactory> factory;
    REQUIRE(SUCCEEDED(factoryFunction(ClassId, IID_PPV_ARGS(&factory))));
    REQUIRE(canUnload() == S_FALSE);
    int result = Test(factory.Get(), argv[2]);
    factory.Reset();
    REQUIRE(canUnload() == S_OK);
    FreeLibrary(module);
    CoUninitialize();
    if (!result) std::puts("PASS Windows native COM: thumbnail pixels, preview resize/unload, absence and corrupt fallback");
    return result;
}
