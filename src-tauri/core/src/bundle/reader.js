(function () {
  'use strict';
  var FOLDER = __FOLDER__;
  var body = document.body;
  // A missing or malformed island reads as empty: the page then shows its
  // posters, it never throws.
  function island(id) {
    var el = document.getElementById(id);
    if (!el) return {};
    try {
      var v = JSON.parse(el.textContent);
      return v && typeof v === 'object' ? v : {};
    } catch (_) {
      return {};
    }
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
  // The paper's theme record, the one the page's CSS was written from:
  // every widget gets the same values the chrome is styled with.
  var themes = island('mfw-theme');
  var dark = window.matchMedia('(prefers-color-scheme: dark)');
  function mode() {
    return dark.matches ? 'dark' : 'light';
  }
  function paint() {
    body.setAttribute('data-theme', mode());
  }
  paint();
  function theme() {
    var m = mode();
    return { mode: m, tokens: themes[m] || {} };
  }
  function b64(s) {
    var bin = atob(s),
      out = new Uint8Array(bin.length);
    for (var i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
  }

  var widgets = {};
  (Array.isArray(manifest.widgets) ? manifest.widgets : []).forEach(function (w) {
    if (w && typeof w.id === 'string') widgets[w.id] = w;
  });
  // The kinds this reader mounts; anything else keeps its poster.
  var KINDS = ['model', 'video', 'table', 'chart', 'html', 'custom'];
  var frames = [];
  // A widget that fails gives its place back to the poster: the frame is
  // hidden again and the caption says so. A figure goes live only when its
  // widget reports painted (`status loaded`): until then the poster covers
  // the frame, so a screenshot never catches a mounted-but-blank widget.
  function fail(fig) {
    fig.classList.remove('live');
    fig.setAttribute('data-state', 'error');
  }
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
        rec.fig.setAttribute('data-state', 'loading');
      })
      .catch(function () {
        fail(rec.fig);
      });
  }
  window.addEventListener('message', function (e) {
    var d = e.data;
    if (e.origin !== 'null' || !d || d.mfw !== 1) return;
    for (var i = 0; i < frames.length; i++) {
      if (frames[i].win !== e.source) continue;
      if (d.type === 'ready' && !frames[i].started) {
        frames[i].started = true;
        try {
          init(frames[i]);
        } catch (_) {
          fail(frames[i].fig);
        }
      } else if (d.type === 'status' && d.state === 'loaded') {
        var fig = frames[i].fig;
        if (!fig.classList.contains('live') && fig.getAttribute('data-state') !== 'error') {
          fig.classList.add('live');
          fig.setAttribute('data-state', 'ready');
        }
      } else if (d.type === 'status' && d.state === 'error') fail(frames[i].fig);
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
  // A widget this copy cannot run keeps its poster and says why: no record,
  // a fallback, a kind or a custom runtime this reader does not know, or no
  // document to mount. A figure the in-app article marked for approval keeps
  // its poster and offers approval instead: the button only asks the app
  // (a message the app may ignore); the widget itself never runs here.
  function posterOnly(fig, w) {
    fig.setAttribute('data-state', 'poster-only');
    if (fig.getAttribute('data-approval') === 'required') {
      approvalButton(fig);
      return;
    }
    if (fig.querySelector('.m-widget-note')) return;
    var name = (w && (w.runtime || w.type)) || fig.getAttribute('data-type') || 'unknown';
    var p = document.createElement('p');
    p.className = 'm-widget-note';
    p.textContent =
      'Interactive version unavailable: this reader does not know runtime ' + String(name) + '.';
    fig.appendChild(p);
  }
  function approvalButton(fig) {
    if (fig.querySelector('.m-approve')) return;
    var b = document.createElement('button');
    b.type = 'button';
    b.className = 'm-approve';
    var id = fig.getAttribute('data-widget') || 'this widget';
    b.textContent = 'Approve';
    b.setAttribute('aria-label', 'Approve ' + id + ' so it can run here');
    b.addEventListener('click', function () {
      try {
        parent.postMessage({ mfw: 1, type: 'approve-widget', widgetId: id }, '*');
      } catch (_) {
        /* no app listening: the poster stays */
      }
    });
    fig.appendChild(b);
  }
  function runnable(w) {
    if (!w || w.fallback || KINDS.indexOf(w.type) < 0) return false;
    if (
      w.type === 'custom' &&
      !Object.prototype.hasOwnProperty.call(manifest.runtimes || {}, w.runtime)
    )
      return false;
    return FOLDER || typeof docs[w.id] === 'string';
  }
  Array.prototype.forEach.call(document.querySelectorAll('figure[data-widget]'), function (fig) {
    var w = widgets[fig.getAttribute('data-widget')];
    try {
      if (!runnable(w)) {
        posterOnly(fig, w);
        return;
      }
      mount(fig, w);
    } catch (_) {
      try {
        posterOnly(fig, w);
      } catch (_e) {
        /* the poster stays as it is */
      }
    }
  });
  function mount(fig, w) {
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
        if (fig.getAttribute('data-state') === 'error') return;
        fig.classList.add('live');
        fig.setAttribute('data-state', 'ready');
      });
    }
    if (FOLDER) f.src = w.entry;
    else f.srcdoc = wrap(w, docs[w.id]);
  }
})();

