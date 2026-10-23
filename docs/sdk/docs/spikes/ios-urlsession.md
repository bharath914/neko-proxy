# Spike S3: iOS URLSession capture

Status: **not run.** It needs a Mac with Xcode and a Simulator or device. Nothing here is a result.

What was done instead, and what is left:

- The explicit approach (plan option C) is written: `Inspector.session(configuration:delegate:delegateQueue:)` returns a session with a recording delegate that forwards every selector to the app's own delegate and never re-issues requests (`InspectedSession.swift`). It has not been compiled.
- Alamofire coverage uses `EventMonitor` (`NekoInspectorAlamofire`). It compiles on Linux.
- Options A (`URLProtocol` injection) and B (delegate proxying by swizzling task creation) are **not implemented**. Choosing between them needs the matrix below.

Matrix to run: shared session, custom session, async/await, completion handler, upload, download, redirect, auth challenge, cancellation, HTTP/2. For each: are the request, response, body and timings captured; does the app's behaviour change (callbacks, challenges, redirects, caching)?

Known problem to measure: tasks created with a completion handler do not deliver `didReceive data` to the session delegate, so approach C records them without a body.

Record the table and the decision here once it has been run.
