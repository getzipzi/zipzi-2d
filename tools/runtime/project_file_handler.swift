// Keep a temporary default handler alive only while the parent owns stdin.
// EOF restores the original handler even if the Python acceptance runner exits.
import AppKit
import CoreServices
import UniformTypeIdentifiers

let contentType = "dev.zipzi.project" as CFString
let roles = LSRolesMask.all

func handler() -> String? {
    guard let url = NSWorkspace.shared.urlForApplication(toOpen: UTType(importedAs: "dev.zipzi.project")) else {
        return nil
    }
    return Bundle(url: url)?.bundleIdentifier
}

func emit(_ value: [String: Any]) {
    let data = try! JSONSerialization.data(withJSONObject: value, options: [.sortedKeys])
    FileHandle.standardOutput.write(data + Data([10]))
}

func selectApplication(_ url: URL) throws {
    var completed = false
    var failure: Error?
    NSWorkspace.shared.setDefaultApplication(at: url, toOpen: UTType(importedAs: "dev.zipzi.project")) { error in
        failure = error
        completed = true
    }
    let deadline = Date(timeIntervalSinceNow: 25)
    while !completed && Date() < deadline {
        RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.05))
    }
    guard completed else { throw NSError(domain: "DefaultHandlerTimeout", code: 1) }
    if let failure { throw failure }
}

func unregisterBundle(_ url: URL) throws -> Int32 {
    let process = Process()
    process.executableURL = URL(fileURLWithPath:
        "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister")
    process.arguments = ["-u", url.path]
    try process.run()
    process.waitUntilExit()
    return process.terminationStatus
}

guard CommandLine.arguments.count >= 2 else { fatalError("Expected a temporary application bundle") }
if CommandLine.arguments[1] == "--query" {
    let applications = CommandLine.arguments.dropFirst(2).map {
        NSWorkspace.shared.urlForApplication(toOpen: URL(fileURLWithPath: $0))?.path ?? ""
    }
    emit(["active": handler() ?? "", "applications": applications])
    exit(0)
}
let bundleURL = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
guard let bundle = Bundle(url: bundleURL), let identity = bundle.bundleIdentifier,
      identity.hasPrefix("dev.zipzi.native-acceptance."),
      bundleURL.lastPathComponent == "Zipzi Native Acceptance.app",
      let original = handler(), original != identity else {
    fatalError("An existing default handler is required for reversible acceptance")
}
let registration = LSRegisterURL(bundleURL as CFURL, true)
guard registration == noErr else { fatalError("LSRegisterURL: \(registration)") }
do { try selectApplication(bundleURL) }
catch {
    _ = LSSetDefaultRoleHandlerForContentType(contentType, roles, original as CFString)
    _ = try? unregisterBundle(bundleURL)
    try? FileManager.default.removeItem(at: bundleURL)
    throw error
}
emit(["previous": original, "temporary": identity, "active": handler() ?? ""])

while readLine() != nil {}

// Preserve an intervening user choice instead of silently overwriting it.
var restore: OSStatus = noErr
if handler() == identity {
    restore = LSSetDefaultRoleHandlerForContentType(contentType, roles, original as CFString)
} else if handler() != original { restore = -1 }
// Removal also restores the inherited handler when no explicit user override
// existed. LaunchServices queries within this process can retain stale results.
let unregisterStatus = try unregisterBundle(bundleURL)
// The guardian also removes the owned throwaway bundle if its parent crashes,
// so Spotlight cannot register the Owner-ranked application again later.
try FileManager.default.removeItem(at: bundleURL)
emit(["restore_status": restore, "active": handler() ?? "", "expected": original])
if unregisterStatus != 0 { exit(1) }
