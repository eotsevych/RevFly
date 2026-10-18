// revfly-translate: translates text with Apple's on-device Translation framework for RevFly.
// Needs macOS 26+ (TranslationSession(installedSource:target:) translates without a SwiftUI view).
// Built by scripts/build_apple_translate.sh (run from src-tauri/build.rs) and bundled as a Tauri
// sidecar next to the RevFly executable.
//
// Worker mode (no arguments): reads one JSON request per line on stdin and writes one JSON response
// per line on stdout, in order. RevFly keeps one worker running so the language model stays warm.
//   {"op":"ping"}                                           → {"ok":true,"os":"26.1","protocol":1}
//   {"op":"status","source":"uk","target":"en"}             → {"ok":true,"status":"installed|supported|unsupported"}
//   {"op":"languages","target":"en"}                        → {"ok":true,"languages":[{"code","name","status"}]}
//   {"op":"warm","source":"uk","target":"en"}               → {"ok":true,"source":"uk","ms":…}
//   {"op":"translate","text":…,"source":"uk","target":"en"} → {"ok":true,"text":…,"source":"uk","ms":…}
// A missing or empty "source" is detected from the text. Any request may add
// "model":"lowLatency"|"highFidelity", the model to prefer when both are downloaded (macOS 26.4+;
// lowLatency when absent). Failures are
//   {"ok":false,"error":"not_installed|unsupported|unable_to_identify|nothing_to_translate|bad_request|internal","message":…}
//
// Download mode: `revfly-translate download <source> <target> [model]` opens a small window that asks
// macOS to download the language pair (Apple's own download prompt), then prints one JSON line
// ({"ok":true}, or an error; "cancelled" when the window is closed) and exits.
//
// Smoke test: scripts/test_apple_translate.sh

import AppKit
import Foundation
import NaturalLanguage
import SwiftUI
import Translation

let protocolVersion = 1

// MARK: - Output

func emit(_ object: [String: Any]) {
    guard var data = try? JSONSerialization.data(withJSONObject: object, options: [.withoutEscapingSlashes]) else {
        FileHandle.standardOutput.write(Data("{\"ok\":false,\"error\":\"internal\",\"message\":\"Could not encode the response\"}\n".utf8))
        return
    }
    data.append(0x0A)
    FileHandle.standardOutput.write(data)
}

func failure(_ code: String, _ message: String) -> [String: Any] {
    ["ok": false, "error": code, "message": message]
}

func elapsedMs(since start: Date) -> Int {
    Int(Date().timeIntervalSince(start) * 1000)
}

// MARK: - Models

/// macOS 26.4 added a second model per language pair: low latency (tens of ms per sentence) and
/// high fidelity (the Translate app's default; better wording, ~0.4-0.8 s per sentence). Which one
/// a plain session uses depends on the SDK the binary was built with, so the helper asks for the
/// preferred one and uses the other when only that is downloaded. `nil` = the system's only model
/// (before 26.4).
enum Model: String {
    case highFidelity, lowLatency
}

/// The models to try, preferred first. REVFLY_TRANSLATE_MODEL=highFidelity|lowLatency limits it to
/// one (the smoke test uses it to reach a pair that isn't downloaded).
func candidateModels(_ preferred: Model?) -> [Model?] {
    guard #available(macOS 26.4, *) else { return [nil] }
    if let forced = ProcessInfo.processInfo.environment["REVFLY_TRANSLATE_MODEL"].flatMap(Model.init(rawValue:)) {
        return [forced]
    }
    return preferred == .highFidelity ? [.highFidelity, .lowLatency] : [.lowLatency, .highFidelity]
}

@available(macOS 26.4, *)
func strategy(_ model: Model) -> TranslationSession.Strategy {
    model == .lowLatency ? .lowLatency : .highFidelity
}

func availability(_ model: Model?) -> LanguageAvailability {
    if #available(macOS 26.4, *), let model {
        return LanguageAvailability(preferredStrategy: strategy(model))
    }
    return LanguageAvailability()
}

