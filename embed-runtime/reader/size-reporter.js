// The height reporter every widget document carries; see SIZE_REPORTER in
// src-tauri/core/src/bundle/fold.rs for why it measures and wakes as it does.
(function () {
  if (parent === window) return;
  var last = 0,
    queued = 0;
  function send() {
    queued = 0;
    var h = Math.ceil(document.documentElement.getBoundingClientRect().height);
    if (!(h > 0) || Math.abs(h - last) < 2) return;
    last = h;
    parent.postMessage({ mfw: 1, type: 'size', height: h }, '*');
  }
  function later() {
    if (!queued) queued = setTimeout(send, 30);
  }
  function start() {
    new ResizeObserver(later).observe(document.documentElement);
    new MutationObserver(later).observe(document.documentElement, {
      subtree: true,
      childList: true,
      attributes: true,
      characterData: true,
    });
    addEventListener('load', later);
    addEventListener('message', function () {
      setTimeout(later, 0);
    });
    document.fonts && document.fonts.ready.then(later);
    later();
  }
  if (document.readyState === 'loading') addEventListener('DOMContentLoaded', start);
  else start();
})();
