# Register this portable Editor for Open With without changing the user's default app.
param([switch]$Unregister)
$ErrorActionPreference = 'Stop'
# Register/remove previews first, while the owned ProgID still exists.
& (Join-Path $PSScriptRoot 'Register-ProjectPreview.ps1') -Unregister:$Unregister
$editorPath = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot 'zipzi-editor.exe'))
$openCommand = '"' + $editorPath + '" -- "%1"'
$classes = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Software\Classes')
$progId = 'Zipzi.Project'
try {
    if ($Unregister) {
        $current = $classes.OpenSubKey("$progId\shell\open\command")
        $owned = $false
        if ($null -ne $current) {
            try { $owned = $current.GetValue('') -eq $openCommand } finally { $current.Dispose() }
        }
        # Another moved installation may now own this ProgID. Preserve its registration.
        if ($owned) {
            $classes.DeleteSubKeyTree($progId, $false)
            $openWith = $classes.OpenSubKey('.zipzi\OpenWithProgids', $true)
            if ($null -ne $openWith) {
                try { $openWith.DeleteValue($progId, $false) } finally { $openWith.Dispose() }
            }
        }
    } else {
        if (-not [System.IO.File]::Exists($editorPath)) { throw "Editor not found: $editorPath" }
        $values = @{
            $progId = 'Zipzi Project'
            "$progId\DefaultIcon" = '"' + $editorPath + '",0'
            "$progId\shell\open\command" = $openCommand
        }
        foreach ($entry in $values.GetEnumerator()) {
            $key = $classes.CreateSubKey($entry.Key)
            try { $key.SetValue('', $entry.Value, [Microsoft.Win32.RegistryValueKind]::String) } finally { $key.Dispose() }
        }
        $openWith = $classes.CreateSubKey('.zipzi\OpenWithProgids')
        try { $openWith.SetValue($progId, '', [Microsoft.Win32.RegistryValueKind]::String) } finally { $openWith.Dispose() }
    }
} finally { $classes.Dispose() }
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class ZipziAssociationNotification {
    [DllImport("shell32.dll")]
    public static extern void SHChangeNotify(uint eventId, uint flags, IntPtr item1, IntPtr item2);
}
'@
[ZipziAssociationNotification]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)
