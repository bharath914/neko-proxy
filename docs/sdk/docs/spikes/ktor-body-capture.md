# Spike S4: Ktor body capture

Status: **done.** Ktor 3.6.0, engines CIO and OkHttp, plus `MockEngine`.

## Approach

A client plugin (`createClientPlugin`):

- `on(Send)`: starts the call, records the request (in-memory `OutgoingContent.ByteArrayContent` is copied; streamed and
  channel uploads record size only), reports failures and rethrows.
- A receive hook after Ktor's own stages splits the response body channel with `ByteReadChannel.split(scope)` and
  `HttpClientCall.replaceResponse`, the way Ktor's `ResponseObserver` plugin does (read from the Ktor 3.6.0 sources). The
  app gets one half untouched; the other half is copied to the desktop as it arrives.

`split`, `rawContent` and `replaceResponse` are `@InternalAPI` in Ktor, so this needs an opt-in and can break on a Ktor
update. The tests cover the supported versions; pin and re-test on each Ktor upgrade.

## Verified

| Check | Result |
|---|---|
| The app still receives the full body | `streamingResponsesArriveWholeOnBothRealEngines`: a 6 event SSE response, read in 16 byte pieces, equals what the server sent on CIO and OkHttp. |
| Captured bytes equal received bytes | Same test: the desktop's copy equals the stream. |
| Works on the OkHttp and CIO engines, and MockEngine | Yes. |
| Streaming responses keep streaming | The copy runs in its own coroutine; `bodyDelay` in the mock server shows chunks arrive over time. |
| A call through the OkHttp engine with an instrumented client is recorded once | `ktorOnTheOkHttpEngineWithAnInstrumentedClientIsRecordedOnce`. The core drops the OkHttp record that matches a Ktor one (same method and URL within 50 ms), so the Ktor record, which carries the metadata, wins. |

## A bug this found

The receive pipeline can run before the `Send` hook returns, so announcing the response in `Send` could put `CallComplete`
before `ResponseStart` on fast responses. The response is now announced in the receive hook, always before any body bytes.
The call object seen by the receive pipeline is also a different instance from the one `Send` returns, so state travels in the
*request's* attributes.
