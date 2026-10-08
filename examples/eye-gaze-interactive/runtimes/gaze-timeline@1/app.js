// gaze-timeline@1: one context size k drives three linked views, the scanpath
// of the last k fixations over the scene frame, the query those fixations
// build, and the metric curve measured over k. Sources arrive as bytes in the
// host's `init`; nothing is fetched.
(function () {
  'use strict';

  var $ = function (id) {
    return document.getElementById(id);
  };
  var root = $('root');
  var frameImg = $('frame');
  var overlay = $('overlay');
  var curveSvg = $('curve');
  var slider = $('k');
  var state = { k: 8, exp: 'E1', trace: null, rows: [], ready: false, timer: 0, sysOpen: false };
  var FRAMES = { E1: 919, E2: 237 };

  function post(msg) {
    msg.mfw = 1;
    window.parent.postMessage(msg, '*');
  }
  function fail(message) {
    var m = $('msg');
    m.hidden = false;
    m.textContent = message;
    post({ type: 'status', state: 'error', message: message.slice(0, 200) });
  }
  function esc(s) {
    return String(s).replace(/[&<>"']/g, function (c) {
      return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c];
    });
  }
  function token(name, fallback) {
    var v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
    return v || fallback;
  }
  function applyTheme(t) {
    var tokens = (t && t.tokens) || {};
    for (var k in tokens) {
      if (Object.prototype.hasOwnProperty.call(tokens, k) && k.indexOf('--m-') === 0) {
        document.documentElement.style.setProperty(k, tokens[k]);
      }
    }
    if (state.ready) render();
  }
  function text(src) {
    return new TextDecoder('utf-8').decode(src.bytes);
  }
  function parseCsv(s) {
    var lines = s.replace(/\r/g, '').trim().split('\n');
    var head = lines[0].split(',');
    return lines.slice(1).map(function (line) {
      var cells = line.split(',');
      var row = {};
      head.forEach(function (h, i) {
        var v = cells[i];
        row[h] = v === '' || v === undefined ? null : isNaN(Number(v)) ? v : Number(v);
      });
      return row;
    });
  }
  function fmt(v) {
    return v === null || v === undefined ? '–' : v.toFixed(1);
  }

  // ---- scene: the frame and the scanpath of the last k fixations ----------
  function renderScene() {
    var t = state.trace;
    var k = state.k;
    var fx = t.fixations;
    var shown = fx.slice(0, Math.min(k, fx.length));
    var ink = '#FFD24A';
    var out = [];
    overlay.setAttribute('viewBox', '0 0 1000 1000');
    overlay.setAttribute('preserveAspectRatio', 'none');
    // The path runs oldest to newest, ending at the current gaze.
    var pts = [];
    for (var i = shown.length - 1; i >= 0; i--) {
      if (shown[i].x !== null) pts.push([shown[i].x * 1000, shown[i].y * 1000]);
    }
    pts.push([t.current.x * 1000, t.current.y * 1000]);
    if (pts.length > 1) {
      var d = pts.map(function (p) {
        return p[0].toFixed(1) + ',' + p[1].toFixed(1);
      });
      out.push(
        '<polyline points="' +
          d.join(' ') +
          '" fill="none" stroke="#000" stroke-opacity="0.55" stroke-width="9" stroke-linejoin="round"/>',
      );
      out.push(
        '<polyline points="' +
          d.join(' ') +
          '" fill="none" stroke="' +
          ink +
          '" stroke-width="4" stroke-linejoin="round" stroke-dasharray="14 8"/>',
      );
    }
    for (var j = shown.length - 1; j >= 0; j--) {
      var f = shown[j];
      if (f.x === null) continue;
      var age = shown.length > 1 ? j / (fx.length - 1) : 0;
      var op = (1 - 0.55 * age).toFixed(2);
      var x = f.x * 1000;
      var y = f.y * 1000;
      out.push(
        '<g opacity="' +
          op +
          '"><circle cx="' +
          x +
          '" cy="' +
          y +
          '" r="24" fill="' +
          ink +
          '" stroke="#000" stroke-width="3"/>' +
          '<text x="' +
          x +
          '" y="' +
          (y + 9) +
          '" text-anchor="middle" font-size="26" font-weight="700" fill="#000" font-family="sans-serif">' +
          (j + 1) +
          '</text></g>',
      );
    }
    // The current fixation: what the model is asked to name.
    var cx = t.current.x * 1000;
    var cy = t.current.y * 1000;
    out.push(
      '<g><circle cx="' +
        cx +
        '" cy="' +
        cy +
        '" r="40" fill="none" stroke="#000" stroke-opacity="0.6" stroke-width="10"/>' +
        '<circle cx="' +
        cx +
        '" cy="' +
        cy +
        '" r="40" fill="none" stroke="#fff" stroke-width="5"/>' +
        '<path d="M' +
        (cx - 58) +
        ',' +
        cy +
        'h30M' +
        (cx + 28) +
        ',' +
        cy +
        'h30M' +
        cx +
        ',' +
        (cy - 58) +
        'v30M' +
        cx +
        ',' +
        (cy + 28) +
        'v30" stroke="#fff" stroke-width="5"/>' +
        '<text x="' +
        (cx + 50) +
        '" y="' +
        (cy - 46) +
        '" font-size="30" font-weight="700" fill="#fff" stroke="#000" stroke-width="6" paint-order="stroke" font-family="sans-serif">now</text></g>',
    );
    overlay.innerHTML = out.join('');
    var off = [];
    shown.forEach(function (f, n) {
      if (f.x === null) off.push('<b>' + (n + 1) + '</b> ' + esc(f.label));
    });
    $('offframe').innerHTML = off.length ? 'Not in this view: ' + off.join(', ') : '&nbsp;';
  }

  // ---- the query those fixations build ------------------------------------
  function renderPrompt() {
    var t = state.trace;
    var k = state.k;
    var fx = t.fixations;
    var listed = fx.slice(0, Math.min(k, fx.length)).map(function (f, n) {
      return n === k - 1 ? '<span class="new">' + esc(f.label) + '</span>' : esc(f.label);
    });
    var ctx = '';
    if (k === 1) ctx = 'The past object I have fixated on is: ' + listed[0] + '.';
    else if (k > 1)
      ctx =
        'The past ' +
        k +
        ' objects I have fixated on, in order from most recent to least recent, are: ' +
        listed.join(', ') +
        '.';
    if (k > fx.length)
      ctx += ' <span class="dim">(the published example lists ' + fx.length + ')</span>';
    var pub = t.published;
    var answer =
      k === pub.context && state.exp === 'E1'
        ? '<p><span class="role">Ground truth:</span> ' +
          esc(pub.truth) +
          ' · <span class="role">Response:</span> <span class="ok">' +
          esc(pub.response) +
          ' ✓</span></p>'
        : '<p class="dim">The published example shows the response at ' +
          pub.context +
          ' fixations (E1).</p>';
    $('prompt').innerHTML =
      '<details' +
      (state.sysOpen ? ' open' : '') +
      '><summary>System message</summary><p>' +
      esc(t.system) +
      ' <span class="dim">Visible objects: ' +
      esc(t.visible.join(', ')) +
      '</span></p></details>' +
      '<p><span class="role">User:</span> ' +
      esc(t.questions[state.exp] || '') +
      (ctx ? ' <span class="ctx">' + ctx + '</span>' : ' <span class="dim">(image only)</span>') +
      '</p>' +
      answer;
  }

  // The prompt keeps the height of its longest state at this width, so the
  // widget does not change height under a finger dragging the slider.
  function reservePrompt() {
    var p = $('prompt');
    var k = state.k;
    var exp = state.exp;
    var most = 0;
    p.style.minHeight = '';
    ['E1', 'E2'].forEach(function (e) {
      state.exp = e;
      for (var i = 0; i <= 10; i++) {
        state.k = i;
        renderPrompt();
        most = Math.max(most, p.offsetHeight);
      }
    });
    state.k = k;
    state.exp = exp;
    renderPrompt();
    p.style.minHeight = most + 'px';
  }

  // ---- the metric curve over k ---------------------------------------------
  var geom = null;
  function renderCurve() {
    var rows = state.rows.filter(function (r) {
      return r.experiment === state.exp;
    });
    drawnWidth = curveSvg.clientWidth;
    var w = Math.max(120, curveSvg.clientWidth);
    var h = Math.max(60, curveSvg.clientHeight);
    var m = { l: 30, r: 8, t: 6, b: 16 };
    var ymax = 0;
    rows.forEach(function (r) {
      ['vlm', 'ci_high', 'greedy', 'random_prior'].forEach(function (c) {
        if (r[c] !== null && r[c] > ymax) ymax = r[c];
      });
    });
    ymax = Math.ceil(ymax / 10) * 10;
    var X = function (k) {
      return m.l + (k / 10) * (w - m.l - m.r);
    };
    var Y = function (v) {
      return h - m.b - (v / ymax) * (h - m.t - m.b);
    };
    geom = { X: X, m: m, w: w };
    var ink = token('--m-figure-ink', '#1e1b24');
    var blue = token('--m-cat-1', '#2a78d6');
    var orange = token('--m-cat-2', '#eb6834');
    var green = token('--m-cat-3', '#1baf7a');
    var o = [];
    for (var g = 0; g <= ymax; g += ymax > 40 ? 20 : 10) {
      o.push(
        '<line x1="' +
          m.l +
          '" x2="' +
          (w - m.r) +
          '" y1="' +
          Y(g) +
          '" y2="' +
          Y(g) +
          '" stroke="' +
          ink +
          '" stroke-opacity="0.12"/>',
      );
      o.push(
        '<text x="' +
          (m.l - 4) +
          '" y="' +
          (Y(g) + 4) +
          '" text-anchor="end" font-size="10" fill="' +
          ink +
          '">' +
          g +
          '%</text>',
      );
    }
    for (var t = 0; t <= 10; t++) {
      o.push(
        '<text x="' +
          X(t) +
          '" y="' +
          (h - 3) +
          '" text-anchor="middle" font-size="10" fill="' +
          ink +
          '">' +
          (t === 0 ? 'None' : t) +
          '</text>',
      );
    }
    var band = rows.filter(function (r) {
      return r.ci_low !== null;
    });
    var up = band.map(function (r) {
      return X(r.fixations) + ',' + Y(r.ci_high);
    });
    var down = band
      .slice()
      .reverse()
      .map(function (r) {
        return X(r.fixations) + ',' + Y(r.ci_low);
      });
    o.push(
      '<polygon points="' +
        up.concat(down).join(' ') +
        '" fill="' +
        blue +
        '" fill-opacity="0.18"/>',
    );
    function line(col, color, dash) {
      var p = rows
        .filter(function (r) {
          return r[col] !== null;
        })
        .map(function (r) {
          return X(r.fixations) + ',' + Y(r[col]);
        });
      return (
        '<polyline points="' +
        p.join(' ') +
        '" fill="none" stroke="' +
        color +
        '" stroke-width="' +
        (dash ? 1.6 : 2.2) +
        '"' +
        (dash ? ' stroke-dasharray="5 4"' : '') +
        '/>'
      );
    }
    o.push(line('greedy', orange, true));
    o.push(line('random_prior', green, true));
    o.push(line('vlm', blue, false));
    var cur = rows.filter(function (r) {
      return r.fixations === state.k;
    })[0];
    if (cur) {
      o.push(
        '<line x1="' +
          X(cur.fixations) +
          '" x2="' +
          X(cur.fixations) +
          '" y1="' +
          m.t +
          '" y2="' +
          (h - m.b) +
          '" stroke="' +
          ink +
          '" stroke-opacity="0.5" stroke-dasharray="2 3"/>',
      );
      o.push(
        '<circle cx="' +
          X(cur.fixations) +
          '" cy="' +
          Y(cur.vlm) +
          '" r="5" fill="' +
          blue +
          '" stroke="' +
          token('--m-figure-bg', '#fff') +
          '" stroke-width="2"/>',
      );
    }
    curveSvg.setAttribute('viewBox', '0 0 ' + w + ' ' + h);
    curveSvg.innerHTML = o.join('');
    var base = rows.filter(function (r) {
      return r.fixations === 0;
    })[0];
    curveSvg.setAttribute(
      'aria-label',
      'VLM accuracy by number of prior fixations for ' + state.exp,
    );
    $('readout').innerHTML = cur
      ? '<b style="color:' +
        blue +
        '">VLM ' +
        fmt(cur.vlm) +
        '%</b> [' +
        fmt(cur.ci_low) +
        ', ' +
        fmt(cur.ci_high) +
        ']<span class="extra"> over ' +
        FRAMES[state.exp] +
        ' frames</span>' +
        (state.k > 0 && base
          ? ' · ' + (cur.vlm / base.vlm).toFixed(1) + '× image only'
          : ' · image only') +
        '<span class="extra"> · <span style="color:' +
        orange +
        '">greedy ' +
        fmt(cur.greedy) +
        '%</span></span>'
      : '';
  }

  function render() {
    slider.value = String(state.k);
    $('kout').textContent = String(state.k);
    Array.prototype.forEach.call(document.querySelectorAll('.seg button'), function (b) {
      b.setAttribute('aria-checked', String(b.getAttribute('data-exp') === state.exp));
    });
    renderScene();
    renderPrompt();
    renderCurve();
  }
  function setK(k) {
    state.k = Math.max(0, Math.min(10, k));
    render();
  }
  function stop() {
    window.clearInterval(state.timer);
    state.timer = 0;
    $('play').textContent = '▶ Play';
  }

  $('prompt').addEventListener(
    'toggle',
    function (e) {
      // A re-render of an open <details> fires toggle too: only a change counts.
      if (e.target.open === state.sysOpen) return;
      state.sysOpen = e.target.open;
      reservePrompt();
    },
    true,
  );
  slider.addEventListener('input', function () {
    stop();
    setK(Number(slider.value));
  });
  Array.prototype.forEach.call(document.querySelectorAll('.seg button'), function (b) {
    b.addEventListener('click', function () {
      state.exp = b.getAttribute('data-exp');
      render();
    });
  });
  $('play').addEventListener('click', function () {
    if (state.timer) return stop();
    $('play').textContent = '❚❚ Pause';
    setK(0);
    state.timer = window.setInterval(function () {
      if (state.k >= state.trace.fixations.length) return stop();
      setK(state.k + 1);
    }, 900);
  });
  function pick(e) {
    if (!geom) return;
    var r = curveSvg.getBoundingClientRect();
    var x = ((e.clientX - r.left) / r.width) * geom.w;
    var k = Math.round(((x - geom.m.l) / (geom.w - geom.m.l - geom.m.r)) * 10);
    stop();
    setK(k);
  }
  curveSvg.addEventListener('pointerdown', function (e) {
    curveSvg.setPointerCapture(e.pointerId);
    pick(e);
  });
  curveSvg.addEventListener('pointermove', function (e) {
    if (e.buttons) pick(e);
  });
  var resizeTimer = 0;
  var drawnWidth = 0;
  window.addEventListener('resize', function () {
    // Only a new width redraws: the height follows from it.
    if (curveSvg.clientWidth === drawnWidth) return;
    window.clearTimeout(resizeTimer);
    resizeTimer = window.setTimeout(function () {
      if (!state.ready) return;
      reservePrompt();
      renderCurve();
    }, 60);
  });

  function onInit(d) {
    applyTheme(d.theme);
    post({ type: 'status', state: 'loading' });
    var s = d.sources || {};
    if (!s.frame || !s.trace || !s.curve)
      return fail('the widget needs frame, trace and curve sources');
    try {
      state.trace = JSON.parse(text(s.trace));
      state.rows = parseCsv(text(s.curve));
    } catch (err) {
      return fail('a source could not be read: ' + (err && err.message ? err.message : err));
    }
    var o = d.options || {};
    if (o.experiment === 'E1' || o.experiment === 'E2') state.exp = o.experiment;
    if (typeof o.context === 'number') state.k = o.context;
    root.setAttribute('aria-label', d.alt || 'Gaze timeline');
    var url = URL.createObjectURL(
      new Blob([s.frame.bytes], { type: s.frame.mime || 'image/jpeg' }),
    );
    frameImg.onload = function () {
      state.ready = true;
      root.removeAttribute('aria-busy');
      render();
      reservePrompt();
      post({ type: 'status', state: 'loaded' });
    };
    frameImg.onerror = function () {
      fail('the frame could not be shown');
    };
    frameImg.src = url;
  }

  window.addEventListener('message', function (e) {
    if (e.source !== window.parent) return;
    var d = e.data;
    if (!d || d.mfw !== 1) return;
    if (d.type === 'init') onInit(d);
    else if (d.type === 'theme') applyTheme(d); // the host sends {mode, tokens} at the top level
  });
  post({ type: 'ready' });
})();
