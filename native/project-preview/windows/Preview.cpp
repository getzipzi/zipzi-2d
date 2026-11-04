// Read-only, stream-initialized Windows Shell thumbnail / preview provider.
#include <windows.h>
#include <shobjidl.h>
#include <thumbcache.h>
#include <wincodec.h>
#include <wrl/client.h>
#include <algorithm>
#include <atomic>
#include <new>
#include <vector>
#include "zipzi_preview.h"

using Microsoft::WRL::ComPtr;
static const CLSID CLSID_ZipziPreview = {0xb7666151, 0x0ad8, 0x4b22, {0x82, 0x9e, 0x46, 0x40, 0xcb, 0x90, 0xd3, 0xbe}};
static std::atomic<long> objects{0}, locks{0};
static HINSTANCE instance;

static intptr_t ReadAt(void *context, uint64_t offset, uint8_t *bytes, size_t count) {
    auto stream = static_cast<IStream *>(context);
    LARGE_INTEGER position; position.QuadPart = static_cast<LONGLONG>(offset);
    if (FAILED(stream->Seek(position, STREAM_SEEK_SET, nullptr))) return -1;
    ULONG actual = 0;
    if (FAILED(stream->Read(bytes, static_cast<ULONG>(count), &actual))) return -1;
    return static_cast<intptr_t>(actual);
}

