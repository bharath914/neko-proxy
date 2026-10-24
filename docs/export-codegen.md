# Export and code generation

Linux's Export menu and `neko export --kind <kind> --ids 1,2 -o artifact`
(or `nekod export`) share the API export(kind, ids) implementation.
Kinds: har, postman, k6, wiremock, msw, mockoon, openapi, dart, kotlin, swift,
typescript and dio. Copy as Dio uses FlowDetail.dio, prepared in the core.

All artifacts are redacted in the core, including token-shaped paths. The old
redact=false API input no longer disables redaction; --no-redact is refused.
Binary bodies are omitted from redacted artifacts because schema-free binary
credentials cannot be safely identified. Output is limited to 10,000 flows,
32 MiB of captured input bodies and 64 MiB of generated artifact data.

Postman uses Collection 2.1. WireMock emits a bulk mappings import document.
MSW emits handlers using msw/http, k6 emits an HTTP replay script, and Mockoon
emits an environment at migration 33 with response templating disabled.
OpenAPI 3.0.3 infers request/response schemas from observed examples, preserves
literal paths, and marks query parameters optional. It is a starting point,
not a complete service contract. Multiple captures of the same operation can
overwrite an example. MSW/Mockoon path matching does not distinguish query
variants; review conflicting recordings before use. Redacted placeholders
must be replaced when authenticating replay scripts.

Models infer JSON objects from request/response samples up to 256 KiB, with
bounded nesting and optional fields. Names are FlowNRequest/Response and
fieldN, with original keys retained in serialization mappings/comments.
Swift is Codable, Kotlin uses kotlinx.serialization, TypeScript has interfaces,
and Dart has constructors and fromJson factories. Heterogeneous/null values
use generic JSON types. Sampling cannot infer a complete nullable/union schema.

Validation: cargo test --workspace passed. API tests exercise every kind even
with redact=false; store tests cover redaction, structure and binary omission.
43 desktop screenshot tests passed. Generated Swift passed swiftc -typecheck,
TypeScript passed tsc --noEmit, and MSW/k6 passed node --check. Postman passed
its official draft-07 schema with AJV; OpenAPI passed swagger-cli validation.
Mockoon CLI served the exact captured JSON and content type on localhost.
Dart/Kotlin compilers and a WireMock runtime are unavailable here.

Failures corrected: missing dio in the demo FlowDetail initializer (E0063);
Mockoon migration 0 returned {} instead of the captured body (set to 33);
first Postman schema URL returned curl (56) HTTP 404 (corrected URL validated).
No performance re-benchmark. Proto adds FlowDetail.dio = 30; regenerate Swift
using desktop-macos/gen-proto.sh.

References checked through read-docs:
- https://schema.postman.com/collection/json/v2.1.0/draft-07/docs/index.html
- https://wiremock.org/docs/stubbing/
- https://mswjs.io/guides/best-practices/network-behavior-overrides
- https://grafana.com/docs/k6/latest/javascript-api/k6-http/request/
- https://github.com/mockoon/mockoon/tree/main/packages/commons/src/models
- https://spec.openapis.org/oas/v3.0.3.html
- https://pub.dev/documentation/dio/latest/dio/Dio/request.html
