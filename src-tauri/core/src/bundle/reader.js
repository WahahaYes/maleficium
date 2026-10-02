(function () {
  'use strict';
  var FOLDER = __FOLDER__;
  var TOKENS = __TOKENS__;
  var body = document.body;
  function island(id) {
    var el = document.getElementById(id);
    return el ? JSON.parse(el.textContent) : {};
  }
  if (FOLDER && location.protocol === 'file:') {
    var main = document.querySelector('main');
    main.textContent = '';
    var p = document.createElement('p');
    p.className = 'unsupported';
    p.textContent =
      'This is the folder export, and it needs to be served over http or https (a static host such as GitHub Pages). Browsers do not let a page opened from a file read its own assets. Open the single-file export instead, or serve this folder.';
    main.appendChild(p);
    return;
  }
  var manifest = island('mfw-manifest');
  var docs = island('mfw-widgets');
  var blobs = island('mfw-assets');
  var dark = window.matchMedia('(prefers-color-scheme: dark)');
  function mode() {
    return dark.matches ? 'dark' : 'light';
  }
  function paint() {
    body.setAttribute('data-theme', mode());
  }
  paint();
  function theme() {
    var cs = getComputedStyle(body),
      tokens = {};
    TOKENS.forEach(function (t) {
      tokens[t] = cs.getPropertyValue(t).trim();
    });
    return { mode: mode(), tokens: tokens };
  }
  function b64(s) {
    var bin = atob(s),
      out = new Uint8Array(bin.length);
    for (var i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
  }

  var pdf = document.getElementById('pdf');
  var link = document.getElementById('pdf-link');
  if (!FOLDER && pdf && link) {
    var m = /^data:[^,]*,(.*)$/.exec(link.getAttribute('href') || '');
    if (m) pdf.data = URL.createObjectURL(new Blob([b64(m[1])], { type: 'application/pdf' }));
  }

  var widgets = {};
  (manifest.widgets || []).forEach(function (w) {
    widgets[w.id] = w;
  });
  var frames = [];
  function bytesOf(key) {
    var a = (manifest.assets || {})[key];
    if (!a) return Promise.reject(new Error('no asset ' + key));
    if (a.mode === 'inline') return Promise.resolve(b64(blobs[a.sha256] || '').buffer);
    return fetch(a.path).then(function (r) {
      if (!r.ok) throw new Error(a.path + ': ' + r.status);
      return r.arrayBuffer();
    });
  }
  function nameOf(a, role) {
    var s = a.source || a.path || role;
    return s.split('/').pop();
  }
  function init(rec) {
    var w = widgets[rec.id],
      roles = Object.keys(w.sources || {});
    var remote = roles.some(function (r) {
      return manifest.assets[w.sources[r]].mode === 'remote';
    });
    if (remote) {
      rec.fig.setAttribute('data-state', 'poster-only');
      return;
    }
    Promise.all(
      roles.map(function (r) {
        return bytesOf(w.sources[r]);
      }),
    )
      .then(function (bufs) {
        var sources = {};
        roles.forEach(function (r, i) {
          var a = manifest.assets[w.sources[r]];
          sources[r] = { name: nameOf(a, r), mime: a.mime, sha256: a.sha256, bytes: bufs[i] };
        });
        rec.win.postMessage(
          {
            mfw: 1,
            type: 'init',
            protocol: 1,
            widgetId: w.id,
            runtime: w.runtime || '',
            alt: w.alt || '',
            options: w.options || {},
            theme: theme(),
            sources: sources,
          },
          '*',
          bufs,
        );
        rec.fig.classList.add('live');
        rec.fig.setAttribute('data-state', 'ready');
      })
      .catch(function () {
        rec.fig.setAttribute('data-state', 'error');
      });
  }
  window.addEventListener('message', function (e) {
    var d = e.data;
    if (e.origin !== 'null' || !d || d.mfw !== 1) return;
    for (var i = 0; i < frames.length; i++) {
      if (frames[i].win !== e.source) continue;
      if (d.type === 'ready' && !frames[i].started) {
        frames[i].started = true;
        init(frames[i]);
      } else if (d.type === 'status' && d.state === 'error')
        frames[i].fig.setAttribute('data-state', 'error');
      return;
    }
  });
  dark.addEventListener('change', function () {
    paint();
    var t = theme();
    frames.forEach(function (f) {
      if (f.started)
        f.win.postMessage({ mfw: 1, type: 'theme', mode: t.mode, tokens: t.tokens }, '*');
    });
  });
  // A single-file widget is mounted inside a wrapper document of its own.
  // Navigating a frame is checked against its parent's frame-src, and the
  // reader's is the union of every widget's frame origins; the wrapper's
  // names this widget's only, so a widget cannot navigate its own frame to
  // another widget's origin. The wrapper relays the bridge both ways (the
  // widget's parent is the wrapper) and keeps the widget's sandbox.
  var WRAP_DIRECTIVE = 'frame-src';
  var RELAY =
    'addEventListener("message",function(e){var f=document.querySelector("iframe");if(!f)return;' +
    'var d=e.data,t=[];if(e.source===f.contentWindow){parent.postMessage(d,"*");return}' +
    'if(e.source!==parent)return;if(d&&d.sources)for(var k in d.sources)' +
    'if(d.sources[k]&&d.sources[k].bytes instanceof ArrayBuffer)t.push(d.sources[k].bytes);' +
    'f.contentWindow.postMessage(d,"*",t)})';
  function attr(s) {
    return String(s).replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;');
  }
  function wrap(w, doc) {
    var origins = ((w.csp || {}).frameDomains || []).join(' ') || "'none'";
    return (
      '<!doctype html><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="' +
      attr(WRAP_DIRECTIVE + ' ' + origins) +
      '"><style>html,body{margin:0;height:100%;overflow:hidden}' +
      'iframe{border:0;display:block;width:100%;height:100%}</style><script>' +
      RELAY +
      '<\/script><iframe sandbox="allow-scripts" referrerpolicy="no-referrer" title="' +
      attr(w.alt || w.id) +
      '" srcdoc="' +
      attr(doc) +
      '"></iframe>'
    );
  }
  Array.prototype.forEach.call(document.querySelectorAll('figure[data-widget]'), function (fig) {
    var w = widgets[fig.getAttribute('data-widget')];
    if (!w) return;
    var f = document.createElement('iframe');
    f.setAttribute('sandbox', 'allow-scripts');
    f.setAttribute('title', w.alt || w.id);
    f.setAttribute('referrerpolicy', 'no-referrer');
    fig.querySelector('.frame').appendChild(f);
    var rec = { id: w.id, fig: fig, win: f.contentWindow, started: false };
    frames.push(rec);
    // An author bundle need not speak the host protocol: it is shown once it has loaded.
    if (w.type === 'html') {
      f.addEventListener('load', function () {
        fig.classList.add('live');
        fig.setAttribute('data-state', 'ready');
      });
    }
    if (FOLDER) f.src = w.entry;
    else f.srcdoc = wrap(w, docs[w.id]);
  });
})();