func makeSession(_ from: Locale.Language, _ to: Locale.Language, _ model: Model?) -> TranslationSession {
    if #available(macOS 26.4, *), let model {
        return TranslationSession(installedSource: from, target: to, preferredStrategy: strategy(model))
    }
    return TranslationSession(installedSource: from, target: to)
}

func makeConfiguration(_ from: Locale.Language, _ to: Locale.Language, _ model: Model?) -> TranslationSession.Configuration {
    if #available(macOS 26.4, *), let model {
        return TranslationSession.Configuration(source: from, target: to, preferredStrategy: strategy(model))
    }
    return TranslationSession.Configuration(source: from, target: to)
}

/// The pair's status and the model to use: the first downloaded one, else the first that can be
/// downloaded. Right after the app is installed or updated the system briefly reports downloaded
/// languages as not downloaded, so "not downloaded" is checked again once after a short pause.
func resolve(_ from: Locale.Language, _ to: Locale.Language, _ preferred: Model?) async -> (LanguageAvailability.Status, Model?) {
    let first = await resolveOnce(from, to, preferred)
    guard first.0 == .supported else { return first }
    try? await Task.sleep(nanoseconds: 500_000_000)
    return await resolveOnce(from, to, preferred)
}

func resolveOnce(_ from: Locale.Language, _ to: Locale.Language, _ preferred: Model?) async -> (LanguageAvailability.Status, Model?) {
    let models = candidateModels(preferred)
    var best: (LanguageAvailability.Status, Model?) = (.unsupported, models[0])
    for model in models {
        let status = await availability(model).status(from: from, to: to)
        if status == .installed { return (.installed, model) }
        if status == .supported && best.0 == .unsupported { best = (.supported, model) }
    }
    return best
}

// MARK: - Languages

func statusName(_ status: LanguageAvailability.Status) -> String {
    switch status {
    case .installed: return "installed"
    case .supported: return "supported"
    case .unsupported: return "unsupported"
    @unknown default: return "unsupported"
    }
}

/// Language name in English, e.g. "Ukrainian", "Chinese (Traditional)". The script is named only
/// when the identifier has one ("zh-Hant").
func displayName(_ language: Locale.Language) -> String {
    let english = Locale(identifier: "en")
    guard let code = language.languageCode?.identifier else { return language.minimalIdentifier }
    let name = english.localizedString(forLanguageCode: code) ?? code
    let id = language.minimalIdentifier
    guard id.split(separator: "-").count > 1, let script = language.script?.identifier else { return name }
    let scriptName = ["Hans": "Simplified", "Hant": "Traditional"][script]
        ?? english.localizedString(forScriptCode: script) ?? script
    return "\(name) (\(scriptName))"
}

/// The language the text is written in, as a BCP-47 code, or nil when it can't be told.
func detectLanguage(_ text: String) -> String? {
    let recognizer = NLLanguageRecognizer()
    recognizer.processString(text)
    guard let lang = recognizer.dominantLanguage, lang != .undetermined else { return nil }
    return lang.rawValue
}

/// Maps the framework's errors to the codes RevFly shows reasons for.
func describe(_ error: Error, source: String, target: String) -> [String: Any] {
    let pair = "\(source) → \(target)"
    if TranslationError.notInstalled ~= error {
        return failure("not_installed", "The \(pair) languages aren't downloaded yet")
    }
    if TranslationError.unsupportedSourceLanguage ~= error
        || TranslationError.unsupportedTargetLanguage ~= error
        || TranslationError.unsupportedLanguagePairing ~= error
    {
        return failure("unsupported", "Apple Translation doesn't support \(pair)")
    }
    if TranslationError.unableToIdentifyLanguage ~= error {
        return failure("unable_to_identify", "Couldn't tell which language the text is in")
    }
    if TranslationError.nothingToTranslate ~= error {
        return failure("nothing_to_translate", "There is no text to translate")
    }
    let message = (error as? LocalizedError)?.errorDescription ?? String(describing: error)
    return failure("internal", message)
}

// MARK: - Worker

