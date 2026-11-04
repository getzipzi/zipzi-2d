# Per-user Shell registration. Explorer/prevhost instantiate the DLL from IStream.
param([switch]$Unregister)
$ErrorActionPreference = 'Stop'
$previewPath = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot 'ZipziProjectPreview.dll'))
$classId = '{B7666151-0AD8-4B22-829E-4640CB90D3BE}'
$thumbnailId = '{E357FCCD-A995-4576-B01F-234630154E96}'
$previewId = '{8895B1C6-B41F-4C1C-A562-0D564250836F}'
$classes = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Software\Classes')
$handlers = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Software\Microsoft\Windows\CurrentVersion\PreviewHandlers')
try {
    if ($Unregister) {
        $current = $classes.OpenSubKey("CLSID\$classId\InprocServer32")
        $owned = $false
        if ($null -ne $current) {
            try { $owned = $current.GetValue('') -eq $previewPath } finally { $current.Dispose() }
        }
        if ($owned) {
            foreach ($type in @('.zipzi', 'Zipzi.Project')) {
                foreach ($handler in @($thumbnailId, $previewId)) {
                    $path = "$type\shellex\$handler"
                    $key = $classes.OpenSubKey($path)
                    $matches = $false
                    if ($null -ne $key) {
                        try { $matches = $key.GetValue('') -eq $classId } finally { $key.Dispose() }
                    }
                    if ($matches) { $classes.DeleteSubKeyTree($path, $false) }
                }
            }
            $classes.DeleteSubKeyTree("CLSID\$classId", $false)
            $handlers.DeleteValue($classId, $false)
        }
    } else {
        if (-not [Environment]::Is64BitProcess) { throw 'Use 64-bit PowerShell to register the x64 Shell provider' }
        if (-not [System.IO.File]::Exists($previewPath)) { throw "Preview provider not found: $previewPath" }
        $key = $classes.CreateSubKey("CLSID\$classId")
        try {
            $key.SetValue('', 'Zipzi Project Preview')
            # Microsoft's native 64-bit low integrity preview surrogate.
            $key.SetValue('AppID', '{6D2B5079-2F0B-48DD-AB7F-97CEC514D30B}')
        } finally { $key.Dispose() }
        $key = $classes.CreateSubKey("CLSID\$classId\InprocServer32")
        try {
            $key.SetValue('', $previewPath)
            $key.SetValue('ThreadingModel', 'Apartment')
        } finally { $key.Dispose() }
        foreach ($type in @('.zipzi', 'Zipzi.Project')) {
            foreach ($handler in @($thumbnailId, $previewId)) {
                $key = $classes.CreateSubKey("$type\shellex\$handler")
                try { $key.SetValue('', $classId) } finally { $key.Dispose() }
            }
        }
        $handlers.SetValue($classId, 'Zipzi Project Preview')
    }
} finally { $handlers.Dispose(); $classes.Dispose() }