class Preview final : public IInitializeWithStream, public IThumbnailProvider,
                      public IPreviewHandler, public IOleWindow, public IObjectWithSite,
                      public IPreviewHandlerVisuals {
    std::atomic<ULONG> refs{1};
    ComPtr<IStream> stream;
    ComPtr<IUnknown> site;
    HWND parent = nullptr, window = nullptr;
    RECT rect{};
    HBITMAP bitmap = nullptr;
    UINT width = 0, height = 0;
    COLORREF background = GetSysColor(COLOR_WINDOW);

    HRESULT Load() {
        if (loaded) return S_OK;
        if (!stream) return E_UNEXPECTED;
        STATSTG info{};
        HRESULT hr = stream->Stat(&info, STATFLAG_NONAME);
        if (FAILED(hr)) return hr;
        std::vector<uint8_t> bytes(ZIPZI_PREVIEW_CAPACITY);
        auto size = zipzi_preview_read(stream.Get(), ReadAt, info.cbSize.QuadPart, bytes.data(), bytes.size());
        if (size <= 0) return HRESULT_FROM_WIN32(ERROR_NOT_FOUND);
        ComPtr<IWICImagingFactory> factory;
        hr = CoCreateInstance(CLSID_WICImagingFactory, nullptr, CLSCTX_INPROC_SERVER, IID_PPV_ARGS(&factory));
        if (FAILED(hr)) return hr;
        ComPtr<IWICStream> input;
        hr = factory->CreateStream(&input);
        if (FAILED(hr)) return hr;
        hr = input->InitializeFromMemory(bytes.data(), static_cast<DWORD>(size));
        if (FAILED(hr)) return hr;
        ComPtr<IWICBitmapDecoder> decoder;
        hr = factory->CreateDecoderFromStream(input.Get(), nullptr, WICDecodeMetadataCacheOnLoad, &decoder);
        if (FAILED(hr)) return hr;
        ComPtr<IWICBitmapFrameDecode> source;
        hr = decoder->GetFrame(0, &source);
        if (FAILED(hr)) return hr;
        // Cache a decoded bitmap: WIC's decoder can lazily retain its input buffer.
        ComPtr<IWICBitmap> cached;
        hr = factory->CreateBitmapFromSource(source.Get(), WICBitmapCacheOnLoad, &cached);
        if (FAILED(hr)) return hr;
        // All later copies use this independent source, after the project closes.
        pixels = cached;
        hr = cached->GetSize(&width, &height);
        if (FAILED(hr) || !width || !height || width > 1024 || height > 1024) return E_FAIL;
        stream.Reset();
        loaded = true;
        return S_OK;
    }
    ComPtr<IWICBitmapSource> pixels;
    bool loaded = false;

    HRESULT Bitmap(UINT edge, HBITMAP *output) {
        if (!loaded) { HRESULT hr = Load(); if (FAILED(hr)) return hr; }
        UINT bound = (std::max)(1u, (std::min)(edge, 1024u));
        double scale = (std::min)(double(bound) / width, double(bound) / height);
        UINT w = (std::max)(1u, UINT(width * scale)), h = (std::max)(1u, UINT(height * scale));
        ComPtr<IWICImagingFactory> factory;
        HRESULT hr = CoCreateInstance(CLSID_WICImagingFactory, nullptr, CLSCTX_INPROC_SERVER, IID_PPV_ARGS(&factory));
        if (FAILED(hr)) return hr;
        ComPtr<IWICBitmapScaler> scaler;
        hr = factory->CreateBitmapScaler(&scaler);
        if (FAILED(hr)) return hr;
        hr = scaler->Initialize(pixels.Get(), w, h, WICBitmapInterpolationModeFant);
        if (FAILED(hr)) return hr;
        ComPtr<IWICFormatConverter> converter;
        hr = factory->CreateFormatConverter(&converter);
        if (FAILED(hr)) return hr;
        hr = converter->Initialize(scaler.Get(), GUID_WICPixelFormat32bppPBGRA,
                                   WICBitmapDitherTypeNone, nullptr, 0, WICBitmapPaletteTypeCustom);
        if (FAILED(hr)) return hr;
        BITMAPINFO info{};
        info.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
        info.bmiHeader.biWidth = static_cast<LONG>(w);
        info.bmiHeader.biHeight = -static_cast<LONG>(h);
        info.bmiHeader.biPlanes = 1; info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        void *data = nullptr;
        HBITMAP result = CreateDIBSection(nullptr, &info, DIB_RGB_COLORS, &data, nullptr, 0);
        if (!result) return E_OUTOFMEMORY;
        hr = converter->CopyPixels(nullptr, w * 4, w * h * 4, static_cast<BYTE *>(data));
        if (FAILED(hr)) { DeleteObject(result); return hr; }
        *output = result;
        return S_OK;
    }

    static LRESULT CALLBACK WindowProc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp) {
        if (msg == WM_NCCREATE) {
            auto data = reinterpret_cast<CREATESTRUCTW *>(lp);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, reinterpret_cast<LONG_PTR>(data->lpCreateParams));
        }
        auto self = reinterpret_cast<Preview *>(GetWindowLongPtrW(hwnd, GWLP_USERDATA));
        if (self && msg == WM_PAINT) {
            PAINTSTRUCT paint; HDC dc = BeginPaint(hwnd, &paint);
            RECT r; GetClientRect(hwnd, &r);
            HBRUSH brush = CreateSolidBrush(self->background);
            FillRect(dc, &r, brush); DeleteObject(brush);
            if (self->bitmap && r.right > 0 && r.bottom > 0) {
                BITMAP b; GetObjectW(self->bitmap, sizeof(b), &b);
                double scale = (std::min)(double(r.right) / b.bmWidth, double(r.bottom) / b.bmHeight);
                int w = (std::max)(1, int(b.bmWidth * scale)), h = (std::max)(1, int(b.bmHeight * scale));
                HDC source = CreateCompatibleDC(dc);
                auto old = SelectObject(source, self->bitmap);
                BLENDFUNCTION blend{AC_SRC_OVER, 0, 255, AC_SRC_ALPHA};
                AlphaBlend(dc, (r.right - w) / 2, (r.bottom - h) / 2, w, h,
                           source, 0, 0, b.bmWidth, b.bmHeight, blend);
                SelectObject(source, old); DeleteDC(source);
            }
            EndPaint(hwnd, &paint); return 0;
        }
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
public:
    Preview() { ++objects; }
    ~Preview() { Unload(); --objects; }
    IFACEMETHODIMP QueryInterface(REFIID iid, void **out) override {
        if (!out) return E_POINTER;
        *out = nullptr;
        if (iid == IID_IUnknown || iid == IID_IPreviewHandler) *out = static_cast<IPreviewHandler *>(this);
        else if (iid == IID_IInitializeWithStream) *out = static_cast<IInitializeWithStream *>(this);
        else if (iid == IID_IThumbnailProvider) *out = static_cast<IThumbnailProvider *>(this);
        else if (iid == IID_IOleWindow) *out = static_cast<IOleWindow *>(this);
        else if (iid == IID_IObjectWithSite) *out = static_cast<IObjectWithSite *>(this);
        else if (iid == IID_IPreviewHandlerVisuals) *out = static_cast<IPreviewHandlerVisuals *>(this);
        else return E_NOINTERFACE;
        AddRef(); return S_OK;
    }
    IFACEMETHODIMP_(ULONG) AddRef() override { return ++refs; }
    IFACEMETHODIMP_(ULONG) Release() override { ULONG n = --refs; if (!n) delete this; return n; }
    IFACEMETHODIMP Initialize(IStream *input, DWORD) override {
        if (!input) return E_INVALIDARG;
        if (stream || loaded) return HRESULT_FROM_WIN32(ERROR_ALREADY_INITIALIZED);
        stream = input; return S_OK;
    }
    IFACEMETHODIMP GetThumbnail(UINT edge, HBITMAP *out, WTS_ALPHATYPE *alpha) override {
        if (!out || !alpha) return E_POINTER;
        *out = nullptr; *alpha = WTSAT_UNKNOWN;
        try { HRESULT hr = Bitmap(edge, out); if (SUCCEEDED(hr)) *alpha = WTSAT_ARGB; return hr; }
        catch (...) { return E_OUTOFMEMORY; }
    }
    IFACEMETHODIMP SetWindow(HWND hwnd, const RECT *area) override {
        if (!hwnd || !area) return E_INVALIDARG;
        parent = hwnd; rect = *area;
        if (window) { SetParent(window, parent); return SetRect(area); }
        return S_OK;
    }
    IFACEMETHODIMP SetRect(const RECT *area) override {
        if (!area) return E_POINTER;
        rect = *area;
        if (window) {
            MoveWindow(window, rect.left, rect.top, (std::max)(0L, rect.right - rect.left),
                       (std::max)(0L, rect.bottom - rect.top), TRUE);
            InvalidateRect(window, nullptr, TRUE);
        }
        return S_OK;
    }
    IFACEMETHODIMP DoPreview() override {
        if (!parent || window) return E_UNEXPECTED;
        try {
            HRESULT hr = Bitmap(1024, &bitmap); if (FAILED(hr)) return hr;
            WNDCLASSW cls{}; cls.lpfnWndProc = WindowProc; cls.hInstance = instance;
            cls.lpszClassName = L"ZipziStaticProjectPreview"; cls.hCursor = LoadCursor(nullptr, IDC_ARROW);
            if (!RegisterClassW(&cls) && GetLastError() != ERROR_CLASS_ALREADY_EXISTS)
                return HRESULT_FROM_WIN32(GetLastError());
            window = CreateWindowExW(0, cls.lpszClassName, L"", WS_CHILD | WS_VISIBLE,
                rect.left, rect.top, (std::max)(0L, rect.right - rect.left),
                (std::max)(0L, rect.bottom - rect.top), parent, nullptr, instance, this);
            return window ? S_OK : HRESULT_FROM_WIN32(GetLastError());
        } catch (...) { return E_OUTOFMEMORY; }
    }
    IFACEMETHODIMP Unload() override {
        if (window) { DestroyWindow(window); window = nullptr; }
        if (bitmap) { DeleteObject(bitmap); bitmap = nullptr; }
        pixels.Reset(); stream.Reset(); loaded = false; width = height = 0;
        return S_OK;
    }
    IFACEMETHODIMP SetFocus() override { if (window) ::SetFocus(window); return S_OK; }
    IFACEMETHODIMP QueryFocus(HWND *out) override { if (!out) return E_POINTER; *out = GetFocus(); return S_OK; }
    IFACEMETHODIMP TranslateAccelerator(MSG *message) override {
        ComPtr<IPreviewHandlerFrame> host;
        return site && SUCCEEDED(site.As(&host)) ? host->TranslateAccelerator(message) : S_FALSE;
    }
    IFACEMETHODIMP GetWindow(HWND *out) override { if (!out) return E_POINTER; *out = window; return S_OK; }
    IFACEMETHODIMP ContextSensitiveHelp(BOOL) override { return E_NOTIMPL; }
    IFACEMETHODIMP SetSite(IUnknown *value) override { site = value; return S_OK; }
    IFACEMETHODIMP GetSite(REFIID iid, void **out) override {
        if (!out) return E_POINTER; *out = nullptr; return site ? site->QueryInterface(iid, out) : E_FAIL;
    }
    IFACEMETHODIMP SetBackgroundColor(COLORREF color) override {
        background = color; if (window) InvalidateRect(window, nullptr, TRUE); return S_OK;
    }
    IFACEMETHODIMP SetFont(const LOGFONTW *) override { return S_OK; }
    IFACEMETHODIMP SetTextColor(COLORREF) override { return S_OK; }
};