struct Request: Decodable {
    let op: String
    let text: String?
    let source: String?
    let target: String?
    let model: String?
}

/// Sessions by "source>target>model", kept so later translations skip loading the model.
@MainActor var sessions: [String: TranslationSession] = [:]

@MainActor
func handle(_ request: Request) async -> [String: Any] {
    let preferred = request.model.flatMap(Model.init(rawValue:))
    switch request.op {
    case "ping":
        let v = ProcessInfo.processInfo.operatingSystemVersion
        return ["ok": true, "os": "\(v.majorVersion).\(v.minorVersion)", "protocol": protocolVersion]

    case "status":
        guard let source = request.source, !source.isEmpty, let target = request.target, !target.isEmpty else {
            return failure("bad_request", "status needs source and target")
        }
        let (status, _) = await resolve(Locale.Language(identifier: source), Locale.Language(identifier: target), preferred)
        return ["ok": true, "status": statusName(status)]

    case "languages":
        guard let target = request.target, !target.isEmpty else {
            return failure("bad_request", "languages needs target")
        }
        let to = Locale.Language(identifier: target)
        // One row per language and script: "fr-CA" and "fr" use the same download. The script is
        // kept only where it tells two downloads apart, e.g. "zh-Hans" and "zh-Hant".
        var scripts: [String: Set<String>] = [:]
        for language in await LanguageAvailability().supportedLanguages {
            guard let code = language.languageCode?.identifier, code != to.languageCode?.identifier else { continue }
            scripts[code, default: []].insert(language.script?.identifier ?? "")
        }
        var rows: [[String: Any]] = []
        for (code, codeScripts) in scripts {
            for script in codeScripts.sorted() {
                let id = codeScripts.count > 1 && !script.isEmpty ? "\(code)-\(script)" : code
                let language = Locale.Language(identifier: id)
                let (status, _) = await resolve(language, to, preferred)
                rows.append(["code": id, "name": displayName(language), "status": statusName(status)])
                if codeScripts.count == 1 { break }
            }
        }
        rows.sort { ($0["name"] as! String) < ($1["name"] as! String) }
        return ["ok": true, "languages": rows]

    case "warm", "translate":
        guard let target = request.target, !target.isEmpty else {
            return failure("bad_request", "\(request.op) needs target")
        }
        let text = request.text ?? ""
        let source: String
        if let given = request.source, !given.isEmpty {
            source = given
        } else if let detected = detectLanguage(text) {
            source = detected
        } else {
            return failure("unable_to_identify", "Couldn't tell which language the text is in")
        }
        if request.op == "translate" && text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return failure("nothing_to_translate", "There is no text to translate")
        }
        let started = Date()
        let key = "\(source)>\(target)>\(preferred?.rawValue ?? "")"
        do {
            let session: TranslationSession
            if let cached = sessions[key] {
                session = cached
            } else {
                let from = Locale.Language(identifier: source)
                let to = Locale.Language(identifier: target)
                let (status, model) = await resolve(from, to, preferred)
                switch status {
                case .unsupported:
                    var result = failure("unsupported", "Apple Translation doesn't support \(source) → \(target)")
                    result["ms"] = elapsedMs(since: started)
                    return result
                case .supported:
                    var result = failure("not_installed", "The \(source) → \(target) languages aren't downloaded yet")
                    result["ms"] = elapsedMs(since: started)
                    return result
                default:
                    session = makeSession(from, to, model)
                    sessions[key] = session
                }
            }
            // A one-word translation loads the model, so the first real dictation is fast.
            let response = try await session.translate(request.op == "warm" ? "OK" : text)
            var result: [String: Any] = ["ok": true, "source": source, "ms": elapsedMs(since: started)]
            if request.op == "translate" { result["text"] = response.targetText }
            return result
        } catch {
            // A failed session (e.g. its languages were removed meanwhile) is rebuilt next time.
            sessions[key] = nil
            var result = describe(error, source: source, target: target)
            result["ms"] = elapsedMs(since: started)
            return result
        }

    default:
        return failure("bad_request", "Unknown op '\(request.op)'")
    }
}

