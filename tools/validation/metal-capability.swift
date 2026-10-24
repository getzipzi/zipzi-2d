// GitHub Actions output; failure to run the probe is an error, not a missing device.
import Metal
print("available=\(MTLCreateSystemDefaultDevice() != nil ? "true" : "false")")