class Factory final : public IClassFactory {
    std::atomic<ULONG> refs{1};
public:
    Factory() { ++objects; }
    ~Factory() { --objects; }
    IFACEMETHODIMP QueryInterface(REFIID iid, void **out) override {
        if (!out) return E_POINTER; *out = nullptr;
        if (iid != IID_IUnknown && iid != IID_IClassFactory) return E_NOINTERFACE;
        *out = static_cast<IClassFactory *>(this); AddRef(); return S_OK;
    }
    IFACEMETHODIMP_(ULONG) AddRef() override { return ++refs; }
    IFACEMETHODIMP_(ULONG) Release() override { ULONG n = --refs; if (!n) delete this; return n; }
    IFACEMETHODIMP CreateInstance(IUnknown *outer, REFIID iid, void **out) override {
        if (!out) return E_POINTER; *out = nullptr;
        if (outer) return CLASS_E_NOAGGREGATION;
        auto object = new (std::nothrow) Preview;
        if (!object) return E_OUTOFMEMORY;
        HRESULT hr = object->QueryInterface(iid, out); object->Release(); return hr;
    }
    IFACEMETHODIMP LockServer(BOOL lock) override { lock ? ++locks : --locks; return S_OK; }
};

extern "C" HRESULT __stdcall DllGetClassObject(REFCLSID clsid, REFIID iid, void **out) {
    if (!out) return E_POINTER; *out = nullptr;
    if (clsid != CLSID_ZipziPreview) return CLASS_E_CLASSNOTAVAILABLE;
    auto factory = new (std::nothrow) Factory;
    if (!factory) return E_OUTOFMEMORY;
    HRESULT hr = factory->QueryInterface(iid, out); factory->Release(); return hr;
}
extern "C" HRESULT __stdcall DllCanUnloadNow() { return objects == 0 && locks == 0 ? S_OK : S_FALSE; }
BOOL WINAPI DllMain(HINSTANCE value, DWORD reason, LPVOID) {
    if (reason == DLL_PROCESS_ATTACH) { instance = value; DisableThreadLibraryCalls(value); }
    return TRUE;
}
