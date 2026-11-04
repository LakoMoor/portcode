/* Light/dark theme: follows the system until the visitor picks one with the button in the nav. */
(function () {
  var k = 'pc-theme', d = document.documentElement, t;
  try { t = localStorage.getItem(k); } catch (e) {}
  if (t !== 'dark' && t !== 'light') t = matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  d.dataset.theme = t;
  addEventListener('DOMContentLoaded', function () {
    var n = document.querySelector('nav .wrap'); if (!n) return;
    var b = document.createElement('button');
    b.className = 'theme'; b.type = 'button'; b.setAttribute('aria-label', 'Switch between light and dark');
    b.innerHTML = '<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path class="moon" d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z"/><g class="sun"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/></g></svg>';
    b.onclick = function () {
      var next = d.dataset.theme === 'dark' ? 'light' : 'dark';
      d.dataset.theme = next;
      try { localStorage.setItem(k, next); } catch (e) {}
    };
    n.insertBefore(b, n.querySelector('.gh'));
  });
})();