// The article's own niceties. Everything here is optional: with scripts off
// the article still reads, so nothing below is needed to read it.
(function () {
  'use strict';
  // Its own manifest read: the widget IIFE above owns its `manifest`, and
  // sharing across the scopes reads as a ReferenceError that kills every
  // nicety below on a multi-section page.
  function island(id) {
    var el = document.getElementById(id);
    if (!el) return {};
    try {
      var v = JSON.parse(el.textContent);
      return v && typeof v === 'object' ? v : {};
    } catch (_) {
      return {};
    }
  }
  var manifest = island('mfw-manifest');
  var main = document.querySelector('main');
  var article = document.querySelector('article') || main;
  if (!main || !article) return;
  function el(tag, cls, text) {
    var e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text) e.textContent = text;
    return e;
  }

  // A missing figure is a visible placeholder, not a broken image.
  Array.prototype.forEach.call(article.querySelectorAll('img.ltx_missing_image'), function (img) {
    var name = img.getAttribute('data-graphic') || img.getAttribute('alt') || 'figure';
    var box = el('div', 'm-missing', 'Missing figure: ' + name);
    box.setAttribute('role', 'img');
    box.setAttribute('aria-label', 'Missing figure ' + name);
    if (img.parentNode) img.parentNode.replaceChild(box, img);
  });
  Array.prototype.forEach.call(article.querySelectorAll('.ltx_ERROR'), function (e) {
    if (!e.title) e.title = 'This did not convert (undefined or unsupported)';
  });

  // Footnotes move to a list after the text; the mark links to its entry.
  var notes = article.querySelectorAll('.ltx_note.ltx_role_footnote');
  if (notes.length) {
    var sec = el('section', 'm-notes');
    sec.id = 'm-notes';
    sec.appendChild(el('h2', 'ltx_title ltx_title_section', 'Notes'));
    var list = el('ol');
    Array.prototype.forEach.call(notes, function (note, i) {
      var content = note.querySelector('.ltx_note_content');
      if (!content) return;
      var n = i + 1;
      if (!note.id) note.id = 'm-note-ref-' + n;
      var li = el('li');
      li.id = 'm-note-' + n;
      var copy = content.cloneNode(true);
      Array.prototype.forEach.call(
        copy.querySelectorAll('.ltx_note_mark, .ltx_tag_note, [id]'),
        function (x) {
          if (x.id) x.removeAttribute('id');
          else if (x.parentNode) x.parentNode.removeChild(x);
        },
      );
      while (copy.firstChild) li.appendChild(copy.firstChild);
      var back = el('a', null, '↩');
      back.href = '#' + note.id;
      back.setAttribute('aria-label', 'Back to the text');
      li.appendChild(back);
      list.appendChild(li);
      var mark = note.querySelector('.ltx_note_mark');
      if (mark) {
        var a = el('a', 'm-note-ref', mark.textContent);
        a.href = '#m-note-' + n;
        a.setAttribute('aria-label', 'Footnote ' + n);
        mark.parentNode.replaceChild(a, mark);
      }
    });
    sec.appendChild(list);
    var bib = article.querySelector('.ltx_bibliography');
    if (bib && bib.parentNode) bib.parentNode.insertBefore(sec, bib);
    else article.appendChild(sec);
    document.body.classList.add('m-notes-on');
  }

  // Proofs fold. The page carries each as an open <details>, so it reads
  // open with scripts off; here they start folded, open for print, and
  // open when a link or the app's sync lands inside one.
  var proofs = article.querySelectorAll('details.ltx_proof');
  function reveal(to) {
    for (var n = to; n && n !== article; n = n.parentNode) {
      if (n.tagName === 'DETAILS' && !n.open) n.open = true;
    }
  }
  Array.prototype.forEach.call(proofs, function (d) {
    d.open = false;
  });
  var landed = null;
  try {
    landed = location.hash && document.getElementById(decodeURIComponent(location.hash.slice(1)));
  } catch (_) {
    landed = null;
  }
  if (landed && article.contains(landed)) {
    reveal(landed);
    if (landed.scrollIntoView) landed.scrollIntoView();
  }
  var foldedForPrint = [];
  window.addEventListener('beforeprint', function () {
    Array.prototype.forEach.call(proofs, function (d) {
      if (!d.open) {
        foldedForPrint.push(d);
        d.open = true;
      }
    });
  });
  window.addEventListener('afterprint', function () {
    foldedForPrint.forEach(function (d) {
      d.open = false;
    });
    foldedForPrint = [];
  });

  // Following an in-page link flashes where it landed, so a citation, a
  // footnote mark or a figure reference shows what it pointed at.
  var flashed = null;
  article.addEventListener('click', function (ev) {
    var a = ev.target && ev.target.closest ? ev.target.closest('a[href^="#"]') : null;
    if (!a) return;
    var id;
    try {
      id = decodeURIComponent(a.getAttribute('href').slice(1));
    } catch (_) {
      return;
    }
    var to = id && document.getElementById(id);
    if (!to) return;
    reveal(to);
    if (flashed) flashed.classList.remove('m-flash');
    flashed = to;
    // :target already marks the first visit; the class covers a repeat click.
    to.classList.remove('m-flash');
    void to.offsetWidth;
    to.classList.add('m-flash');
    setTimeout(function () {
      to.classList.remove('m-flash');
    }, 1800);
  });

  // Editor-to-article sync, forward only. The app posts the caret's section
  // and the article scrolls to it with the same flash an in-page link gets.
  // Only the embedding parent is heard (e.source === parent): a widget frame
  // or a stray page cannot steer the article. The article never replies.
  // Up here, before the contents early-return below: a single-section
  // article has no nav, but its one anchor still syncs.
  window.addEventListener('message', function (e) {
    var d = e.data;
    if (!d || d.mfw !== 1 || e.source !== parent) return;
    if (d.type !== 'article-scroll' || typeof d.id !== 'string' || !d.id) return;
    var to = document.getElementById(d.id);
    if (!to) return;
    reveal(to);
    if (to.scrollIntoView) {
      try {
        to.scrollIntoView({ block: 'start' });
      } catch (_) {
        to.scrollIntoView();
      }
    }
    if (flashed) flashed.classList.remove('m-flash');
    flashed = to;
    to.classList.remove('m-flash');
    void to.offsetWidth;
    to.classList.add('m-flash');
    setTimeout(function () {
      to.classList.remove('m-flash');
    }, 1800);
  });

  // Contents: the list is in the page already (the article carries its own
  // `nav.m-contents`, which reads with scripts off). Here it only gains a
  // collapse on narrow screens and a marker on the section in view.
  // Defensive: the exporter omits the nav when paper.reader.contents is
  // false, but a preview reuse may carry one anyway.
  var nav = article.querySelector('nav.m-contents');
  if (nav && manifest.paper && manifest.paper.reader && manifest.paper.reader.contents === false) {
    nav.remove();
  }
  nav = article.querySelector('nav.m-contents');
  if (!nav) return;
  var links = {};
  var items = [];
  Array.prototype.forEach.call(nav.querySelectorAll('a[href^="#"]'), function (a) {
    var id = a.getAttribute('href').slice(1);
    var target = id && document.getElementById(id);
    if (!target) return;
    var h = target.querySelector('h2, h3, h4, h5, h6') || target;
    links[id] = a;
    items.push({ id: id, h: h });
  });
  if (!items.length) return;
  article.classList.add('m-has-contents');
  var wide = window.matchMedia('(min-width: 1100px)');
  var title = nav.querySelector('.m-contents-title');
  var toggle = null;
  if (title) {
    toggle = el('button', 'm-contents-toggle', title.textContent);
    toggle.type = 'button';
    toggle.setAttribute('aria-controls', 'm-contents-list');
    title.textContent = '';
    title.appendChild(toggle);
  }
  var list = nav.querySelector('ol');
  if (list) list.id = 'm-contents-list';
  function collapse(on) {
    nav.classList.toggle('m-collapsed', on);
    if (toggle) toggle.setAttribute('aria-expanded', on ? 'false' : 'true');
  }
  collapse(!wide.matches);
  wide.addEventListener('change', function () {
    collapse(!wide.matches);
  });
  if (toggle)
    toggle.addEventListener('click', function () {
      if (!wide.matches) collapse(!nav.classList.contains('m-collapsed'));
    });
  nav.addEventListener('click', function (ev) {
    if (!wide.matches && ev.target && ev.target.closest && ev.target.closest('a')) collapse(true);
  });

  // Scroll-spy: the last heading above a line a quarter down the screen.
  var current = null;
  var queued = false;
  function spy() {
    queued = false;
    var line = window.innerHeight * 0.25;
    var hit = items[0];
    for (var i = 0; i < items.length; i++) {
      if (items[i].h.getBoundingClientRect().top <= line) hit = items[i];
      else break;
    }
    if (current === hit.id) return;
    if (current && links[current]) {
      links[current].classList.remove('m-active');
      links[current].removeAttribute('aria-current');
    }
    current = hit.id;
    var a = links[current];
    a.classList.add('m-active');
    a.setAttribute('aria-current', 'location');
    if (wide.matches && nav.scrollHeight > nav.clientHeight) {
      nav.scrollTop = Math.max(0, a.offsetTop - nav.clientHeight / 2);
    }
  }
  window.addEventListener(
    'scroll',
    function () {
      if (!queued) {
        queued = true;
        requestAnimationFrame(spy);
      }
    },
    { passive: true },
  );
  window.addEventListener('resize', spy);
  spy();
})();
