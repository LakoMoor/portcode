/* Copy buttons for code blocks (<pre>) and the install command (.cmd). No dependencies. */
(function () {
  function text(el) {
    var t = (el.dataset.copy || el.innerText || el.textContent || '').replace(/ /g, ' ').trim();
    // the hero command shows a "$ " prompt that must not be copied
    return el.classList.contains('cmdtext') ? t.replace(/^\$\s*/, '') : t;
  }
  function copy(s, done) {
    if (navigator.clipboard && window.isSecureContext) {
      navigator.clipboard.writeText(s).then(done, fallback);
    } else fallback();
    function fallback() {
      var a = document.createElement('textarea');
      a.value = s; a.setAttribute('readonly', ''); a.style.cssText = 'position:fixed;opacity:0;top:0;left:0';
      document.body.appendChild(a); a.select();
      try { document.execCommand('copy'); done(); } catch (e) {}
      document.body.removeChild(a);
    }
  }
  function button(target) {
    var b = document.createElement('button');
    b.type = 'button'; b.className = 'copybtn'; b.textContent = 'Copy';
    b.setAttribute('aria-label', 'Copy to clipboard');
    b.onclick = function () {
      copy(text(target), function () {
        b.textContent = 'Copied'; b.classList.add('ok');
        clearTimeout(b._t);
        b._t = setTimeout(function () { b.textContent = 'Copy'; b.classList.remove('ok'); }, 1600);
      });
    };
    return b;
  }
  function init() {
    document.querySelectorAll('pre').forEach(function (pre) {
      if (pre.parentNode.classList && pre.parentNode.classList.contains('codebox')) return;
      var w = document.createElement('div'); w.className = 'codebox';
      pre.parentNode.insertBefore(w, pre); w.appendChild(pre); w.appendChild(button(pre));
    });
    document.querySelectorAll('.cmd').forEach(function (c) {
      var span = c.querySelector('span'); if (!span) return;
      span.classList.add('cmdtext');
      var b = button(span); c.appendChild(b);
    });
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();
})();