@MainActor
func runWorker() async {
    do {
        for try await line in FileHandle.standardInput.bytes.lines {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed.isEmpty { continue }
            guard let request = try? JSONDecoder().decode(Request.self, from: Data(trimmed.utf8)) else {
                emit(failure("bad_request", "Request is not valid JSON"))
                continue
            }
            emit(await handle(request))
        }
    } catch {
        FileHandle.standardError.write(Data("revfly-translate: reading stdin failed: \(error)\n".utf8))
    }
}

// MARK: - Download window

/// Prints the result once and quits; later calls (e.g. the window closing afterwards) do nothing.
@MainActor
enum DownloadOutcome {
    static var reported = false
    static func finish(_ object: [String: Any]) {
        guard !reported else { return }
        reported = true
        emit(object)
        exit((object["ok"] as? Bool) == true ? 0 : 1)
    }
}

struct DownloadView: View {
    let configuration: TranslationSession.Configuration
    let pair: String

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Download \(pair)").font(.headline)
            Text("macOS will ask to download these languages for on-device translation. They work offline and nothing leaves your Mac.")
                .font(.callout)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            HStack {
                ProgressView().controlSize(.small)
                Spacer()
                Button("Cancel") {
                    DownloadOutcome.finish(failure("cancelled", "The download was cancelled"))
                }
            }
        }
        .padding(20)
        .frame(width: 400)
        .translationTask(configuration) { session in
            do {
                try await session.prepareTranslation()
                DownloadOutcome.finish(["ok": true])
            } catch {
                let source = configuration.source?.minimalIdentifier ?? "?"
                let target = configuration.target?.minimalIdentifier ?? "?"
                DownloadOutcome.finish(describe(error, source: source, target: target))
            }
        }
    }
}

final class DownloadDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
    let source: String
    let target: String
    let preferred: Model?
    var window: NSWindow?

    init(source: String, target: String, preferred: Model?) {
        self.source = source
        self.target = target
        self.preferred = preferred
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        Task { @MainActor in await self.start() }
    }

    func windowWillClose(_ notification: Notification) {
        MainActor.assumeIsolated {
            DownloadOutcome.finish(failure("cancelled", "The download window was closed"))
        }
    }

    @MainActor
    func start() async {
        let from = Locale.Language(identifier: source)
        let to = Locale.Language(identifier: target)
        let (status, model) = await resolve(from, to, preferred)
        switch status {
        case .installed:
            return DownloadOutcome.finish(["ok": true])
        case .unsupported:
            return DownloadOutcome.finish(failure("unsupported", "Apple Translation doesn't support \(source) → \(target)"))
        default:
            break
        }
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 400, height: 150),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false)
        window.title = "RevFly — Translation Languages"
        window.isReleasedWhenClosed = false
        window.delegate = self
        let pair = "\(displayName(from)) → \(displayName(to))"
        window.contentView = NSHostingView(
            rootView: DownloadView(configuration: makeConfiguration(from, to, model), pair: pair))
        window.center()
        window.level = .floating
        window.makeKeyAndOrderFront(nil)
        NSApp.activate()
        self.window = window
    }
}

// MARK: - Entry

let arguments = Array(CommandLine.arguments.dropFirst())
if arguments.first == "download" {
    guard arguments.count == 3 || arguments.count == 4 else {
        emit(failure("bad_request", "usage: revfly-translate download <source> <target> [lowLatency|highFidelity]"))
        exit(2)
    }
    MainActor.assumeIsolated {
        let app = NSApplication.shared
        // A window of its own, but no Dock icon or menu bar.
        app.setActivationPolicy(.accessory)
        let delegate = DownloadDelegate(
            source: arguments[1], target: arguments[2],
            preferred: arguments.count == 4 ? Model(rawValue: arguments[3]) : nil)
        app.delegate = delegate
        withExtendedLifetime(delegate) { app.run() }
    }
} else {
    Task { @MainActor in
        await runWorker()
        exit(0)
    }
    dispatchMain()
}
