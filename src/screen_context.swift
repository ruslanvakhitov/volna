import Foundation
import AppKit
import Vision
import CoreML
import ScreenCaptureKit
import ImageIO

private struct OCRResult: Codable {
    var text = ""
    var elapsed_ms: UInt64 = 0
    var error: String? = nil
    var screenshot_jpeg: String? = nil
    var screenshot_note: String? = nil
}

private func encode(_ result: OCRResult) -> UnsafeMutablePointer<CChar>? {
    guard let bytes = try? JSONEncoder().encode(result), let json = String(data: bytes, encoding: .utf8) else { return nil }
    return strdup(json)
}

private func recognize(_ image: CGImage) throws -> OCRResult {
    let started = Date()
    let request = VNRecognizeTextRequest()
    // Vision's fast recognizer lacks Cyrillic on some macOS versions. Its
    // accurate recognizer supports Russian and still runs locally in parallel.
    request.recognitionLevel = .accurate
    let supported = try request.supportedRecognitionLanguages()
    request.recognitionLanguages = ["ru-RU", "en-US"].filter { supported.contains($0) }
    request.automaticallyDetectsLanguage = false
    request.usesLanguageCorrection = false
    request.minimumTextHeight = 0.006
    // Automatic routing stalled in ANE model loading on the live Mac.
    // Explicit GPU routing read the same frame in 450 ms; keep CPU fallback.
    for (stage, devices) in try request.supportedComputeStageDevices {
        let preferred = devices.first { if case .gpu = $0 { return true }; return false }
            ?? devices.first { if case .cpu = $0 { return true }; return false }
        if let preferred = preferred { request.setComputeDevice(preferred, for: stage) }
    }
    try VNImageRequestHandler(cgImage: image, options: [:]).perform([request])
    let text = (request.results ?? []).compactMap { observation -> String? in
        guard let value = observation.topCandidates(1).first, value.confidence >= 0.35 else { return nil }
        return value.string
    }.joined(separator: "\n")
    return OCRResult(text: String(text.prefix(16000)), elapsed_ms: UInt64(Date().timeIntervalSince(started) * 1000))
}

private final class ResultBox: @unchecked Sendable {
    let lock = NSLock()
    var result = OCRResult(error: "Screen reader timed out; using the app dictionary")
    func set(_ value: OCRResult) { lock.lock(); result = value; lock.unlock() }
    func get() -> OCRResult { lock.lock(); defer { lock.unlock() }; return result }
}

@_cdecl("volna_screen_permission")
public func volnaScreenPermission(_ request: Bool) -> Bool {
    if CGPreflightScreenCaptureAccess() { return true }
    return request ? CGRequestScreenCaptureAccess() : false
}

@_cdecl("volna_screen_capture")
public func volnaScreenCapture(_ expectedPID: Int32) -> UnsafeMutablePointer<CChar>? {
    guard CGPreflightScreenCaptureAccess() else {
        return encode(OCRResult(error: "Allow Volna in System Settings > Privacy & Security > Screen Recording"))
    }
    let done = DispatchSemaphore(value: 0)
    let box = ResultBox()
    Task.detached(priority: .userInitiated) {
        defer { done.signal() }
        do {
            box.set(OCRResult(error: "Screen enumeration timed out"))
            let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
            guard NSWorkspace.shared.frontmostApplication?.processIdentifier == expectedPID else {
                box.set(OCRResult(error: "App changed before the screenshot; using its saved dictionary")); return
            }
            let window = content.windows.filter { $0.owningApplication?.processID == expectedPID && $0.windowLayer == 0 }
                .max { $0.frame.width * $0.frame.height < $1.frame.width * $1.frame.height }
            let cursor = NSEvent.mouseLocation
            let display = content.displays.max { a,b in
                if let window = window {
                    let x = a.frame.intersection(window.frame), y = b.frame.intersection(window.frame)
                    return x.width * x.height < y.width * y.height
                }
                return !a.frame.contains(cursor) && b.frame.contains(cursor)
            }
            guard let display = display else { box.set(OCRResult(error: "No screen available")); return }
            let own = content.applications.filter { $0.processID == ProcessInfo.processInfo.processIdentifier }
            let filter = SCContentFilter(display: display, excludingApplications: own, exceptingWindows: [])
            let config = SCStreamConfiguration()
            let scale = min(2.0, 3200.0 / display.frame.width)
            config.width = Int(display.frame.width * scale)
            config.height = Int(display.frame.height * scale)
            config.showsCursor = false
            box.set(OCRResult(error: "Screen capture timed out"))
            let image = try await SCScreenshotManager.captureImage(contentFilter: filter, configuration: config)
            // Keep a local JPEG preview of the image OCR saw for this dictation.
            let data = NSMutableData()
            var jpeg: String? = nil
            if let destination = CGImageDestinationCreateWithData(data, "public.jpeg" as CFString, 1, nil) {
                CGImageDestinationAddImage(destination, image, [kCGImageDestinationLossyCompressionQuality: 0.78] as CFDictionary)
                if CGImageDestinationFinalize(destination), data.length <= 6 * 1024 * 1024 {
                    jpeg = (data as Data).base64EncodedString()
                }
            }
            // Vision runs in a short-lived OCR process on a decoded CPU image.
            // Do not run its synchronous ML request on the capture Task executor.
            var result = OCRResult(screenshot_jpeg: jpeg)
            if jpeg == nil { result.error = "Screenshot could not be encoded or exceeded 6 MiB" }
            box.set(result)
        } catch { box.set(OCRResult(error: "Could not read screen text; check Screen Recording permission")) }
    }
    // ScreenCaptureKit cold startup is part of this capture-only budget.
    _ = done.wait(timeout: .now() + 12.0)
    return encode(box.get())
}

@_cdecl("volna_ocr_bytes")
public func volnaOCRBytes(_ bytes: UnsafePointer<UInt8>, _ count: Int) -> UnsafeMutablePointer<CChar>? {
    let data = Data(bytes: bytes, count: count)
    guard let source = CGImageSourceCreateWithData(data as CFData, nil),
          let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
        return encode(OCRResult(error: "Could not decode screen image"))
    }
    do { return encode(try recognize(image)) }
    catch { return encode(OCRResult(error: "Vision could not recognize screen text")) }
}

@_cdecl("volna_ocr_file")
public func volnaOCRFile(_ path: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>? {
    let url = URL(fileURLWithPath: String(cString: path))
    guard let source = CGImageSourceCreateWithURL(url as CFURL, nil), let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
        return encode(OCRResult(error: "Could not open OCR test image"))
    }
    do { return encode(try recognize(image)) }
    catch { return encode(OCRResult(error: "Vision could not recognize text")) }
}

@_cdecl("volna_screen_free")
public func volnaScreenFree(_ string: UnsafeMutablePointer<CChar>?) { free(string) }
