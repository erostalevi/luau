// apple-llm — tiny bridge between Luau and Apple's on-device language model
// (FoundationModels, macOS 26+ with Apple Intelligence).
//
// Protocol (one request per process; JSON, UTF-8):
//   apple-llm availability
//     stdout: {"status":"available"|"appleIntelligenceNotEnabled"|"deviceNotEligible"|
//              "modelNotReady"|"unsupportedOs"|"unknown","contextSize":8192}
//   apple-llm generate        (request JSON on stdin)
//     stdin:  {"instructions":"…","prompt":"…","maxTokens":1024,"temperature":0.3,
//              "schema":{JSON Schema}|null,"stream":true}
//     stdout: NDJSON — zero or more {"delta":"…"}, then {"done":true,"text":"…"}
//             or a single {"error":"<code>"} (exit status 1).
//   apple-llm --version
//
// Privacy: the helper never logs. Prompts and answers only travel over the
// stdin/stdout pipes of the parent process; errors carry a code, never text.
//
// Built by src-tauri/build.rs with a macOS 13 deployment target and
// FoundationModels weak-linked, so it starts on any macOS and answers
// `unsupportedOs` below macOS 26.

import Foundation

#if canImport(FoundationModels)
  import FoundationModels
#endif

let version = "1"
let maxInputBytes = 2 * 1024 * 1024
let stdout = FileHandle.standardOutput

func emit(_ obj: [String: Any]) {
  guard var data = try? JSONSerialization.data(withJSONObject: obj, options: []) else { return }
  data.append(0x0A)
  stdout.write(data)
}

func fail(_ code: String) -> Never {
  emit(["error": code])
  exit(1)
}

struct Request: Decodable {
  var instructions: String?
  var prompt: String
  var maxTokens: Int?
  var temperature: Double?
  var schema: JSONValue?
  var stream: Bool?
}

/// Minimal JSON value so the schema can be re-encoded for `GenerationSchema`.
indirect enum JSONValue: Codable {
  case null, bool(Bool), number(Double), string(String), array([JSONValue]), object([String: JSONValue])
  init(from d: Decoder) throws {
    let c = try d.singleValueContainer()
    if c.decodeNil() { self = .null }
    else if let b = try? c.decode(Bool.self) { self = .bool(b) }
    else if let n = try? c.decode(Double.self) { self = .number(n) }
    else if let s = try? c.decode(String.self) { self = .string(s) }
    else if let a = try? c.decode([JSONValue].self) { self = .array(a) }
    else { self = .object(try c.decode([String: JSONValue].self)) }
  }
  func encode(to e: Encoder) throws {
    var c = e.singleValueContainer()
    switch self {
    case .null: try c.encodeNil()
    case .bool(let b): try c.encode(b)
    case .number(let n):
      if n.rounded() == n && abs(n) < 1e15 { try c.encode(Int64(n)) } else { try c.encode(n) }
    case .string(let s): try c.encode(s)
    case .array(let a): try c.encode(a)
    case .object(let o): try c.encode(o)
    }
  }
}

#if canImport(FoundationModels)
  @available(macOS 26.0, *)
  func availabilityStatus() -> (String, Int) {
    let model = SystemLanguageModel.default
    let ctx = model.contextSize
    switch model.availability {
    case .available: return ("available", ctx)
    case .unavailable(let reason):
      switch reason {
      case .appleIntelligenceNotEnabled: return ("appleIntelligenceNotEnabled", ctx)
      case .deviceNotEligible: return ("deviceNotEligible", ctx)
      case .modelNotReady: return ("modelNotReady", ctx)
      @unknown default: return ("unknown", ctx)
      }
    @unknown default: return ("unknown", ctx)
    }
  }

  @available(macOS 26.0, *)
  func errorCode(_ error: Error) -> String {
    if let e = error as? LanguageModelSession.GenerationError {
      switch e {
      case .exceededContextWindowSize: return "context"
      case .guardrailViolation, .refusal: return "guardrail"
      case .unsupportedLanguageOrLocale: return "language"
      case .assetsUnavailable: return "unavailable"
      case .rateLimited, .concurrentRequests: return "rate"
      case .decodingFailure, .unsupportedGuide: return "decoding"
      @unknown default: return "failed"
      }
    }
    // Newer error types (macOS 27+): classify by type name so this file still
    // builds against older SDKs.
    let name = String(describing: type(of: error)) + " " + String(describing: error)
    if name.contains("ontextSize") || name.contains("ontextWindow") { return "context" }
    if name.contains("uardrail") || name.contains("efusal") { return "guardrail" }
    if name.contains("nsupportedLanguage") { return "language" }
    if name.contains("ateLimit") { return "rate" }
    return "failed"
  }

  @available(macOS 26.0, *)
  func generate(_ req: Request) async {
    let (status, _) = availabilityStatus()
    guard status == "available" else { fail("unavailable") }
    let options = GenerationOptions(
      temperature: req.temperature.map { min(max($0, 0), 2) },
      maximumResponseTokens: req.maxTokens.map { min(max($0, 16), 8192) })
    do {
      if let schemaValue = req.schema {
        let data = try JSONEncoder().encode(schemaValue)
        let schema: GenerationSchema
        do { schema = try JSONDecoder().decode(GenerationSchema.self, from: data) } catch { fail("invalidSchema") }
        let session = LanguageModelSession(model: SystemLanguageModel.default, instructions: req.instructions)
        let r = try await session.respond(to: req.prompt, schema: schema, includeSchemaInPrompt: true, options: options)
        emit(["done": true, "text": r.content.jsonString])
        return
      }
      // Summaries and rewrites transform the user's own notes.
      let model = SystemLanguageModel(guardrails: .permissiveContentTransformations)
      let session = LanguageModelSession(model: model, instructions: req.instructions)
      if req.stream ?? true {
        var sent = ""
        for try await snap in session.streamResponse(to: req.prompt, options: options) {
          let cur = snap.content
          if cur.hasPrefix(sent) {
            let delta = String(cur.dropFirst(sent.count))
            if !delta.isEmpty { emit(["delta": delta]) }
          }
          sent = cur
        }
        emit(["done": true, "text": sent])
      } else {
        let r = try await session.respond(to: req.prompt, options: options)
        emit(["done": true, "text": r.content])
      }
    } catch {
      fail(errorCode(error))
    }
  }
#endif

let args = CommandLine.arguments.dropFirst()
switch args.first {
case "--version":
  emit(["version": version])
case "availability":
  #if canImport(FoundationModels)
    if #available(macOS 26.0, *) {
      let (status, ctx) = availabilityStatus()
      emit(["status": status, "contextSize": ctx])
    } else {
      emit(["status": "unsupportedOs", "contextSize": 0])
    }
  #else
    emit(["status": "unsupportedOs", "contextSize": 0])
  #endif
case "generate":
  let input = FileHandle.standardInput.readData(ofLength: maxInputBytes + 1)
  if input.count > maxInputBytes { fail("tooLarge") }
  guard let req = try? JSONDecoder().decode(Request.self, from: input), !req.prompt.isEmpty else {
    fail("invalidRequest")
  }
  #if canImport(FoundationModels)
    if #available(macOS 26.0, *) {
      await generate(req)
    } else {
      fail("unavailable")
    }
  #else
    fail("unavailable")
  #endif
default:
  FileHandle.standardError.write("usage: apple-llm availability | generate | --version\n".data(using: .utf8)!)
  exit(2)
}
