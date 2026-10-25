(function () {
  "use strict";
  // Copy buttons
  document.querySelectorAll(".cmd button").forEach(function (b) {
    b.onclick = function () {
      var text = b.parentNode.querySelector("pre").textContent;
      var done = function () { var t = b.textContent; b.textContent = "Copied!"; setTimeout(function () { b.textContent = t; }, 1400); };
      try { navigator.clipboard.writeText(text).then(done, function () {}); } catch (e) {}
    };
  });
  // Platform tabs: pick one from the browser, let the visitor switch
  var tabs = document.getElementById("tabs");
  if (!tabs) return;
  var ua = navigator.userAgent || "";
  var guess = /iPhone|iPad|iPod/.test(ua) || (/Macintosh/.test(ua) && navigator.maxTouchPoints > 1) ? "ios"
    : /Android/.test(ua) ? "android"
    : /Firefox\//.test(ua) && !/Mobile|Tablet/.test(ua) ? "firefox"
    : /Windows/.test(ua) ? "win"
    : /Mac OS X|Macintosh/.test(ua) ? "mac"
    : /Linux|X11|CrOS/.test(ua) ? "linux" : "ios";
  var names = { ios: "iPhone / iPad", android: "Android", mac: "macOS", win: "Windows", linux: "Linux", firefox: "Firefox" };
  function select(os, focus) {
    tabs.querySelectorAll("button").forEach(function (b) { b.setAttribute("aria-selected", String(b.dataset.os === os)); });
    document.querySelectorAll(".panel").forEach(function (p) { p.hidden = p.dataset.os !== os; });
    try { history.replaceState(null, "", "#" + os); } catch (e) {}
  }
  Object.keys(names).forEach(function (k) {
    var b = document.createElement("button");
    b.type = "button"; b.setAttribute("role", "tab"); b.dataset.os = k; b.textContent = names[k];
    b.onclick = function () { select(k); };
    tabs.appendChild(b);
  });
  var h = (location.hash || "").slice(1);
  select(names[h] ? h : guess);
})();
