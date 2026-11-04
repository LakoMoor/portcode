/* Replay of a real PortCode run (events from scripts/build_demo.py) and a clickable /create. */
(function () {
  var root = document.getElementById('demo');
  if (!root || !window.PC_DEMO) return;
  var L = root.dataset.lang === 'ru' ? {
    play: 'Пуск', pause: 'Пауза', again: 'Заново', evid: 'Доказательства', todo: 'Дела', done: 'Готово'
  } : { play: 'Play', pause: 'Pause', again: 'Replay', evid: 'Evidence', todo: 'To do', done: 'Done' };
  var S = window.PC_DEMO, log = root.querySelector('.dlog'), side = root.querySelector('.dside');
  var btn = root.querySelector('.dplay'), bar = root.querySelector('.dbar'), spd = root.querySelector('.dspeed');
  var i = 0, timer = null, playing = false, speed = 1, started = false;
  var reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
  function el(t, c, x) { var e = document.createElement(t); if (c) e.className = c; if (x != null) e.textContent = x; return e; }
  function scroll() { log.scrollTop = log.scrollHeight; }
  function sleep(ms, f) { timer = setTimeout(f, reduce ? 0 : ms / speed); }
  function sideTodos(items) {
    side.querySelector('.dtodo').replaceChildren.apply(side.querySelector('.dtodo'), items.map(function (t) {
      var r = el('div', t[1] ? 'ok' : ''); r.append(el('b', '', t[1] ? '✓' : '○'), el('span', '', t[0])); return r; }));
  }
  var ev = 0;
  function type(node, text, cb) {
    if (reduce) { node.textContent = text; return cb(); }
    var n = 0; (function t() { n += 3; node.textContent = text.slice(0, n); scroll();
      if (n < text.length) timer = setTimeout(t, 16 / speed); else cb(); })();
  }
  function step() {
    if (!playing) return;
    bar.style.width = (i / S.length * 100) + '%';
    if (i >= S.length) { playing = false; btn.textContent = L.again; bar.style.width = '100%'; return; }
    var s = S[i++];
    if (s.k === 'user') {
      var m = el('div', 'dmsg u'); log.append(m); type(m, s.text, function () { sleep(500, step); });
    } else if (s.k === 'say') {
      var a = el('div', 'dmsg a'); log.append(a); type(a, s.text, function () { sleep(350, step); });
    } else if (s.k === 'tool') {
      var c = el('div', 'dtool run'); c.append(el('b', '', '…'), el('code', '', s.name + (s.arg ? ' ' + s.arg : '')));
      log.append(c); scroll();
      sleep(700, function () {
        c.className = 'dtool ' + (s.ok ? 'ok' : 'bad'); c.firstChild.textContent = s.ok ? '✓' : '✗';
        var o = el('pre', '', s.out.join('\n')); c.append(o);
        if (/E-\d+/.test(s.out.join(' '))) { ev++; side.querySelector('.dev').textContent = ev; }
        scroll(); sleep(450, step);
      });
    } else if (s.k === 'todos') { sideTodos(s.items); sleep(300, step); }
  }
  function reset() {
    clearTimeout(timer); i = 0; ev = 0; log.replaceChildren(); bar.style.width = '0';
    side.querySelector('.dev').textContent = '0';
    sideTodos(['pin + survey original', 'golden cases (6)', 'parity PASS', 'screenshots'].map(function (x) { return [x, false]; }));
  }
  function play() {
    if (i >= S.length) reset();
    playing = true; started = true; btn.textContent = L.pause; step();
  }
  function pause() { playing = false; clearTimeout(timer); btn.textContent = L.play; }
  btn.addEventListener('click', function () { playing ? pause() : play(); });
  spd.addEventListener('click', function () { speed = speed === 1 ? 2 : speed === 2 ? 4 : 1; spd.textContent = speed + '×'; });
  reset();
  if ('IntersectionObserver' in window) {
    new IntersectionObserver(function (es, o) { if (es[0].isIntersecting && !started) { play(); o.disconnect(); } }, { threshold: .5 }).observe(root);
  }
  if (reduce) { play(); }

  /* /create: pick answers, see the prompt it would send */
  var w = document.getElementById('wiz'); if (!w) return;
  var out = w.querySelector('.wprompt'), st = {};
  function upd() {
    var g = st.goal || 'port', k = st.kind || 'website', cl = st.close || 'clean-room';
    var agent = g === 'look' ? 'reverse' : (k === 'idea' ? 'build' : 'port');
    var deep = g === 'look' || cl === 'deep';
    out.textContent = 'agent: ' + agent + '\nmode: ' + (deep ? 'deep (may look inside)' : 'clean-room (outside only)') +
      '\nsource: ' + k + (g === 'look' ? '\nresult: docs/SPEC.md + evidence ledger, no Rust code' : '\nstack: ' + (st.stack || 'Rust, std only')) +
      '\n\n> ' + (g === 'look' ? 'Find out how this ' + k + ' works. Write docs/SPEC.md in your own words.' :
      'Recreate this ' + k + ' in Rust. Pin it, record facts, port one module at a time, and prove parity.');
  }
  w.querySelectorAll('[data-k]').forEach(function (b) {
    b.addEventListener('click', function () {
      st[b.dataset.k] = b.dataset.v;
      w.querySelectorAll('[data-k="' + b.dataset.k + '"]').forEach(function (x) { x.classList.toggle('on', x === b); });
      upd();
    });
  });
  upd();
})();
