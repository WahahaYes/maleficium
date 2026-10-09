// gaze-curves@1: a line chart whose controls, legend and live readout sit
// outside the plot. View `context`: accuracy against the number of prior
// fixations given as context, with a CI band and baselines. View `coverage`:
// the eye-tracking accuracy needed to place gaze on a share of objects, with
// a tracker-error line and the share it covers. Data arrives as bytes in the
// host's `init`; nothing is fetched.
(function () {
  'use strict';

  var $ = function (id) {
    return document.getElementById(id);
  };
  var root = $('root');
  var plot = $('plot');
  var tip = $('tip');
  var state = {
    view: 'context',
    rows: [],
    pick: 6,
    hidden: {},
    ci: true,
    exp: 'E1',
    measure: 'radius',
    acc: 3,
    ready: false,
  };
  var geom = null;

  // ---- host bridge -------------------------------------------------------
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
  function applyTheme(t) {
    var tokens = (t && t.tokens) || {};
    for (var k in tokens) {
      if (Object.prototype.hasOwnProperty.call(tokens, k) && k.indexOf('--m-') === 0) {
        document.documentElement.style.setProperty(k, tokens[k]);
      }
    }
    if (state.ready) render();
  }
  function token(name, fallback) {
    var v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
    return v || fallback;
  }
  function esc(s) {
    return String(s).replace(/[&<>"']/g, function (c) {
      return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c];
    });
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
  function f1(v) {
    return v === null || v === undefined ? '–' : v.toFixed(1);
  }
  function cat(i) {
    return token('--m-cat-' + i, ['#2a78d6', '#eb6834', '#1baf7a', '#eda100'][i - 1]);
  }
  function swatch(color) {
    return '<span class="sw" style="background:' + color + '"></span>';
  }

  // ---- the two views -----------------------------------------------------
  var CONTEXT = [
    { key: 'vlm', label: 'VLM (Llama 3.2 90B)', c: 1, dash: false },
    { key: 'greedy', label: 'Greedy (most fixated)', short: 'Greedy', c: 2, dash: true },
    {
      key: 'random_prior',
      label: 'Random (prior fixations)',
      short: 'Random, prior',
      c: 3,
      dash: true,
    },
    {
      key: 'random_visible',
      label: 'Random (visible objects)',
      short: 'Random, visible',
      c: 4,
      dash: true,
    },
  ];
  var SPACES = [
    { key: 'near', label: 'Near-field (≤ 1 m)', c: 1 },
    { key: 'mid', label: 'Mid-field (1–2 m)', c: 2 },
    { key: 'interacted', label: 'Interacted', c: 3 },
    { key: 'fixated', label: 'Fixated (≤ 2 m)', c: 4 },
  ];
  var FRAMES = { E1: 919, E2: 237 };

  function contextModel() {
    var rows = state.rows.filter(function (r) {
      return r.experiment === state.exp;
    });
    var at = function (k) {
      return rows.filter(function (r) {
        return r.fixations === k;
      })[0];
    };
    var ymax = 0;
    rows.forEach(function (r) {
      ['ci_high', 'greedy', 'random_prior', 'vlm'].forEach(function (c) {
        if (r[c] !== null && r[c] > ymax) ymax = r[c];
      });
    });
    ymax = Math.ceil(ymax / 10) * 10;
    var ticks = [];
    for (var k = 0; k <= 10; k++) ticks.push({ v: k, label: k === 0 ? 'None' : String(k) });
    var yt = [];
    for (var y = 0; y <= ymax; y += ymax > 40 ? 10 : 5) yt.push(y);
    return {
      x0: 0,
      x1: 10,
      step: 1,
      xTicks: ticks,
      xTitle: 'Prior fixations given as context',
      yMax: ymax,
      yTicks: yt,
      yFmt: function (v) {
        return v + '%';
      },
      yTitle: 'Accuracy',
      band:
        state.ci && !state.hidden.vlm
          ? {
              color: cat(1),
              pts: rows.map(function (r) {
                return [r.fixations, r.ci_low, r.ci_high];
              }),
            }
          : null,
      series: CONTEXT.map(function (s) {
        return {
          key: s.key,
          label: s.label,
          color: cat(s.c),
          dash: s.dash,
          dots: !s.dash,
          pts: rows
            .filter(function (r) {
              return r[s.key] !== null;
            })
            .map(function (r) {
              return [r.fixations, r[s.key]];
            }),
        };
      }),
      at: at,
      hline: null,
    };
  }

  function coverageModel() {
    var rows = state.rows.filter(function (r) {
      return r.measure === state.measure;
    });
    var ticks = [95, 75, 50, 25, 5].map(function (v) {
      return { v: v, label: v + '%' };
    });
    return {
      x0: 95,
      x1: 5,
      step: 1,
      xTicks: ticks,
      xTitle: 'Share of objects at least this large',
      yMax: 12,
      yTicks: [0, 2, 4, 6, 8, 10, 12],
      yFmt: function (v) {
        return v + '°';
      },
      yTitle: 'Visual size (accuracy needed)',
      band: null,
      series: SPACES.map(function (s) {
        return {
          key: s.key,
          label: s.label,
          color: cat(s.c),
          dash: false,
          dots: false,
          pts: rows
            .filter(function (r) {
              return r.space === s.key;
            })
            .map(function (r) {
              return [r.pct, r.deg];
            }),
        };
      }),
      hline: { y: state.acc, color: token('--m-figure-ink', '#1e1b24') },
      rows: rows,
    };
  }
  function model() {
    return state.view === 'context' ? contextModel() : coverageModel();
  }
  // The share of objects at least `acc` degrees: the largest percentile
  // whose visual size still reaches it.
  function coverage(rows, space, acc) {
    var best = null;
    rows.forEach(function (r) {
      if (r.space === space && r.deg !== null && r.deg >= acc && (best === null || r.pct > best))
        best = r.pct;
    });
    return best;
  }
  function sizeAt(rows, space, pct) {
    var r = rows.filter(function (x) {
      return x.space === space && x.pct === pct;
    })[0];
    return r ? r.deg : null;
  }

  // ---- drawing -----------------------------------------------------------
  function draw(m) {
    drawnWidth = plot.clientWidth;
    var w = Math.max(160, plot.clientWidth);
    var h = Math.max(90, plot.clientHeight);
    var pad = { l: 44, r: 10, t: 8, b: 34 };
    var X = function (v) {
      return pad.l + ((v - m.x0) / (m.x1 - m.x0)) * (w - pad.l - pad.r);
    };
    var Y = function (v) {
      return h - pad.b - (Math.min(v, m.yMax) / m.yMax) * (h - pad.t - pad.b);
    };
    geom = { m: m, X: X, w: w, pad: pad };
    plot.setAttribute('aria-valuemin', Math.min(m.x0, m.x1));
    plot.setAttribute('aria-valuemax', Math.max(m.x0, m.x1));
    plot.setAttribute('aria-valuenow', state.pick);
    plot.setAttribute(
      'aria-valuetext',
      state.view === 'context'
        ? state.pick === 0
          ? 'Image only'
          : state.pick + ' prior fixations'
        : state.pick + '% of objects',
    );
    var ink = token('--m-figure-ink', '#1e1b24');
    var o = [];
    o.push(
      '<defs><clipPath id="clip"><rect x="' +
        pad.l +
        '" y="' +
        pad.t +
        '" width="' +
        (w - pad.l - pad.r) +
        '" height="' +
        (h - pad.t - pad.b) +
        '"/></clipPath></defs>',
    );
    m.yTicks.forEach(function (v) {
      o.push(
        '<line x1="' +
          pad.l +
          '" x2="' +
          (w - pad.r) +
          '" y1="' +
          Y(v) +
          '" y2="' +
          Y(v) +
          '" stroke="' +
          ink +
          '" stroke-opacity="0.1"/>',
      );
      o.push(
        '<text x="' +
          (pad.l - 6) +
          '" y="' +
          (Y(v) + 4) +
          '" text-anchor="end" font-size="11" fill="' +
          ink +
          '">' +
          m.yFmt(v) +
          '</text>',
      );
    });
    m.xTicks.forEach(function (t) {
      o.push(
        '<text x="' +
          X(t.v) +
          '" y="' +
          (h - pad.b + 14) +
          '" text-anchor="middle" font-size="11" fill="' +
          ink +
          '">' +
          t.label +
          '</text>',
      );
    });
    o.push(
      '<line x1="' +
        pad.l +
        '" x2="' +
        (w - pad.r) +
        '" y1="' +
        (h - pad.b) +
        '" y2="' +
        (h - pad.b) +
        '" stroke="' +
        ink +
        '" stroke-opacity="0.4"/>',
    );
    o.push(
      '<text x="' +
        (pad.l + w - pad.r) / 2 +
        '" y="' +
        (h - 3) +
        '" text-anchor="middle" font-size="11.5" font-weight="700" fill="' +
        ink +
        '">' +
        esc(m.xTitle) +
        '</text>',
    );
    o.push(
      '<text transform="translate(11,' +
        (pad.t + h - pad.b) / 2 +
        ') rotate(-90)" text-anchor="middle" font-size="11.5" font-weight="700" fill="' +
        ink +
        '">' +
        esc(m.yTitle) +
        '</text>',
    );
    o.push('<g clip-path="url(#clip)">');
    if (m.band) {
      var b = m.band.pts.filter(function (p) {
        return p[1] !== null;
      });
      var up = b.map(function (p) {
        return X(p[0]) + ',' + Y(p[2]);
      });
      var dn = b
        .slice()
        .reverse()
        .map(function (p) {
          return X(p[0]) + ',' + Y(p[1]);
        });
      o.push(
        '<polygon points="' +
          up.concat(dn).join(' ') +
          '" fill="' +
          m.band.color +
          '" fill-opacity="0.18"/>',
      );
    }
    if (m.hline) {
      o.push(
        '<line x1="' +
          pad.l +
          '" x2="' +
          (w - pad.r) +
          '" y1="' +
          Y(m.hline.y) +
          '" y2="' +
          Y(m.hline.y) +
          '" stroke="' +
          m.hline.color +
          '" stroke-opacity="0.75" stroke-width="1.5" stroke-dasharray="6 4"/>',
      );
    }
    m.series.forEach(function (s) {
      if (state.hidden[s.key] || !s.pts.length) return;
      var p = s.pts
        .map(function (q) {
          return X(q[0]).toFixed(1) + ',' + Y(q[1]).toFixed(1);
        })
        .join(' ');
      o.push(
        '<polyline points="' +
          p +
          '" fill="none" stroke="' +
          s.color +
          '" stroke-width="' +
          (s.dash ? 2 : 2.6) +
          '"' +
          (s.dash ? ' stroke-dasharray="6 4"' : '') +
          ' stroke-linejoin="round"/>',
      );
      if (s.dots) {
        s.pts.forEach(function (q) {
          o.push(
            '<circle cx="' + X(q[0]) + '" cy="' + Y(q[1]) + '" r="3" fill="' + s.color + '"/>',
          );
        });
      }
    });
    o.push('</g>');
    // The picked x: a rule and a ring on every visible series.
    var px = X(state.pick);
    o.push(
      '<line x1="' +
        px +
        '" x2="' +
        px +
        '" y1="' +
        pad.t +
        '" y2="' +
        (h - pad.b) +
        '" stroke="' +
        ink +
        '" stroke-opacity="0.55" stroke-dasharray="2 3"/>',
    );
    m.series.forEach(function (s) {
      if (state.hidden[s.key]) return;
      var q = s.pts.filter(function (p) {
        return p[0] === state.pick;
      })[0];
      if (q && q[1] <= m.yMax) {
        o.push(
          '<circle cx="' +
            px +
            '" cy="' +
            Y(q[1]) +
            '" r="5" fill="' +
            token('--m-figure-bg', '#fff') +
            '" stroke="' +
            s.color +
            '" stroke-width="2.5"/>',
        );
      }
    });
    o.push(
      '<rect id="hover" x="0" y="' +
        pad.t +
        '" width="0" height="' +
        (h - pad.t - pad.b) +
        '" fill="' +
        ink +
        '" fill-opacity="0.06"/>',
    );
    plot.setAttribute('viewBox', '0 0 ' + w + ' ' + h);
    plot.innerHTML = o.join('');
  }

  // ---- controls, legend, readout ----------------------------------------
  function seg(label, items, current, onPick) {
    var g = document.createElement('span');
    g.className = 'group';
    g.innerHTML =
      '<span class="label">' +
      esc(label) +
      '</span><span class="seg" role="group" aria-label="' +
      esc(label) +
      '"></span>';
    var s = g.lastChild;
    items.forEach(function (it) {
      var b = document.createElement('button');
      b.type = 'button';
      b.innerHTML = it.html;
      if (it.title) b.title = it.title;
      b.setAttribute('aria-pressed', String(it.value === current));
      b.addEventListener('click', function () {
        onPick(it.value);
      });
      s.appendChild(b);
    });
    return g;
  }
  function toolbar() {
    var t = $('toolbar');
    t.textContent = '';
    if (state.view === 'context') {
      t.appendChild(
        seg(
          'Task',
          [
            {
              value: 'E1',
              html: 'E1<span class="long"> · what am I looking at?</span>',
              title: 'E1: What am I looking at?',
            },
            {
              value: 'E2',
              html: 'E2<span class="long"> · what will I interact with?</span>',
              title: 'E2: What am I going to interact with?',
            },
          ],
          state.exp,
          function (v) {
            state.exp = v;
            render();
          },
        ),
      );
      var c = document.createElement('label');
      c.className = 'check';
      c.innerHTML = '<input type="checkbox"' + (state.ci ? ' checked' : '') + '> 95% CI band';
      c.firstChild.addEventListener('change', function (e) {
        state.ci = e.target.checked;
        render();
      });
      t.appendChild(c);
    } else {
      t.appendChild(
        seg(
          'Measure',
          [
            {
              value: 'radius',
              html: 'Circular bound<span class="long"> (average)</span>',
              title: 'err ≤ √(A_seg / π)',
            },
            {
              value: 'minor',
              html: '½ minor axis<span class="long"> (conservative)</span>',
              title: 'err ≤ ½ L_min',
            },
          ],
          state.measure,
          function (v) {
            state.measure = v;
            render();
          },
        ),
      );
      var presets = seg(
        'Tracker error',
        [
          { value: 1, html: '1°', title: 'Lab-grade' },
          {
            value: 1.5,
            html: '1.5°<span class="long"> Aria</span>',
            title: 'Project Aria median error',
          },
          {
            value: 3,
            html: '3°<span class="long"> daily wear</span>',
            title: 'Daily-wear assumption in the paper',
          },
        ],
        state.acc,
        function (v) {
          state.acc = v;
          render();
        },
      );
      var r = document.createElement('span');
      r.className = 'range';
      r.innerHTML =
        '<input type="range" min="0.5" max="8" step="0.1" aria-label="Tracker error in degrees" value="' +
        state.acc +
        '"><output>' +
        state.acc.toFixed(1) +
        '°</output>';
      r.firstChild.addEventListener('input', function (e) {
        state.acc = Number(e.target.value);
        r.lastChild.textContent = state.acc.toFixed(1) + '°';
        Array.prototype.forEach.call(presets.querySelectorAll('button'), function (b) {
          b.setAttribute('aria-pressed', 'false');
        });
        draw(model());
        readout(model());
      });
      presets.appendChild(r);
      t.appendChild(presets);
    }
  }
  function legend(m) {
    var l = $('legend');
    l.textContent = '';
    m.series.forEach(function (s) {
      var b = document.createElement('button');
      b.type = 'button';
      b.setAttribute('aria-pressed', String(!state.hidden[s.key]));
      b.title = (state.hidden[s.key] ? 'Show ' : 'Hide ') + s.label;
      b.innerHTML =
        '<svg viewBox="0 0 22 8" aria-hidden="true"><line x1="1" x2="21" y1="4" y2="4" stroke="' +
        s.color +
        '" stroke-width="3"' +
        (s.dash ? ' stroke-dasharray="5 3"' : '') +
        '/></svg>' +
        esc(s.label);
      b.addEventListener('click', function () {
        state.hidden[s.key] = !state.hidden[s.key];
        render();
      });
      l.appendChild(b);
    });
  }
  function readout(m) {
    var r = $('readout');
    if (state.view === 'context') {
      var row = m.at(state.pick);
      var base = m.at(0);
      if (!row) {
        r.textContent = '';
        return;
      }
      var k = state.pick;
      var html =
        '<h3>' +
        (k === 0 ? 'Image only' : k + ' prior fixation' + (k === 1 ? '' : 's')) +
        '</h3>' +
        '<div class="big" style="color:' +
        cat(1) +
        '">' +
        f1(row.vlm) +
        '%</div>' +
        '<p>VLM accuracy<br><span class="muted">95% CI ' +
        f1(row.ci_low) +
        '–' +
        f1(row.ci_high) +
        '%</span>' +
        (k > 0 && base
          ? '<br><b>' +
            (row.vlm / base.vlm).toFixed(1) +
            '×</b> the image-only ' +
            f1(base.vlm) +
            '%'
          : '') +
        '</p><table>';
      CONTEXT.slice(1).forEach(function (s) {
        html +=
          '<tr><td>' +
          swatch(cat(s.c)) +
          esc(s.short) +
          '</td><td>' +
          (row[s.key] === null ? '–' : f1(row[s.key]) + '%') +
          '</td></tr>';
      });
      html +=
        '</table><p class="muted">' +
        FRAMES[state.exp] +
        ' frames · ' +
        (row.source === 'text' ? 'value stated in the paper' : 'read from the published figure') +
        '</p>';
      r.innerHTML = html;
    } else {
      var acc = state.acc;
      var h2 = '<h3>Placeable at ' + acc.toFixed(1) + '° error</h3><table>';
      SPACES.forEach(function (s) {
        var c = coverage(m.rows, s.key, acc);
        h2 +=
          '<tr><td>' +
          swatch(cat(s.c)) +
          esc(s.label) +
          '</td><td>' +
          (c === null ? '< 5%' : c >= 95 ? '≥ 95%' : c + '%') +
          '</td></tr>';
      });
      h2 += '</table><h3>Error needed for ' + state.pick + '%</h3><table>';
      SPACES.forEach(function (s) {
        var d = sizeAt(m.rows, s.key, state.pick);
        h2 +=
          '<tr><td>' +
          swatch(cat(s.c)) +
          esc(s.label) +
          '</td><td>' +
          (d === null ? '> 12°' : d.toFixed(2) + '°') +
          '</td></tr>';
      });
      h2 +=
        '</table><p class="muted">' +
        (state.measure === 'radius'
          ? 'Circular bound, average case'
          : '½ minor axis, conservative') +
        ' · read from the published figure</p>';
      r.innerHTML = h2;
    }
  }
  function render() {
    var m = model();
    toolbar();
    legend(m);
    draw(m);
    readout(m);
  }

  // ---- pointer: hover shows values, click or drag picks -----------------
  function xAt(e) {
    if (!geom) return null;
    var r = plot.getBoundingClientRect();
    var sx = ((e.clientX - r.left) / r.width) * geom.w;
    var m = geom.m;
    var v = m.x0 + ((sx - geom.pad.l) / (geom.w - geom.pad.l - geom.pad.r)) * (m.x1 - m.x0);
    var lo = Math.min(m.x0, m.x1);
    var hi = Math.max(m.x0, m.x1);
    return Math.max(lo, Math.min(hi, Math.round(v / m.step) * m.step));
  }
  function hover(e) {
    var x = xAt(e);
    var band = plot.querySelector('#hover');
    if (x === null || !band) return;
    var m = geom.m;
    var cx = geom.X(x);
    var bw = Math.abs(geom.X(x + m.step) - cx) || 10;
    band.setAttribute('x', cx - bw / 2);
    band.setAttribute('width', bw);
    var lines = [];
    var title;
    if (state.view === 'context') {
      title = x === 0 ? 'Image only' : x + ' prior fixation' + (x === 1 ? '' : 's');
      var row = m.at(x);
      CONTEXT.forEach(function (s) {
        if (!state.hidden[s.key] && row && row[s.key] !== null)
          lines.push(swatch(cat(s.c)) + esc(s.label) + ': ' + f1(row[s.key]) + '%');
      });
      if (row && state.ci)
        lines.push(
          '<span class="muted">95% CI ' + f1(row.ci_low) + '–' + f1(row.ci_high) + '%</span>',
        );
    } else {
      title = x + '% of objects';
      SPACES.forEach(function (s) {
        var d = sizeAt(m.rows, s.key, x);
        if (!state.hidden[s.key])
          lines.push(
            swatch(cat(s.c)) + esc(s.label) + ': ' + (d === null ? '> 12°' : d.toFixed(2) + '°'),
          );
      });
    }
    tip.innerHTML = '<b>' + title + '</b>' + lines.join('<br>');
    tip.hidden = false;
    var tx = e.clientX + 14;
    if (tx + tip.offsetWidth > window.innerWidth - 4) tx = e.clientX - tip.offsetWidth - 14;
    tip.style.left = tx + 'px';
    tip.style.top =
      Math.max(4, Math.min(e.clientY - 10, window.innerHeight - tip.offsetHeight - 4)) + 'px';
  }
  function pick(e) {
    var x = xAt(e);
    if (x === null || x === state.pick) return;
    state.pick = x;
    var m = model();
    draw(m);
    readout(m);
  }
  plot.addEventListener('pointerdown', function (e) {
    plot.setPointerCapture(e.pointerId);
    pick(e);
  });
  plot.addEventListener('pointermove', function (e) {
    if (e.buttons) pick(e);
    hover(e);
  });
  plot.addEventListener('pointerleave', function () {
    tip.hidden = true;
    var band = plot.querySelector('#hover');
    if (band) band.setAttribute('width', 0);
  });
  // Keyboard: the focused plot moves its pick with the arrow keys.
  plot.addEventListener('keydown', function (e) {
    if (!geom || state.pick === null || state.pick === undefined) return;
    var m = geom.m;
    var lo = Math.min(m.x0, m.x1);
    var hi = Math.max(m.x0, m.x1);
    var dir = m.x1 < m.x0 ? -1 : 1;
    var next = state.pick;
    if (e.key === 'ArrowRight' || e.key === 'ArrowUp') next += dir * m.step;
    else if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') next -= dir * m.step;
    else if (e.key === 'Home') next = dir > 0 ? lo : hi;
    else if (e.key === 'End') next = dir > 0 ? hi : lo;
    else return;
    e.preventDefault();
    next = Math.max(lo, Math.min(hi, next));
    if (next === state.pick) return;
    state.pick = next;
    var mm = model();
    draw(mm);
    readout(mm);
  });
  var resizeTimer = 0;
  var drawnWidth = 0;
  window.addEventListener('resize', function () {
    // Only a new width redraws: the height follows from it.
    if (plot.clientWidth === drawnWidth) return;
    window.clearTimeout(resizeTimer);
    resizeTimer = window.setTimeout(function () {
      if (state.ready) draw(model());
    }, 60);
  });

  function onInit(d) {
    applyTheme(d.theme);
    post({ type: 'status', state: 'loading' });
    var src = d.sources && d.sources.data;
    if (!src) return fail('the widget needs a data source');
    try {
      state.rows = parseCsv(new TextDecoder('utf-8').decode(src.bytes));
    } catch (err) {
      return fail('the data could not be read');
    }
    var o = d.options || {};
    if (o.view === 'context' || o.view === 'coverage') state.view = o.view;
    if (typeof o.pick === 'number') state.pick = o.pick;
    else if (state.view === 'coverage') state.pick = 50;
    if (typeof o.accuracy === 'number') state.acc = o.accuracy;
    root.setAttribute('aria-label', d.alt || 'Chart');
    plot.setAttribute('aria-label', d.alt || 'Chart');
    state.ready = true;
    root.removeAttribute('aria-busy');
    render();
    post({ type: 'status', state: 'loaded' });
  }
  window.addEventListener('message', function (e) {
    if (e.source !== window.parent) return;
    var d = e.data;
    if (!d || d.mfw !== 1) return;
    if (d.type === 'init') onInit(d);
    // A mode change carries {mode, tokens} at the top level.
    else if (d.type === 'theme') applyTheme(d);
  });
  post({ type: 'ready' });
})();
