import Cocoa
// usage: wininfo PID  -> prints JSON: windows of PID on screen (layer 0), their z-index among all on-screen layer-0 windows, and the frontmost app pid
let pid = Int32(CommandLine.arguments[1])!
let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as! [[String: Any]]
var z = 0
var out: [[String: Any]] = []
var above: [Int] = []
for w in list {
    let layer = w[kCGWindowLayer as String] as? Int ?? -1
    if layer != 0 { continue }
    let owner = w[kCGWindowOwnerPID as String] as? Int32 ?? -1
    let b = w[kCGWindowBounds as String] as? [String: Any] ?? [:]
    if owner == pid {
        out.append(["z": z, "bounds": b, "abovePids": above])
    } else { above.append(Int(owner)) }
    z += 1
}
let front = NSWorkspace.shared.frontmostApplication?.processIdentifier ?? -1
let d: [String: Any] = ["pid": pid, "frontmost": front, "windows": out]
let data = try! JSONSerialization.data(withJSONObject: d)
print(String(data: data, encoding: .utf8)!)
